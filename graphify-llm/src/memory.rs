use crate::config::LLMConfig;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use fastembed::{Bgem3Embedding, Bgem3InitOptions, Bgem3Model};

pub struct QdrantMemoryStore {
    config: LLMConfig,
    client: Client,
    // ponytail: Arc + Mutex allows safe sharing and on-demand initialization across tokio threads
    fastembed_model: std::sync::Arc<std::sync::Mutex<Option<Bgem3Embedding>>>,
}

impl QdrantMemoryStore {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            config,
            client,
            fastembed_model: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn get_or_init_fastembed(&self) -> Result<std::sync::Arc<std::sync::Mutex<Option<Bgem3Embedding>>>> {
        {
            let mut lock = self.fastembed_model.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
            if lock.is_none() {
                let cache_dir = dirs::home_dir().map_or_else(
                    || std::path::PathBuf::from(".fastembed_cache"),
                    |h| h.join(".cache").join("fastembed"),
                );

                // ponytail: ensure the cache directory exists to prevent any file system or permission write anomalies
                let _ = std::fs::create_dir_all(&cache_dir);

                println!("[graphify] Initializing local ONNX Runtime & loading BGE-M3 (int8)...");

                let mut opts = Bgem3InitOptions::new(Bgem3Model::BGEM3Q)
                    .with_max_length(1024)
                    .with_cache_dir(cache_dir);

                if let Some(concurrency) = self.config.extraction.concurrency {
                    opts = opts.with_intra_threads(concurrency);
                } else {
                    opts = opts.with_intra_threads(4);
                }
                let model = Bgem3Embedding::try_new(opts)
                    .map_err(|e| anyhow!("Failed to init fastembed BGE-M3 model: {}", e))?;
                
                println!("[graphify] ONNX Runtime initialized & BGE-M3 model loaded successfully!");
                *lock = Some(model);
            }
        }
        Ok(self.fastembed_model.clone())
    }

    /// Generates embeddings for a batch of text chunks using the configured Ollama model.
    pub async fn get_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let embedding_config = &self.config.memory.long_term.embedding;

        if embedding_config.provider == "fastembed" {
            let texts = texts.to_vec();
            let model_arc = self.get_or_init_fastembed()?;
            let results = tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
                let mut lock = model_arc.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
                let model = lock.as_mut().ok_or_else(|| anyhow!("Model not initialized"))?;
                let output = model.embed(texts, None).map_err(|e| anyhow!("fastembed error: {}", e))?;
                drop(lock); // ponytail: early drop to avoid significant_drop_tightening
                Ok(output.dense)
            }).await??;
            return Ok(results);
        }

        if embedding_config.provider != "ollama" {
            return Err(anyhow!("Unsupported embedding provider: {}", embedding_config.provider));
        }

        let url = format!("{}/api/embeddings", embedding_config.endpoint.trim_end_matches('/'));
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
        let mut join_set = tokio::task::JoinSet::new();

        for (idx, text) in texts.iter().cloned().enumerate() {
            let client = self.client.clone();
            let url = url.clone();
            let model = embedding_config.model.clone();
            let sem = semaphore.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire().await.map_err(|e| anyhow!("Semaphore acquire failed: {}", e))?;
                let payload = serde_json::json!({
                    "model": model,
                    "prompt": text,
                });

                let res = client.post(&url)
                    .json(&payload)
                    .send()
                    .await?;
                let res = res.error_for_status()?;
                let body: Value = res.json().await?;
                
                body["embedding"].as_array().map_or_else(|| Err(anyhow!("Ollama response missing embedding array")), |arr| {
                    #[allow(clippy::cast_possible_truncation)]
                    let vec: Vec<f32> = arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
                    Ok::<(usize, Vec<f32>), anyhow::Error>((idx, vec))
                })
            });
        }

        let mut indexed_results = Vec::with_capacity(texts.len());
        while let Some(res) = join_set.join_next().await {
            let (idx, vec) = res.map_err(|e| anyhow!("Task join failed: {}", e))??;
            indexed_results.push((idx, vec));
        }

        indexed_results.sort_by_key(|(idx, _)| *idx);
        let results = indexed_results.into_iter().map(|(_, vec)| vec).collect();
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
        
        // ponytail: filter out fine-grained nodes based on config to optimize memory density & RAG quality
        let filtered_nodes: Vec<&graphify_core::Node> = nodes
            .iter()
            .filter(|node| lt_config.index_kinds.contains(&node.kind))
            .collect();

        if filtered_nodes.is_empty() {
            return Ok(());
        }

        let texts: Vec<String> = filtered_nodes.iter().map(|node| {
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

        let embeddings = self.get_embeddings(&texts).await?;

        let url = format!(
            "{}/collections/{}/points",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let mut points = Vec::new();
        for (node, vector) in filtered_nodes.iter().zip(embeddings) {
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
