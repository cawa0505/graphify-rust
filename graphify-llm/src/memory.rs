use crate::config::LLMConfig;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use fastembed::{Bgem3Embedding, Bgem3InitOptions, Bgem3Model};

pub struct QdrantMemoryStore {
    config: LLMConfig,
    client: Client,
    grpc_client: Option<qdrant_client::Qdrant>,
    // ponytail: Arc + Mutex allows safe sharing and on-demand initialization across tokio threads
    fastembed_model: std::sync::Arc<std::sync::Mutex<Option<Bgem3Embedding>>>,
}

impl QdrantMemoryStore {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        let grpc_client = if config.memory.long_term.qdrant.grpc {
            let mut url = config.memory.long_term.qdrant.url.clone();
            if url.contains(":6333") {
                url = url.replace(":6333", ":6334");
            }
            let mut builder = qdrant_client::Qdrant::from_url(&url);
            if let Some(ref api_key) = config.memory.long_term.qdrant.api_key {
                builder = builder.api_key(api_key.as_str());
            }
            builder.build().ok()
        } else {
            None
        };
        Self {
            config,
            client,
            grpc_client,
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

        // gRPC path
        if let Some(ref client) = self.grpc_client {
            use qdrant_client::qdrant::{
                CreateCollectionBuilder, VectorParamsBuilder, Distance,
                OptimizersConfigDiffBuilder, CreateFieldIndexCollectionBuilder, FieldType,
            };
            let exists = client.collection_exists(&qdrant_config.collection).await.unwrap_or(false);
            if !exists {
                let dist = match qdrant_config.distance.to_uppercase().as_str() {
                    "DOT" => Distance::Dot,
                    "EUCLIDEAN" => Distance::Euclid,
                    _ => Distance::Cosine,
                };
                client.create_collection(
                    CreateCollectionBuilder::new(&qdrant_config.collection)
                        .vectors_config(VectorParamsBuilder::new(embedding_config.vector_size as u64, dist))
                        .optimizers_config(OptimizersConfigDiffBuilder::default().indexing_threshold(0))
                ).await?;

                // Create metadata field indexes for 100% precision filtered RAG queries
                let _ = client.create_field_index(
                    CreateFieldIndexCollectionBuilder::new(&qdrant_config.collection, "source_file", FieldType::Keyword).wait(true)
                ).await;
                let _ = client.create_field_index(
                    CreateFieldIndexCollectionBuilder::new(&qdrant_config.collection, "kind", FieldType::Keyword).wait(true)
                ).await;
                let _ = client.create_field_index(
                    CreateFieldIndexCollectionBuilder::new(&qdrant_config.collection, "language", FieldType::Keyword).wait(true)
                ).await;

                eprintln!("[graphify] Auto-created Qdrant collection (gRPC): {}", qdrant_config.collection);
            }
            return Ok(());
        }

        // REST fallback path
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
            },
            "optimizers_config": {
                "indexing_threshold": 0
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

        // Create indexes on REST
        for field in &["source_file", "kind", "language"] {
            let index_url = format!("{}/index", url.trim_end_matches('/'));
            let idx_payload = serde_json::json!({
                "field_name": field,
                "field_schema": "keyword"
            });
            let mut req = self.client.put(&index_url).json(&idx_payload);
            if let Some(ref key) = qdrant_config.api_key {
                req = req.header("api-key", key);
            }
            let _ = req.send().await;
        }

        eprintln!("[graphify] Auto-created Qdrant collection: {}", qdrant_config.collection);
        Ok(())
    }

