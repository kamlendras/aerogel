use crate::config::ApiConfig;
use anyhow::{anyhow, Result};
use async_stream::stream;
use base64::{engine::general_purpose, Engine as _};
use mime_guess;
use reqwest::{multipart, Client};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::Path;
use std::pin::Pin;
use tokio::fs;
use tokio_stream::{Stream, StreamExt};

#[derive(Debug, Clone)]
pub struct Media {
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone)]
pub struct PromptData {
    pub text: String,
    pub media: Vec<Media>,
}

impl PromptData {
    pub async fn new(text: String, file_paths: &[String]) -> Result<Self> {
        let mut media_items = Vec::new();
        for path_str in file_paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(anyhow!("File not found: {}", path_str));
            }
            let mime_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            let file_content = fs::read(path).await?;
            let base64_data = general_purpose::STANDARD.encode(&file_content);
            media_items.push(Media {
                mime_type,
                data: base64_data,
            });
        }
        Ok(PromptData {
            text,
            media: media_items,
        })
    }
}

pub struct AiClient {
    client: Client,
    config: ApiConfig,
}

const SUPPORTED_AUDIO_TYPES: &[&str] = &[
    "audio/flac",
    "audio/m4a",
    "audio/mp3",
    "audio/mp4",
    "audio/mpeg",
    "audio/mpga",
    "audio/oga",
    "audio/ogg",
    "audio/wav",
    "audio/webm",
];

impl AiClient {
    pub fn new(config: ApiConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    async fn transcribe_audio_openai(&self, media: &Media) -> Result<String> {
        let api_key = self
            .config
            .get_key("openai")
            .ok_or_else(|| anyhow!("OpenAI API key not found"))?;

        let extension = match media.mime_type.as_str() {
            "audio/flac" => "flac",
            "audio/m4a" | "audio/x-m4a" | "audio/mp4" => "m4a", // Group common M4A types
            "audio/mp3" | "audio/mpeg" | "audio/mpga" => "mp3", // Group common MPEG types
            "audio/oga" | "audio/ogg" => "ogg",
            "audio/wav" | "audio/x-wav" => "wav",
            "audio/webm" => "webm",
            unsupported_mime => {
                return Err(anyhow!(
                    "Unsupported audio MIME type for OpenAI transcription: '{}'",
                    unsupported_mime
                ));
            }
        };

        let filename = format!("audio.{}", extension);

        let audio_bytes = general_purpose::STANDARD
            .decode(&media.data)
            .map_err(|e| anyhow!("Failed to decode base64 audio data: {}", e))?;

        let audio_part = multipart::Part::bytes(audio_bytes)
            .file_name(filename)
            .mime_str(&media.mime_type)?;

        let form = multipart::Form::new()
            .part("file", audio_part)
            .text("model", "whisper-1");

        let response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await?;
            return Err(anyhow!(
                "OpenAI Audio API Error ({}): {}",
                status,
                error_body
            ));
        }

        let json_body: Value = response.json().await?;
        let transcription = json_body["text"]
            .as_str()
            .ok_or_else(|| anyhow!("Failed to find 'text' in OpenAI transcription response"))?
            .to_string();

        Ok(transcription)
    }

    pub async fn chat_openai(
        &self,
        prompt_data: &PromptData,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let api_key = self
            .config
            .get_key("openai")
            .ok_or_else(|| anyhow!("OpenAI API key not found"))?;

        let mut transcribed_text = String::new();

        for media in &prompt_data.media {
            if SUPPORTED_AUDIO_TYPES.contains(&media.mime_type.as_str()) {
                print!("\n[OpenAI] Transcribing supported audio... ");
                io::stdout().flush()?;
                match self.transcribe_audio_openai(media).await {
                    Ok(transcript) => {
                        println!("Done.");
                        transcribed_text.push_str(&transcript);
                        transcribed_text.push_str("\n\n"); //
                    }
                    Err(e) => {
                        eprintln!("\n[OpenAI] Transcription failed: {}", e);
                        transcribed_text
                            .push_str(&format!("[Audio Transcription Failed: {}]\n\n", e));
                    }
                }
            }
        }

        let final_text = format!("{}{}", transcribed_text, prompt_data.text);
        let mut content_parts: Vec<Value> = Vec::new();

        content_parts.push(json!({
            "type": "text",
            "text": final_text
        }));

        for media in &prompt_data.media {
            if media.mime_type.starts_with("image/") {
                let image_url = format!("data:{};base64,{}", media.mime_type, media.data);
                content_parts.push(json!({
                    "type": "image_url",
                    "image_url": { "url": image_url }
                }));
            } else if !media.mime_type.starts_with("audio/") {
                eprintln!(
                    "\n[Warning (OpenAI Chat)]: Skipping media with unsupported MIME type '{}'.",
                    media.mime_type
                );
            }
        }

        let payload = json!({
            "model": &self.config.openai.model,
            "messages": [{"role": "user", "content": content_parts}],
            "max_tokens": self.config.openai.max_tokens,
            "temperature": self.config.openai.temperature,
            "top_p": self.config.openai.top_p,
            "stream": true
        });

        let response = self
            .client
            .post(&self.config.openai.api_base)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await?;
            return Err(anyhow!("OpenAI API Error ({}): {}", status, error_body));
        }

