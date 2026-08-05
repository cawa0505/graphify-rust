use crate::config::LLMConfig;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

pub struct QdrantMemoryStore {
    config: LLMConfig,
    client: Client,
}

impl QdrantMemoryStore {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    /// Generates embeddings for a batch of text chunks using the configured Ollama model.
    pub async fn get_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let embedding_config = &self.config.memory.long_term.embedding;
        if embedding_config.provider != "ollama" {
            return Err(anyhow!("Unsupported embedding provider: {}", embedding_config.provider));
        }

        let url = format!("{}/api/embeddings", embedding_config.endpoint.trim_end_matches('/'));
        let mut results = Vec::new();

        for text in texts {
            let payload = serde_json::json!({
                "model": embedding_config.model,
                "prompt": text,
            });

            let res = self.client.post(&url)
                .json(&payload)
                .send()
                .await?;
            let res = res.error_for_status()?;
            let body: Value = res.json().await?;
            
            if let Some(arr) = body["embedding"].as_array() {
                #[allow(clippy::cast_possible_truncation)]
                let vec: Vec<f32> = arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
                results.push(vec);
            } else {
                return Err(anyhow!("Ollama response missing embedding array"));
            }
        }

        Ok(results)
    }

    /// Ensures that the configured collection exists in Qdrant, creating it if necessary.
    pub async fn ensure_collection(&self) -> Result<()> {
        let lt_config = &self.config.memory.long_term;
        let qdrant_config = &lt_config.qdrant;
        let embedding_config = &lt_config.embedding;

        let url = format!(
            "{}/collections/{}",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let req = self.client.get(&url);
        let req = if let Some(ref key) = qdrant_config.api_key {
            req.header("api-key", key)
        } else {
            req
        };

        let res = req.send().await?;
        if res.status().is_success() {
            return Ok(());
        }

        let create_payload = serde_json::json!({
            "vectors": {
                "size": embedding_config.vector_size,
                "distance": qdrant_config.distance
            }
        });

        let req = self.client.put(&url).json(&create_payload);
        let req = if let Some(ref key) = qdrant_config.api_key {
            req.header("api-key", key)
        } else {
            req
        };

        let res = req.send().await?;
        if !res.status().is_success() {
            let status = res.status();
            let body_text = res.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to auto-create Qdrant collection '{}': status={}, response={}",
                qdrant_config.collection,
                status,
                body_text
            ));
        }

        eprintln!("[graphify] Auto-created Qdrant collection: {}", qdrant_config.collection);
        Ok(())
    }

    /// Upserts a batch of nodes into the Qdrant database.
    pub async fn upsert_nodes(&self, nodes: &[graphify_core::Node]) -> Result<()> {
        let lt_config = &self.config.memory.long_term;
        if !lt_config.enabled {
            return Ok(());
        }

        let qdrant_config = &lt_config.qdrant;
        
        let texts: Vec<String> = nodes.iter().map(|node| {
            format!(
                "ID: {}\nLabel: {}\nFileType: {:?}\nKind: {}\nLanguage: {}\nSourceFile: {}\nLineRange: {}-{}\nDoc: {}\nDescription: {}",
                node.id.0,
                node.label,
                node.file_type,
                node.kind,
                node.language,
                node.source_file,
                node.start_line,
                node.end_line,
                node.doc_comment.as_deref().unwrap_or(""),
                node.description.as_deref().unwrap_or("")
            )
        }).collect();

        if texts.is_empty() {
            return Ok(());
        }

        let embeddings = self.get_embeddings(&texts).await?;

        let url = format!(
            "{}/collections/{}/points",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let mut points = Vec::new();
        for (node, vector) in nodes.iter().zip(embeddings) {
            let point_id = {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                node.id.0.hash(&mut hasher);
                hasher.finish()
            };

            let payload = serde_json::json!({
                "id": point_id,
                "vector": vector,
                "payload": {
                    "node_id": node.id.0,
                    "label": node.label,
                    "file_type": node.file_type,
                    "kind": node.kind,
                    "language": node.language,
                    "source_file": node.source_file,
                    "start_line": node.start_line,
                    "end_line": node.end_line,
                    "description": node.description,
                }
            });
            points.push(payload);
        }

        let payload = serde_json::json!({
            "points": points
        });

        let req = self.client.put(&url).json(&payload);
        let req = if let Some(ref key) = qdrant_config.api_key {
            req.header("api-key", key)
        } else {
            req
        };

        let res = req.send().await?;
        res.error_for_status()?;

        Ok(())
    }

    /// Query Qdrant for similar nodes (RAG semantic retrieval).
    pub async fn query_similar_nodes(&self, query_text: &str, limit: usize) -> Result<Vec<Value>> {
        let lt_config = &self.config.memory.long_term;
        if !lt_config.enabled {
            return Ok(Vec::new());
        }

        let qdrant_config = &lt_config.qdrant;

        let query_embeddings = self.get_embeddings(&[query_text.to_string()]).await?;
        if query_embeddings.is_empty() {
            return Err(anyhow!("Failed to generate query embedding"));
        }
        let vector = &query_embeddings[0];

        let url = format!(
            "{}/collections/{}/points/search",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let payload = serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
            "with_vector": false,
        });

        let req = self.client.post(&url).json(&payload);
        let req = if let Some(ref key) = qdrant_config.api_key {
            req.header("api-key", key)
        } else {
            req
        };

        let res = req.send().await?;
        let res = res.error_for_status()?;
        let body: Value = res.json().await?;

        body["result"].as_array().map_or_else(
            || Ok(Vec::new()),
            |result_list| Ok(result_list.clone())
        )
    }
}