    /// Deletes the Qdrant collection if it exists.
    pub async fn delete_collection(&self) -> Result<()> {
        let lt_config = &self.config.memory.long_term;
        let qdrant_config = &lt_config.qdrant;

        if let Some(ref client) = self.grpc_client {
            let _ = client.delete_collection(&qdrant_config.collection).await;
            return Ok(());
        }

        let url = format!(
            "{}/collections/{}",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let mut req = self.client.delete(&url);
        if let Some(ref key) = qdrant_config.api_key {
            req = req.header("api-key", key);
        }

        let _ = req.send().await;
        Ok(())
    }

    async fn set_indexing_threshold(&self, threshold: usize) -> Result<()> {
        let lt_config = &self.config.memory.long_term;
        let qdrant_config = &lt_config.qdrant;

        if let Some(ref client) = self.grpc_client {
            use qdrant_client::qdrant::{OptimizersConfigDiffBuilder, UpdateCollectionBuilder};
            client.update_collection(
                UpdateCollectionBuilder::new(&qdrant_config.collection)
                    .optimizers_config(OptimizersConfigDiffBuilder::default().indexing_threshold(threshold as u64))
            ).await?;
            return Ok(());
        }

        let patch_url = format!(
            "{}/collections/{}",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let patch_payload = serde_json::json!({
            "optimizers_config": {
                "indexing_threshold": threshold
            }
        });

        let mut req = self.client.patch(&patch_url).json(&patch_payload);
        if let Some(ref key) = qdrant_config.api_key {
            req = req.header("api-key", key);
        }

        let res = req.send().await?;
        res.error_for_status()?;
        Ok(())
    }

    /// Upserts a batch of nodes into the Qdrant database.
    pub async fn upsert_nodes(&self, nodes: &[graphify_core::Node]) -> Result<()> {
        let lt_config = &self.config.memory.long_term;
        if !lt_config.enabled {
            return Ok(());
        }

        // ponytail: filter out fine-grained nodes based on config to optimize memory density & RAG quality
        let filtered_nodes: Vec<&graphify_core::Node> = nodes
            .iter()
            .filter(|node| lt_config.index_kinds.contains(&node.kind))
            .collect();

        if filtered_nodes.is_empty() {
            return Ok(());
        }

        self.embed_and_upsert(&filtered_nodes).await
    }

    /// Incremental sync: delete obsolete vectors for changed/deleted files, then
    /// embed + upsert only nodes belonging to those files. Unchanged files are skipped.
    pub async fn sync_nodes(
        &self,
        nodes: &[graphify_core::Node],
        changed_files: &std::collections::HashSet<String>,
    ) -> Result<()> {
        let lt_config = &self.config.memory.long_term;
        if !lt_config.enabled {
            return Ok(());
        }

        // remove stale vectors for every changed/deleted source file first
        self.delete_points_by_source_files(changed_files).await?;

        // only re-embed nodes whose source file changed (and pass the kind filter)
        let filtered_nodes: Vec<&graphify_core::Node> = nodes
            .iter()
            .filter(|node| lt_config.index_kinds.contains(&node.kind))
            .filter(|node| changed_files.contains(&node.source_file))
            .collect();

        if filtered_nodes.is_empty() {
            return Ok(());
        }

        self.embed_and_upsert(&filtered_nodes).await
    }

    /// Deletes all Qdrant points whose payload `source_file` matches any of `files`.
    async fn delete_points_by_source_files(&self, files: &std::collections::HashSet<String>) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let lt_config = &self.config.memory.long_term;
        let qdrant_config = &lt_config.qdrant;
        let collection = &qdrant_config.collection;

        if let Some(ref client) = self.grpc_client {
            use qdrant_client::qdrant::{Condition, DeletePointsBuilder, Filter};

            // ponytail: OR-match on source_file keeps a single delete RPC per batch of files
            let filter = Filter::should(
                files.iter().cloned().map(|f| Condition::matches("source_file", f)),
            );
            client
                .delete_points(DeletePointsBuilder::new(collection).points(filter).wait(false))
                .await?;
            return Ok(());
        }

        // REST fallback: POST /collections/{c}/points/delete with a payload filter
        let url = format!(
            "{}/collections/{}/points/delete",
            qdrant_config.url.trim_end_matches('/'),
            collection
        );
        let filter_payload = serde_json::json!({
            "should": files.iter().map(|f| serde_json::json!({
                "key": "source_file",
                "match": { "value": f }
            })).collect::<Vec<_>>()
        });
        let req = self.client.post(&url).json(&serde_json::json!({ "filter": filter_payload }));
        let req = if let Some(ref key) = qdrant_config.api_key {
            req.header("api-key", key)
        } else {
            req
        };
        let res = req.send().await?;
        res.error_for_status()?;
        Ok(())
    }

    /// Shared embed + transport-agnostic upsert core used by full and incremental indexing.
    async fn embed_and_upsert(&self, filtered_nodes: &[&graphify_core::Node]) -> Result<()> {
        let lt_config = &self.config.memory.long_term;
        let qdrant_config = &lt_config.qdrant;

        // ponytail: disable HNSW index construction during massive bulk upload for high ingestion speed
        let _ = self.set_indexing_threshold(0).await;

        let texts: Vec<String> = filtered_nodes.iter().map(|node| node_to_embed_text(node)).collect();
        let embeddings = self.get_embeddings(&texts).await?;

        if let Some(ref client) = self.grpc_client {
            self.upsert_grpc(client, &qdrant_config.collection, filtered_nodes, embeddings).await?;
        } else {
            self.upsert_rest(qdrant_config, filtered_nodes, embeddings).await?;
        }

        // ponytail: restore HNSW optimizer indexing threshold on completed bulk ingestion
        let _ = self.set_indexing_threshold(qdrant_config.indexing_threshold).await;

        Ok(())
    }

    async fn upsert_grpc(
        &self,
        client: &qdrant_client::Qdrant,
        collection: &str,
        nodes: &[&graphify_core::Node],
        embeddings: Vec<Vec<f32>>,
    ) -> Result<()> {
        use qdrant_client::qdrant::{PointStruct, UpsertPointsBuilder};
        use qdrant_client::Payload;

        let mut points = Vec::new();
        for (node, vector) in nodes.iter().zip(embeddings) {
            let mut map = std::collections::HashMap::new();
            map.insert("node_id".to_string(), serde_json::Value::String(node.id.0.clone()));
            map.insert("label".to_string(), serde_json::Value::String(node.label.clone()));
            map.insert("file_type".to_string(), serde_json::Value::String(format!("{:?}", node.file_type)));
            map.insert("kind".to_string(), serde_json::Value::String(node.kind.clone()));
            map.insert("language".to_string(), serde_json::Value::String(node.language.clone()));
            map.insert("source_file".to_string(), serde_json::Value::String(node.source_file.clone()));
            map.insert("start_line".to_string(), serde_json::Value::Number(serde_json::Number::from(node.start_line)));
            map.insert("end_line".to_string(), serde_json::Value::Number(serde_json::Number::from(node.end_line)));
            if let Some(ref desc) = node.description {
                map.insert("description".to_string(), serde_json::Value::String(desc.clone()));
            }

            points.push(PointStruct::new(
                hash_node_id(&node.id.0),
                vector,
                Payload::from(map),
            ));
        }

        // ponytail: chunked upsert prevents gRPC packet size limits and long-timeout requests
        client.upsert_points_chunked(
            UpsertPointsBuilder::new(collection, points).wait(false),
            256,
        ).await?;
        Ok(())
    }

    async fn upsert_rest(
        &self,
        qdrant_config: &crate::config::QdrantConfig,
        nodes: &[&graphify_core::Node],
        embeddings: Vec<Vec<f32>>,
    ) -> Result<()> {
        let url = format!(
            "{}/collections/{}/points",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let mut points = Vec::new();
        for (node, vector) in nodes.iter().zip(embeddings) {
            points.push(serde_json::json!({
                "id": hash_node_id(&node.id.0),
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
            }));
        }

        // ponytail: chunked upsert matches the gRPC path; a single-shot REST upload of
        // ~77k points exceeds Qdrant's max_request_size (default 32MB) and gets HTTP 400
        for (chunk_idx, chunk) in points.chunks(256).enumerate() {
            let req = self.client.put(&url).json(&serde_json::json!({ "points": chunk }));
            let req = if let Some(ref key) = qdrant_config.api_key {
                req.header("api-key", key)
            } else {
                req
            };

            let res = req.send().await?;
            let status = res.status();
            if !status.is_success() {
                let body = res.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "Qdrant REST upsert failed ({status}) for chunk {chunk_idx}: {body}"
                ));
            }
        }
        Ok(())
    }

