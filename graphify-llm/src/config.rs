use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use graphify_memory::config::{
    EmbeddingConfig, LongTermMemoryConfig, MemoryConfig, QdrantConfig, ShortTermMemoryConfig,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Ollama,
    Gemini,
    OpenRouter,
    OpenAI,
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
    #[serde(default)]
    pub concurrency: Option<usize>, // Rayon thread pool concurrency limit
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1024,
            max_concurrency: 1,
            concurrency: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginsConfig {
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
}

fn default_plugins_config_path() -> Option<PathBuf> {
    std::env::var("GRAPHIFY_CONFIG_PATH")
        .ok()
        .map(PathBuf::from)
}

impl PluginsConfig {
    /// Loads plugin configuration from the first available source:
    /// 1. `GRAPHIFY_CONFIG_PATH` env var.
    /// 2. XDG standard config path (`~/.config/graphify/config.toml`).
    /// 3. Legacy JSON config path (`~/.graphify/config.json`) — plugin section not migrated.
    ///
    /// # Errors
    /// Returns an error if an existing configuration file could not be parsed.
    pub fn load() -> Result<Self> {
        let path = default_plugins_config_path().unwrap_or_else(|| {
            std::env::var("XDG_CONFIG_HOME").map_or_else(
                |_| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    Path::new(&home)
                        .join(".config")
                        .join("graphify")
                        .join("config.toml")
                },
                |xdg| PathBuf::from(xdg).join("graphify").join("config.toml"),
            )
        });
        Self::load_from(Some(path))
    }

    /// Loads plugin configuration from a specific TOML file path.
    ///
    /// A missing file yields an empty container; a malformed file is an error.
    pub fn load_from(path: Option<PathBuf>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let Ok(content) = fs::read_to_string(&path) else {
            return Ok(Self::default());
        };
        let parsed: Self = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse plugin config {}: {}", path.display(), e))?;
        Ok(parsed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub providers: Vec<Provider>,
    pub extraction: ExtractionConfig,
    #[serde(default)]
    pub api_keys: Vec<String>, // Flattened keys for rotation
    #[serde(default)]
    pub memory: MemoryConfig,
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            providers: vec![Provider {
                name: "ollama".to_string(),
                r#type: ProviderType::Ollama,
                endpoint: "http://localhost:11434".to_string(),
                model: "bge-m3".to_string(),
                api_key: None,
                priority: 10,
            }],
            extraction: ExtractionConfig::default(),
            api_keys: Vec::new(),
            memory: MemoryConfig::default(),
        }
    }
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
    #[allow(clippy::too_many_lines)] // ponytail: file reading and legacy JSON migration paths are naturally long
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        // An explicit CLI path must win over environment and XDG defaults.
        let path = path.as_ref();
        if !path.as_os_str().is_empty() {
            let content = fs::read_to_string(path)
                .map_err(|e| anyhow!("Failed to read config '{}': {}", path.display(), e))?;
            let mut config: LLMConfig = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse TOML config '{}': {}", path.display(), e))?;

            if config.api_keys.is_empty() {
                for provider in &config.providers {
                    if let Some(ref keys) = provider.api_key {
                        config.api_keys.extend(
                            keys.split(',')
                                .map(str::trim)
                                .filter(|key| !key.is_empty())
                                .map(ToString::to_string),
                        );
                    }
                }
            }
            config.providers.sort_by_key(|provider| provider.priority);
            return Ok(config);
        }

        // 1. Check GRAPHIFY_CONFIG_PATH
        let env_path = std::env::var("GRAPHIFY_CONFIG_PATH").ok();
        if let Some(p) = env_path {
            let content = fs::read_to_string(p)
                .map_err(|e| anyhow!("Failed to read config from GRAPHIFY_CONFIG_PATH: {}", e))?;
            let mut config: LLMConfig = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse TOML config: {}", e))?;

            // Auto-populate api_keys if empty
            if config.api_keys.is_empty() {
                for p in &config.providers {
                    if let Some(ref k) = p.api_key {
                        for k_part in k.split(',') {
                            let trimmed = k_part.trim();
                            if !trimmed.is_empty() {
                                config.api_keys.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }

            config.providers.sort_by_key(|p| p.priority);
            return Ok(config);
        }

        // 2. Check XDG Path (~/.config/graphify/config.toml)
        let xdg_path = std::env::var("XDG_CONFIG_HOME").map_or_else(
            |_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                std::path::Path::new(&home)
                    .join(".config")
                    .join("graphify")
                    .join("config.toml")
            },
            |xdg_home| PathBuf::from(xdg_home).join("graphify").join("config.toml"),
        );

        if xdg_path.exists() {
            let content = fs::read_to_string(&xdg_path)
                .map_err(|e| anyhow!("Failed to read XDG config: {}", e))?;
            let mut config: LLMConfig = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse TOML config: {}", e))?;

            // Auto-populate api_keys if empty
            if config.api_keys.is_empty() {
                for p in &config.providers {
                    if let Some(ref k) = p.api_key {
                        for k_part in k.split(',') {
                            let trimmed = k_part.trim();
                            if !trimmed.is_empty() {
                                config.api_keys.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }

            config.providers.sort_by_key(|p| p.priority);
            return Ok(config);
        }

        // 3. Legacy JSON Migration (~/.graphify/config.json)
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let final_legacy_path = std::path::Path::new(&home)
            .join(".graphify")
            .join("config.json");

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
                    concurrency: None,
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
                        Some("openai") => ProviderType::OpenAI,
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
            eprintln!(
                "[graphify] Migrated legacy JSON config to {}",
                xdg_path.display()
            );

            return Ok(config);
        }

        // Default fallback if nothing exists: auto-create default TOML configuration
        let default_config = LLMConfig::default();
        let xdg_dir = xdg_path
            .parent()
            .ok_or_else(|| anyhow!("Invalid XDG path parent"))?;
        if !xdg_dir.exists() {
            fs::create_dir_all(xdg_dir)
                .map_err(|e| anyhow!("Failed to create XDG config directory: {}", e))?;
        }
        let toml_str = toml::to_string_pretty(&default_config)
            .map_err(|e| anyhow!("Failed to serialize default TOML: {}", e))?;
        fs::write(&xdg_path, &toml_str)
            .map_err(|e| anyhow!("Failed to write default XDG config: {}", e))?;
        eprintln!(
            "[graphify] Created default configuration at {}",
            xdg_path.display()
        );

        Ok(default_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_multi_plugin_toml() -> Result<()> {
        let toml_str = r#"
            [plugins.opendoc]
            command = "opendoc-mcp"
            args = ["--port", "8080"]
            env = { RUST_LOG = "debug" }
            cwd = "/tmp/opendoc"

            [plugins.review]
            command = "graphify-plugin-review"
        "#;
        let parsed: PluginsConfig = toml::from_str(toml_str)?;
        assert_eq!(parsed.plugins.len(), 2, "both plugins parsed");

        let opendoc = parsed
            .plugins
            .get("opendoc")
            .ok_or_else(|| anyhow!("opendoc exists"))?;
        assert_eq!(opendoc.command, "opendoc-mcp");
        assert_eq!(opendoc.args, vec!["--port", "8080"]);
        assert_eq!(opendoc.env.get("RUST_LOG"), Some(&"debug".to_string()));
        assert_eq!(opendoc.cwd.as_deref(), Some("/tmp/opendoc"));

        let review = parsed
            .plugins
            .get("review")
            .ok_or_else(|| anyhow!("review exists"))?;
        assert_eq!(review.command, "graphify-plugin-review");
        assert!(review.args.is_empty(), "args default to empty");
        assert!(review.cwd.is_none(), "cwd defaults to none");
        Ok(())
    }

    #[test]
    fn test_missing_command_is_error() {
        let toml_str = r#"
            [plugins.broken]
            args = ["--flag"]
        "#;
        let err = toml::from_str::<PluginsConfig>(toml_str);
        assert!(err.is_err(), "missing command must fail to parse");
    }

    #[test]
    fn test_empty_plugins_table() -> Result<()> {
        let parsed: PluginsConfig = toml::from_str("")?;
        assert!(
            parsed.plugins.is_empty(),
            "no [plugins] section => empty map"
        );
        Ok(())
    }

    #[test]
    fn test_load_from_xdg_path() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("graphify-cfg-test-{}", std::process::id()));
        let cfg_dir = dir.join("graphify");
        std::fs::create_dir_all(&cfg_dir)?;
        std::fs::write(
            cfg_dir.join("config.toml"),
            "[plugins.demo]\ncommand = \"demo-mcp\"\n",
        )?;

        let parsed = PluginsConfig::load_from(Some(cfg_dir.join("config.toml")))?;
        assert!(
            parsed.plugins.contains_key("demo"),
            "plugin from config file loaded"
        );

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_load_from_explicit_path() -> Result<()> {
        let path = std::env::temp_dir().join(format!(
            "graphify-explicit-config-{}.toml",
            std::process::id()
        ));
        let mut config = LLMConfig::default();
        config.providers[0].name = "explicit-provider".to_string();
        fs::write(&path, toml::to_string(&config)?)?;

        let loaded = LLMConfig::load_from_file(&path)?;
        assert_eq!(loaded.providers[0].name, "explicit-provider");

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn test_load_missing_file_is_empty() -> Result<()> {
        let parsed = PluginsConfig::load_from(Some(PathBuf::from("/nonexistent/nope.toml")))?;
        assert!(
            parsed.plugins.is_empty(),
            "missing file yields empty container"
        );
        Ok(())
    }

    #[test]
    fn test_load_none_path_is_empty() -> Result<()> {
        let parsed = PluginsConfig::load_from(None)?;
        assert!(parsed.plugins.is_empty(), "no path yields empty container");
        Ok(())
    }
}
