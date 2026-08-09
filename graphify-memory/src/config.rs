use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermMemoryConfig {
    pub max_messages: usize,
}

impl Default for ShortTermMemoryConfig {
    fn default() -> Self {
        Self { max_messages: 20 }
    }
}

fn default_indexing_threshold() -> usize {
    20000
}

fn default_local_fallback_enabled() -> bool {
    false
}

fn default_local_bin_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(".cache/graphify/qdrant")
}

fn default_local_storage_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(".local/share/graphify/qdrant-storage")
}

fn default_local_version() -> String {
    "v1.19.0".to_string()
}

fn default_local_http_port() -> u16 {
    16_333
}

fn default_local_grpc_port() -> u16 {
    16_334
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub api_key: Option<String>,
    pub collection: String,
    pub distance: String,
    #[serde(default)]
    pub grpc: bool,
    #[serde(default = "default_indexing_threshold")]
    pub indexing_threshold: usize,
    #[serde(default = "default_local_fallback_enabled")]
    pub local_fallback_enabled: bool,
    #[serde(default = "default_local_bin_dir")]
    pub local_bin_dir: std::path::PathBuf,
    #[serde(default = "default_local_storage_dir")]
    pub local_storage_dir: std::path::PathBuf,
    #[serde(default = "default_local_version")]
    pub local_version: String,
    #[serde(default = "default_local_http_port")]
    pub local_http_port: u16,
    #[serde(default = "default_local_grpc_port")]
    pub local_grpc_port: u16,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6333".to_string(),
            api_key: None,
            collection: "graphify_memory".to_string(),
            distance: "Cosine".to_string(),
            grpc: false,
            indexing_threshold: 20000,
            local_fallback_enabled: false,
            local_bin_dir: default_local_bin_dir(),
            local_storage_dir: default_local_storage_dir(),
            local_version: default_local_version(),
            local_http_port: default_local_http_port(),
            local_grpc_port: default_local_grpc_port(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub vector_size: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            endpoint: "http://localhost:11434".to_string(),
            model: "bge-m3".to_string(),
            vector_size: 1024,
        }
    }
}

fn default_index_kinds() -> Vec<String> {
    vec![
        "module".to_string(),
        "class".to_string(),
        "struct".to_string(),
        "trait".to_string(),
        "interface".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermMemoryConfig {
    pub enabled: bool,
    pub provider: String,
    pub embedding: EmbeddingConfig,
    pub qdrant: QdrantConfig,
    #[serde(default = "default_index_kinds")]
    pub index_kinds: Vec<String>,
}

impl Default for LongTermMemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "qdrant".to_string(),
            embedding: EmbeddingConfig::default(),
            qdrant: QdrantConfig::default(),
            index_kinds: default_index_kinds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    pub short_term: ShortTermMemoryConfig,
    pub long_term: LongTermMemoryConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qdrant_toml(fields: &str) -> String {
        format!(
            r#"
url = "http://localhost:6333"
api_key = "secret"
collection = "graphify_memory"
distance = "Cosine"
{fields}
"#
        )
    }

    #[test]
    fn parse_defaults_without_local_fields() -> Result<(), Box<dyn std::error::Error>> {
        // A legacy config without any local-fallback fields must parse with defaults.
        let cfg: QdrantConfig = toml::from_str(&qdrant_toml(""))?;
        assert_eq!(cfg.url, "http://localhost:6333");
        assert_eq!(cfg.collection, "graphify_memory");
        assert!(!cfg.local_fallback_enabled);
        assert_eq!(cfg.local_version, "v1.19.0");
        assert_eq!(cfg.local_http_port, 16_333);
        assert_eq!(cfg.local_grpc_port, 16_334);
        assert_eq!(
            cfg.local_bin_dir,
            std::path::PathBuf::from(".cache/graphify/qdrant")
        );
        assert_eq!(
            cfg.local_storage_dir,
            std::path::PathBuf::from(".local/share/graphify/qdrant-storage")
        );
        Ok(())
    }

    #[test]
    fn parse_explicit_local_fields() -> Result<(), Box<dyn std::error::Error>> {
        let cfg: QdrantConfig = toml::from_str(&qdrant_toml(
            r#"
local_fallback_enabled = true
local_version = "v1.19.0"
local_http_port = 16333
local_grpc_port = 16334
local_bin_dir = "/tmp/qdrant-bin"
local_storage_dir = "/tmp/qdrant-storage"
"#,
        ))?;
        assert!(cfg.local_fallback_enabled);
        assert_eq!(cfg.local_version, "v1.19.0");
        assert_eq!(cfg.local_http_port, 16_333);
        assert_eq!(
            cfg.local_bin_dir,
            std::path::PathBuf::from("/tmp/qdrant-bin")
        );
        assert_eq!(
            cfg.local_storage_dir,
            std::path::PathBuf::from("/tmp/qdrant-storage")
        );
        Ok(())
    }

    #[test]
    fn serialized_default_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
        let cfg = QdrantConfig::default();
        let s = toml::to_string(&cfg)?;
        let parsed: QdrantConfig = toml::from_str(&s)?;
        assert_eq!(parsed.local_fallback_enabled, cfg.local_fallback_enabled);
        assert_eq!(parsed.local_http_port, cfg.local_http_port);
        Ok(())
    }
}