    /// Query Qdrant for similar nodes (RAG semantic retrieval).
    pub async fn query_similar_nodes(&self, query_text: &str, limit: usize, filter: Option<Value>) -> Result<Vec<Value>> {
        let lt_config = &self.config.memory.long_term;
        if !lt_config.enabled {
            return Ok(Vec::new());
        }

        let qdrant_config = &lt_config.qdrant;

        let query_embeddings = self.get_embeddings(&[query_text.to_string()]).await? ;
        if query_embeddings.is_empty() {
            return Err(anyhow!("Failed to generate query embedding"));
        }
        let vector = &query_embeddings[0];

        // gRPC Path
        if let Some(ref client) = self.grpc_client {
            use qdrant_client::qdrant::QueryPointsBuilder;
            let mut builder = QueryPointsBuilder::new(&qdrant_config.collection)
                .query(vector.clone())
                .limit(limit as u64)
                .with_payload(true);
            if let Some(grpc_filter) = filter.as_ref().and_then(json_to_grpc_filter) {
                builder = builder.filter(grpc_filter);
            }
            let results = client.query(builder).await?;

            let mut out = Vec::new();
            for scored in results.result {
                // Map ScoredPoint back to serde_json Value matching the REST format
                let payload_map = scored.payload;
                let payload_val = if payload_map.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Object(
                        payload_map.into_iter()
                            .map(|(k, v)| (k, v.into_json()))
                            .collect()
                    )
                };

                let point_id_str = scored.id.and_then(|id| {
                    use qdrant_client::qdrant::point_id::PointIdOptions;
                    match id.point_id_options {
                        Some(PointIdOptions::Num(n)) => Some(n.to_string()),
                        Some(PointIdOptions::Uuid(u)) => Some(u),
                        None => None,
                    }
                });

                let res_json = serde_json::json!({
                    "id": point_id_str.map_or(serde_json::Value::Null, serde_json::Value::String),
                    "score": scored.score,
                    "payload": payload_val,
                });
                out.push(res_json);
            }
            return Ok(out);
        }

