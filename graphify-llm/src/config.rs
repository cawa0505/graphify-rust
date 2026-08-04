use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Ollama,
    Gemini,
    OpenRouter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub r#type: ProviderType,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub priority: usize, // Lower number = higher priority
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub providers: Vec<Provider>,
}

impl LLMConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path)
            .map_err(|e| anyhow!("Failed to read LLM config: {}", e))?;
        let mut config: LLMConfig = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse TOML LLM config: {}", e))?;
        
        // Sort providers by priority ascending (priority 1 runs before priority 2)
        config.providers.sort_by_key(|p| p.priority);
        Ok(config)
    }
}
