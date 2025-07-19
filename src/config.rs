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
#[serde(deny_unknown_fields)]
struct TomlConfig {
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
    pub openai_key: Option<String>,
    pub claude_key: Option<String>,
    pub gemini_key: Option<String>,
    pub xai_key: Option<String>,
    // Model parameters, loaded from aerogel.toml
    pub openai: ModelConfig,
    pub claude: ModelConfig,
    pub gemini: ModelConfig,
    pub xai: ModelConfig,
}

impl ApiConfig {
    /// Returns a list of paths to search for the aerogel.toml configuration file.
    /// The paths are checked in order of preference:
    /// 1. Current working directory
    /// 2. User's home directory
    /// 3. XDG config directory (~/.config/aerogel/ on Linux)
    /// 4. System config directory (/etc/aerogel/ on Unix systems)
    fn get_config_paths() -> Vec<String> {
        let mut paths = vec![
            // Current directory (highest priority)
            "aerogel.toml".to_string(),
            "../../aerogel.toml".to_string(),
        ];

        // Home directory
        if let Some(home_dir) = dirs::home_dir() {
            paths.push(home_dir.join("aerogel.toml").to_string_lossy().to_string());
            paths.push(home_dir.join(".aerogel.toml").to_string_lossy().to_string());
        }

        // XDG config directory
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("aerogel").join("aerogel.toml").to_string_lossy().to_string());
            paths.push(config_dir.join("aerogel").join("config.toml").to_string_lossy().to_string());
        }

        // System-wide config (Unix-like systems)
        #[cfg(unix)]
        {
            paths.push("/etc/aerogel/aerogel.toml".to_string());
            paths.push("/etc/aerogel/config.toml".to_string());
        }

        // Windows system config
        #[cfg(windows)]
        {
            if let Some(program_data) = env::var("PROGRAMDATA").ok() {
                paths.push(format!("{}\\aerogel\\aerogel.toml", program_data));
                paths.push(format!("{}\\aerogel\\config.toml", program_data));
            }
        }

        paths
    }

    /// Attempts to find and read the aerogel.toml configuration file from multiple locations.
    /// Returns the file content and the path where it was found.
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
                        eprintln!("Warning: Found config file at {} but failed to read it: {}", path, e);
                        continue;
                    }
                }
            }
        }

        // If no config file is found, provide a helpful error message
        Err(anyhow::anyhow!(
            "Could not find aerogel.toml configuration file in any of the following locations:\n{}",
            paths.iter()
                .map(|p| format!("  - {}", p))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }

    // Renamed from `from_env` to `load` to reflect loading from both .env and .toml
    pub fn load() -> Result<Self> {
        // 1. Load API keys from .env file or system environment
        dotenv::dotenv().ok();

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
            openai_key,
            claude_key,
            gemini_key,
            xai_key,
            openai: toml_config.openai,
            claude: toml_config.claude,
            gemini: toml_config.gemini,
            xai: toml_config.xai,
        })
    }

    pub fn get_key(&self, provider: &str) -> Option<&String> {
        match provider.to_lowercase().as_str() {
            "openai" => self.openai_key.as_ref(),
            "claude" | "anthropic" => self.claude_key.as_ref(),
            "gemini" | "google" => self.gemini_key.as_ref(),
            "xai" => self.xai_key.as_ref(),
            _ => None,
        }
    }
}