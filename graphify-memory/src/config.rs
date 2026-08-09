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