        let stream = response.bytes_stream();
        let s = stream! {
            for await chunk_result in stream {
                let chunk = chunk_result.map_err(|e| anyhow!("Stream error: {}", e))?;
                let s = String::from_utf8_lossy(&chunk);
                for line in s.split('\n') {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" { break; }
                        if let Ok(json) = serde_json::from_str::<Value>(data) {
                            if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                                yield Ok(content.to_string());
                            }
                        }
                    }
                }
            }
        };
        Ok(Box::pin(s))
    }

    pub async fn chat_gemini(
        &self,
        prompt_data: &PromptData,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let api_key = self
            .config
            .get_key("gemini")
            .ok_or_else(|| anyhow!("Gemini API key not found"))?;

        let mut parts: Vec<Value> = vec![json!({ "text": &prompt_data.text })];
        for media in &prompt_data.media {
            parts.push(json!({
                "inline_data": {
                    "mime_type": &media.mime_type,
                    "data": &media.data
                }
            }));
        }

        let payload = json!({
            "contents": [{"parts": parts}],
            "generationConfig": {
                "maxOutputTokens": self.config.gemini.max_tokens,
                "temperature": self.config.gemini.temperature,
                "topP": self.config.gemini.top_p,
            }
        });

        // Construct the streaming URL from the base URL in the config
        let base_url = self
            .config
            .gemini
            .api_base
            .replace("{model}", &self.config.gemini.model);

        let url = format!("{}:streamGenerateContent?key={}&alt=sse", base_url, api_key);

        let response = self.client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await?;
            return Err(anyhow!("Gemini API Error ({}): {}", status, error_body));
        }

        let stream = response.bytes_stream();
        let s = stream! {
            for await chunk_result in stream {
                let chunk = chunk_result.map_err(|e| anyhow!("Stream error from Gemini: {}", e))?;
                let s = String::from_utf8_lossy(&chunk);

                for line in s.lines() {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if let Ok(json_obj) = serde_json::from_str::<Value>(data) {
                            if let Some(text) = json_obj.pointer("/candidates/0/content/parts/0/text").and_then(Value::as_str) {
                                yield Ok(text.to_string());
                            }
                        }
                    }
                }
            }
        };
        Ok(Box::pin(s))
    }
    pub async fn chat_claude(
        &self,
        prompt_data: &PromptData,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let api_key = self
            .config
            .get_key("claude")
            .ok_or_else(|| anyhow!("Claude API key not found"))?;

        let mut content_parts: Vec<Value> = vec![json!({
            "type": "text",
            "text": &prompt_data.text
        })];

        for media in &prompt_data.media {
            if media.mime_type.starts_with("image/") {
                content_parts.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": &media.mime_type,
                        "data": &media.data,
                    }
                }));
            } else {
                eprintln!(
                    "\n[Warning (Claude)]: Skipping non-image file with MIME type '{}'.",
                    media.mime_type
                );
            }
        }

        let payload = json!({
            "model": &self.config.claude.model,
            "max_tokens": self.config.claude.max_tokens,
            "messages": [{"role": "user", "content": content_parts}],
            "temperature": self.config.claude.temperature,
            "top_p": self.config.claude.top_p,
            "stream": true
        });

        let response = self
            .client
            .post(&self.config.claude.api_base)
            .header("x-api-key", api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await?;
            return Err(anyhow!("Claude API Error ({}): {}", status, error_body));
        }

        let stream = response.bytes_stream();
        let s = stream! {
            for await chunk_result in stream {
                let chunk = chunk_result.map_err(|e| anyhow!("Stream error: {}", e))?;
                let s = String::from_utf8_lossy(&chunk);
                for line in s.split('\n') {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if let Ok(json) = serde_json::from_str::<Value>(data) {
                           if json["type"] == "content_block_delta" {
                                if let Some(content) = json["delta"]["text"].as_str() {
                                    yield Ok(content.to_string());
                                }
                           }
                        }
                    }
                }
            }
        };
        Ok(Box::pin(s))
    }

    pub async fn chat_xai(
        &self,
        prompt_data: &PromptData,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let api_key = self
            .config
            .get_key("xai")
            .ok_or_else(|| anyhow!("XAI API key not found"))?;

        if !prompt_data.media.is_empty() {
            eprintln!("\n[Warning (XAI/Grok)]: This model does not support media inputs. Ignoring all attached files.");
        }

        let payload = json!({
            "messages": [{"role": "user", "content": &prompt_data.text}],
            "model": &self.config.xai.model,
            "max_tokens": self.config.xai.max_tokens,
            "temperature": self.config.xai.temperature,
            "top_p": self.config.xai.top_p,
            "stream": true,
        });

        let response = self
            .client
            .post(&self.config.xai.api_base)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await?;
            return Err(anyhow!("XAI API Error ({}): {}", status, error_body));
        }

        let stream = response.bytes_stream();
        let s = stream! {
            for await chunk_result in stream {
                let chunk = chunk_result.map_err(|e| anyhow!("Stream error: {}", e))?;
                let s = String::from_utf8_lossy(&chunk);
                 for line in s.split('\n') {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" { break; }
                        if let Ok(json) = serde_json::from_str::<Value>(data) {
                            if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                                yield Ok(content.to_string());
                            }
                        }
                    }
                }
            }
        };
        Ok(Box::pin(s))
    }
}