        // REST Fallback Path
        let url = format!(
            "{}/collections/{}/points/search",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );

        let mut payload = serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
            "with_vector": false,
        });
        if let Some(ref f) = filter {
            payload["filter"] = f.clone();
        }

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

// ponytail: deterministic u64 point id from node id, makes re-indexing idempotent
fn hash_node_id(id: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

// ponytail: single-source text renderer for embedding, shared by all transports
fn node_to_embed_text(node: &graphify_core::Node) -> String {
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
}

// ponytail: helper to dynamically map REST style JSON filters into pure proto-generated gRPC filter structures
fn json_to_grpc_filter(val: &Value) -> Option<qdrant_client::qdrant::Filter> {
    use qdrant_client::qdrant::Filter;

    let obj = val.as_object()?;
    let mut grpc_filter = Filter::default();

    for (key, target) in [("must", &mut grpc_filter.must), ("should", &mut grpc_filter.should), ("must_not", &mut grpc_filter.must_not)] {
        if let Some(arr) = obj.get(key).and_then(Value::as_array) {
            *target = arr.iter().filter_map(json_to_condition).collect();
        }
    }

    Some(grpc_filter)
}

fn json_to_condition(val: &Value) -> Option<qdrant_client::qdrant::Condition> {
    use qdrant_client::qdrant::Condition;

    let cond_obj = val.as_object()?;
    let key = cond_obj.get("key")?.as_str()?.to_string();
    let match_inner = cond_obj.get("match")?.get("value")?;

    let condition = match match_inner {
        Value::String(s) => Condition::matches(key, s.clone()),
        Value::Bool(b) => Condition::matches(key, *b),
        Value::Number(n) => Condition::matches(key, n.as_i64()?),
        _ => return None,
    };

    Some(condition)
}
