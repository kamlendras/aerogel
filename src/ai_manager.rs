mod ai_client;
mod config;

use crate::ai_client::{AiClient, PromptData};
use crate::config::ApiConfig;
use anyhow::Result;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::{self, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

async fn process_prompt(
    client: Arc<AiClient>,
    log_file: Arc<Mutex<std::fs::File>>,
    prompt_data: PromptData,
) -> Result<()> {
    {
        let mut file = log_file.lock().await;
        let timestamp = chrono::Local::now();
        if !prompt_data.media.is_empty() {
            writeln!(file, "\nMedia Files Attached: {}  ", prompt_data.media.len())?;
        }
    }

    let spawn_task = |model_name: &'static str,
                      call: Pin<
        Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>> + Send>,
    >| {
        let log_file_clone = Arc::clone(&log_file);
        tokio::spawn(async move {
            print!("{}: ", model_name);
            io::stdout().flush().unwrap();

            match call.await {
                Ok(mut stream) => {
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(content) => {
                                print!("{}", content);
                                io::stdout().flush().unwrap();

                                let mut file = log_file_clone.lock().await;
                                write!(file, "{}", &content).unwrap();
                            }
                            Err(e) => {
                                let err_msg =
                                    format!("\nError streaming {} response: {}", model_name, e);
                                eprint!("{}", err_msg);
                                // Also log the error to the file
                                let mut file = log_file_clone.lock().await;
                                writeln!(file, "{}", err_msg).unwrap();
                                break;
                            }
                        }
                    }

                    println!();

                    let mut file = log_file_clone.lock().await;
                    writeln!(file).unwrap();
                }
                Err(e) => {
                    let err_msg = format!("Error calling {}: {}", model_name, e);
                    eprintln!("{}", err_msg);
                    let mut file = log_file_clone.lock().await;
                }
            }
        })
    };
    let ollama_task = {
        let client = Arc::clone(&client);
        let prompt_data = prompt_data.clone();
        let call = Box::pin(async move { client.chat_ollama(&prompt_data).await });
        spawn_task("Ollama", call)
    };
    let openrouter_task = {
        let client = Arc::clone(&client);
        let prompt_data = prompt_data.clone();
        let call = Box::pin(async move { client.chat_openrouter(&prompt_data).await });
        spawn_task("OpenRouter", call)
    };
    let openai_task = {
        let client = Arc::clone(&client);
        let prompt_data = prompt_data.clone();
        let call = Box::pin(async move { client.chat_openai(&prompt_data).await });
        spawn_task("OpenAI", call)
    };

    let claude_task = {
        let client = Arc::clone(&client);
        let prompt_data = prompt_data.clone();
        let call = Box::pin(async move { client.chat_claude(&prompt_data).await });
        spawn_task("Claude", call)
    };

    let gemini_task = {
        let client = Arc::clone(&client);
        let prompt_data = prompt_data.clone();
        let call = Box::pin(async move { client.chat_gemini(&prompt_data).await });
        spawn_task("Gemini", call)
    };

    let xai_task = {
        let client = Arc::clone(&client);

        let call = Box::pin(async move { client.chat_xai(&prompt_data).await });
        spawn_task("XAI", call)
    };

    let _ = tokio::join!(
        openrouter_task,
        openai_task,
        claude_task,
        gemini_task,
        xai_task
    );

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load config from both .env and aerogel.toml
    let config = ApiConfig::load()?;
    let client = Arc::new(AiClient::new(config));

    let log_file = Arc::new(Mutex::new(
        OpenOptions::new().create(true).append(true).open(".tmp")?,
    ));

    println!("--- AI Client ---");
    println!("Commands: /upload <file_path>, /ask, /quit");
    println!("You can upload text, image, and audio files.");
    println!("Type your prompt (multi-line is okay), then use /ask to send.");

    let mut attached_files: Vec<String> = Vec::new();
    let mut multi_line_prompt = String::new();

    loop {
        if attached_files.is_empty() && multi_line_prompt.is_empty() {
            print!("> ");
        } else {
            print!(". ");
        }
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input_trimmed = input.trim();

        if input_trimmed.starts_with("/upload ") {
            let path = input_trimmed.split_at(8).1.trim().to_string();
            if Path::new(&path).exists() {
                println!("[INFO] Attached file: {}", path);
                attached_files.push(path);
            } else {
                eprintln!("[ERROR] File not found at '{}'", path);
            }
            continue;
        }

        if input_trimmed.eq_ignore_ascii_case("/quit")
            || input_trimmed.eq_ignore_ascii_case("/exit")
        {
            println!("Exiting.");
            break;
        }

        if input_trimmed.eq_ignore_ascii_case("/ask") {
            if multi_line_prompt.is_empty() && attached_files.is_empty() {
                println!("Cannot send an empty prompt. Type something or upload a file.");
                continue;
            }

            let prompt_text = multi_line_prompt.trim().to_string();
            println!(
                "\nSending prompt with {} attached file(s)...",
                attached_files.len()
            );

            match PromptData::new(prompt_text, &attached_files).await {
                Ok(prompt_data) => {
                    if let Err(e) =
                        process_prompt(Arc::clone(&client), Arc::clone(&log_file), prompt_data)
                            .await
                    {
                        eprintln!(
                            "[ERROR] An error occurred while processing the prompt: {}",
                            e
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[ERROR] Error preparing prompt data: {}", e);
                }
            }

            attached_files.clear();
            multi_line_prompt.clear();
            println!("\n------------------------------");
            println!("Enter a new prompt. Use /ask to send.");
            continue;
        }

        if !multi_line_prompt.is_empty() {
            multi_line_prompt.push('\n');
        }
        multi_line_prompt.push_str(input_trimmed);
    }

    Ok(())
}
