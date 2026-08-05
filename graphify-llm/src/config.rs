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
pub struct ExtractionConfig {
    pub chunk_size: usize,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermMemoryConfig {
    pub max_messages: usize,
}

impl Default for ShortTermMemoryConfig {
    fn default() -> Self {
        Self { max_messages: 20 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub api_key: Option<String>,
    pub collection: String,
    pub distance: String,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6333".to_string(),
            api_key: None,
            collection: "graphify_memory".to_string(),
            distance: "Cosine".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermMemoryConfig {
    pub enabled: bool,
    pub provider: String,
    pub qdrant: QdrantConfig,
}

impl Default for LongTermMemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "qdrant".to_string(),
            qdrant: QdrantConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    pub short_term: ShortTermMemoryConfig,
    pub long_term: LongTermMemoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub providers: Vec<Provider>,
    pub extraction: ExtractionConfig,
    pub api_keys: Vec<String>, // Flattened keys for rotation
    #[serde(default)]
    pub memory: MemoryConfig,
}

#[derive(Deserialize)]
struct LegacyConfig {
    #[allow(dead_code)]
    backend: Option<String>,
    providers: Option<Vec<serde_json::Value>>,
    extraction: Option<serde_json::Value>,
}

impl LLMConfig {
    /// Loads the configuration from the first available source:
    /// 1. `GRAPHIFY_CONFIG_PATH` env var.
    /// 2. XDG standard config path (`~/.config/graphify/config.toml`).
    /// 3. Legacy JSON config path (`~/.graphify/config.json`) with auto-migration.
    ///
    /// # Errors
    /// Returns an error if no configuration file could be found or parsed.
    pub fn load_from_file<P: AsRef<Path>>(_path: P) -> Result<Self> {
        // 1. Check GRAPHIFY_CONFIG_PATH
        let env_path = std::env::var("GRAPHIFY_CONFIG_PATH").ok();
        if let Some(p) = env_path {
            let content = fs::read_to_string(p)
                .map_err(|e| anyhow!("Failed to read config from GRAPHIFY_CONFIG_PATH: {}", e))?;
            let mut config: LLMConfig = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse TOML config: {}", e))?;
            config.providers.sort_by_key(|p| p.priority);
            return Ok(config);
        }

        // 2. Check XDG Path (~/.config/graphify/config.toml)
        let xdg_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| ".".to_string());
        let xdg_path = std::path::Path::new(&xdg_home)
            .join(".config")
            .join("graphify")
            .join("config.toml");
        
        if xdg_path.exists() {
            let content = fs::read_to_string(xdg_path)
                .map_err(|e| anyhow!("Failed to read XDG config: {}", e))?;
            let mut config: LLMConfig = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse TOML config: {}", e))?;
            config.providers.sort_by_key(|p| p.priority);
            return Ok(config);
        }

        // 3. Legacy JSON Migration (~/.graphify/config.json)
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let final_legacy_path = std::path::Path::new(&home).join(".graphify").join("config.json");

        if final_legacy_path.exists() {
            let content = fs::read_to_string(final_legacy_path)
                .map_err(|e| anyhow!("Failed to read legacy config: {}", e))?;
            
            let legacy: LegacyConfig = serde_json::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse legacy JSON: {}", e))?;

            let mut config = LLMConfig {
                providers: Vec::new(),
                extraction: ExtractionConfig {
                    chunk_size: 1024,
                    max_concurrency: 1,
                },
                api_keys: Vec::new(),
                memory: MemoryConfig::default(),
            };

            if let Some(pro_list) = legacy.providers {
                for p_val in pro_list {
                    let name = p_val["name"].as_str().unwrap_or("Unknown").to_string();
                    let r_type = match p_val["r#type"].as_str() {
                        Some("gemini") => ProviderType::Gemini,
                        Some("openrouter") => ProviderType::OpenRouter,
                        _ => ProviderType::Ollama,
                    };
                    let endpoint = p_val["endpoint"].as_str().unwrap_or("").to_string();
                    let model = p_val["model"].as_str().unwrap_or("").to_string();
                    
                    let raw_priority = p_val["priority"].as_u64().unwrap_or(10);
                    let priority = usize::try_from(raw_priority).unwrap_or(10);
                    
                    let p = Provider {
                        name,
                        r#type: r_type,
                        endpoint,
                        model,
                        api_key: p_val["api_key"].as_str().map(|s| s.to_string()),
                        priority,
                    };
                    config.providers.push(p);
                }
            }

            for p in &config.providers {
                if let Some(ref k) = p.api_key {
                    config.api_keys.push(k.clone());
                }
            }

            if let Some(ex_val) = legacy.extraction {
                let chunk_val = ex_val["chunk_size"].as_u64().unwrap_or(1024);
                let concurrency_val = ex_val["max_concurrency"].as_u64().unwrap_or(1);
                
                config.extraction.chunk_size = usize::try_from(chunk_val).unwrap_or(1024);
                config.extraction.max_concurrency = usize::try_from(concurrency_val).unwrap_or(1);
            }

            config.providers.sort_by_key(|p| p.priority);

            let xdg_dir = std::path::Path::new(&home).join(".config").join("graphify");
            if !xdg_dir.exists() {
                fs::create_dir_all(&xdg_dir).ok();
            }
            let xdg_path = xdg_dir.join("config.toml");
            let toml_str = toml::to_string_pretty(&config)
                .map_err(|e| anyhow!("Failed to serialize migrated TOML: {}", e))?;
            fs::write(&xdg_path, toml_str).ok();
            eprintln!("[graphify] Migrated legacy JSON config to {}", xdg_path.display());

            return Ok(config);
        }

        // Default fallback if nothing exists
        let content = fs::read_to_string(xdg_path)
            .map_err(|e| anyhow!("Failed to read XDG config: {}", e))?;
        let mut config: LLMConfig = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse TOML config: {}", e))?;
        config.providers.sort_by_key(|p| p.priority);
        Ok(config)
    }
}
