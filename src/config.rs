use anyhow::{Context, Result};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub api_base: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlConfig {
    #[serde(rename = "Ollama")]
    ollama: ModelConfig,
    #[serde(rename = "OpenRouter")]
    openrouter: ModelConfig,
    #[serde(rename = "Gemini")]
    gemini: ModelConfig,
    #[serde(rename = "OpenAI")]
    openai: ModelConfig,
    #[serde(rename = "Claude")]
    claude: ModelConfig,
    #[serde(rename = "Xai")]
    xai: ModelConfig,
}

// The main config struct holds both the loaded model parameters and the API keys.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    // API keys, loaded from environment variables for security
    pub openrouter_key: Option<String>,
    pub openai_key: Option<String>,
    pub claude_key: Option<String>,
    pub gemini_key: Option<String>,
    pub xai_key: Option<String>,
    // Model parameters, loaded from aerogel.toml
    pub ollama: ModelConfig,
    pub openrouter: ModelConfig,
    pub openai: ModelConfig,
    pub claude: ModelConfig,
    pub gemini: ModelConfig,
    pub xai: ModelConfig,
}

impl ApiConfig {
    fn get_config_paths() -> Vec<String> {
        let mut paths = vec![
            // Current directory (highest priority)
            "aerogel.toml".to_string(),
            "../../aerogel.toml".to_string(),
        ];

        // XDG Base Directory Specification paths
        if let Ok(xdg_config_home) = env::var("XDG_CONFIG_HOME") {
            paths.push(format!("{}/aerogel/aerogel.toml", xdg_config_home));
            paths.push(format!("{}/aerogel.toml", xdg_config_home));
        }

        // Home directory paths
        if let Ok(home) = env::var("HOME") {
            paths.push(format!("{}/.config/aerogel/aerogel.toml", home));
            paths.push(format!("{}/.aerogel.toml", home));
        }

        // System-wide configuration
        paths.push("/etc/aerogel/aerogel.toml".to_string());

        paths
    }

    // Attempts to find and read the aerogel.toml configuration file from multiple locations.
    fn find_and_read_config() -> Result<(String, String)> {
        let paths = Self::get_config_paths();

        for path in &paths {
            if Path::new(path).exists() {
                match fs::read_to_string(path) {
                    Ok(content) => {
                        eprintln!("Loading configuration from: {}", path);
                        return Ok((content, path.clone()));
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Found config file at {} but failed to read it: {}",
                            path, e
                        );
                        continue;
                    }
                }
            }
        }

        // If no config file is found, provide a helpful error message
        Err(anyhow::anyhow!(
            "Could not find aerogel.toml configuration file in any of the following locations:\n{}",
            paths
                .iter()
                .map(|p| format!("  - {}", p))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }

    pub fn load() -> Result<Self> {
        // 1. Load API keys from .env file or system environment
        dotenv::dotenv().ok();
        let openrouter_key = env::var("OPENROUTER_API_KEY").ok();
        let openai_key = env::var("OPENAI_API_KEY").ok();
        let claude_key = env::var("CLAUDE_API_KEY").ok();
        let gemini_key = env::var("GEMINI_API_KEY").ok();
        let xai_key = env::var("XAI_API_KEY").ok();

        // 2. Find and read model parameters from aerogel.toml
        let (toml_str, config_path) = Self::find_and_read_config()
            .with_context(|| "Failed to locate and read aerogel.toml configuration file")?;

        let toml_config: TomlConfig = toml::from_str(&toml_str)
            .with_context(|| format!("Failed to parse configuration file at {}", config_path))?;

        // 3. Combine them into the final ApiConfig struct
        Ok(ApiConfig {
            openrouter_key,
            openai_key,
            claude_key,
            gemini_key,
            xai_key,
            ollama: toml_config.ollama,
            openrouter: toml_config.openrouter,
            openai: toml_config.openai,
            claude: toml_config.claude,
            gemini: toml_config.gemini,
            xai: toml_config.xai,
        })
    }

    pub fn get_key(&self, provider: &str) -> Option<&String> {
        match provider.to_lowercase().as_str() {
            "openrouter" => self.openrouter_key.as_ref(),
            "openai" => self.openai_key.as_ref(),
            "claude" | "anthropic" => self.claude_key.as_ref(),
            "gemini" | "google" => self.gemini_key.as_ref(),
            "xai" => self.xai_key.as_ref(),
            _ => None,
        }
    }
}
