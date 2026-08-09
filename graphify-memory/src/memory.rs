use crate::config::LongTermMemoryConfig;
use crate::local_process::{DEFAULT_TARGET, QdrantLocalProcess};
use anyhow::{Context as _, Result, anyhow};
use fastembed::{Bgem3Embedding, Bgem3InitOptions, Bgem3Model};
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

/// Input for memory queries with workspace scoping
#[derive(Debug, Clone)]
pub struct MemoryQueryInput {
    /// Workspace key to scope the query
    pub workspace_key: String,
    /// Query text for semantic search
    pub query: String,
    /// Maximum number of results to return (will be clamped)
    pub limit: usize,
}

/// Result of a memory query that doesn't expose storage-specific types
#[derive(Debug, Clone)]
pub enum MemoryQueryResult {
    /// Found matching nodes
    Found(Vec<MemoryNode>),
    /// No matching nodes found
    NotFound,
    /// Semantic memory is unavailable (provider disabled or error)
    Unavailable(String),
}

/// A memory node with essential fields for semantic search results.
///
/// Serialization is the public result contract of the restricted memory
/// query API: it carries stable Graphify identifiers and bounded context,
/// and deliberately excludes storage internals (Qdrant collection names,
/// point IDs, credentials, embedding-provider configuration).
#[derive(Debug, Clone, Serialize)]
pub struct MemoryNode {
    /// Unique identifier of the node
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Type of file (e.g., source, documentation)
    pub file_type: String,
    /// Kind of node (e.g., function, class, variable)
    pub kind: String,
    /// Programming language
    pub language: String,
    /// Source file path
    pub source_file: String,
    /// Start line number
    pub start_line: u32,
    /// End line number
    pub end_line: u32,
    /// Optional description
    pub description: Option<String>,
}

impl MemoryNode {
    fn from_payload(payload: &serde_json::Value) -> Self {
        let get_str = |k: &str| {
            payload
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let get_opt_str = |k: &str| {
            payload
                .get(k)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        let get_u32 = |k: &str| {
            payload
                .get(k)
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0)
        };
        Self {
            id: get_str("node_id"),
            label: get_str("label"),
            file_type: get_str("file_type"),
            kind: get_str("kind"),
            language: get_str("language"),
            source_file: get_str("source_file"),
            start_line: get_u32("start_line"),
            end_line: get_u32("end_line"),
            description: get_opt_str("description"),
        }
    }
}

/// Storage-agnostic semantic search over core memory.
///
/// Implementations supply the raw scoped search ([`Self::search`]); the
/// default [`Self::query`] validates inputs, clamps the result limit, and
/// maps outcomes to [`MemoryQueryResult`] without exposing storage internals.
// allow: native async fn in trait (Rust 2024). `Send + Sync` supertrait makes
// the returned futures `Send`, and this trait is never dynamically dispatched,
// so explicit auto-trait bounds on the futures are unnecessary.
#[allow(async_fn_in_trait)]
pub trait MemorySearcher: Send + Sync {
    /// Maximum result limit enforced by [`Self::query`].
    const MAX_QUERY_LIMIT: usize = 1000;

    /// Whether semantic memory is currently available (e.g., provider enabled).
    fn is_available(&self) -> bool {
        true
    }

    /// Execute a search scoped to `workspace_key`, returning at most `limit`
    /// matches. Implementations MUST filter by `workspace_key`.
    async fn search(
        &self,
        workspace_key: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryNode>>;

    /// Restricted query: validates inputs, clamps `input.limit` to
    /// [`Self::MAX_QUERY_LIMIT`], and maps results/errors to typed outcomes.
    async fn query(&self, input: MemoryQueryInput) -> Result<MemoryQueryResult> {
        if !self.is_available() {
            return Ok(MemoryQueryResult::Unavailable(
                "Semantic memory is not enabled in configuration".to_string(),
            ));
        }

        if input.workspace_key.is_empty() {
            return Ok(MemoryQueryResult::Unavailable(
                "workspace_key must not be empty".to_string(),
            ));
        }

        if input.query.is_empty() {
            return Ok(MemoryQueryResult::Unavailable(
                "query must not be empty".to_string(),
            ));
        }

        let limit = input.limit.clamp(1, Self::MAX_QUERY_LIMIT);

        match self.search(&input.workspace_key, &input.query, limit).await {
            Ok(nodes) if nodes.is_empty() => Ok(MemoryQueryResult::NotFound),
            Ok(nodes) => Ok(MemoryQueryResult::Found(nodes)),
            Err(e) => Ok(MemoryQueryResult::Unavailable(e.to_string())),
        }
    }
}

/// Active storage backend of a [`QdrantMemoryStore`] (RFC-0004 §1.3 dual-track).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageMode {
    /// Connected to the configured external server.
    ServerUrl(String),
    /// Serving from the managed local standalone process.
    LocalProcess,
}

/// Decides the active storage mode from a (possibly injected) health probe
/// result — pure decision, unit-testable with fake health (tasks 3.6).
/// Fallback disabled: always the server URL; a dead server surfaces as
/// `Unavailable` at query time (existing semantics), never a hard failure.
fn choose_mode(server_healthy: bool, fallback_enabled: bool) -> StorageMode {
    if server_healthy || !fallback_enabled {
        StorageMode::ServerUrl(String::new())
    } else {
        StorageMode::LocalProcess
    }
}

/// Probes the configured server's `/healthz` with a bounded 10ms timeout
/// (tasks 3.2). A healthy answer means "use the server"; any failure —
/// timeout, connection refused, non-200 — means "fall back to local".
async fn server_healthy(url: &str) -> bool {
    let client = Client::builder()
        .timeout(Duration::from_millis(10))
        .build()
        .unwrap_or_default();
    let healthz = format!("{}/healthz", url.trim_end_matches('/'));
    client
        .get(healthz)
        .send()
        .await
        .is_ok_and(|resp| resp.status().is_success())
}

pub struct QdrantMemoryStore {
    config: LongTermMemoryConfig,
    embedding_concurrency: Option<usize>,
    storage_mode: StorageMode,
    // Kept alive for the store's lifetime: dropping the handle kills the child.
    local_process: Option<QdrantLocalProcess>,
    client: Client,
    grpc_client: Option<qdrant_client::Qdrant>,
    // ponytail: Arc + Mutex allows safe sharing and on-demand initialization across tokio threads
    fastembed_model: std::sync::Arc<std::sync::Mutex<Option<Bgem3Embedding>>>,
}

impl QdrantMemoryStore {
    pub fn new(config: LongTermMemoryConfig, embedding_concurrency: Option<usize>) -> Self {
        let server_url = config.qdrant.url.clone();
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        let grpc_client = if config.qdrant.grpc {
            let mut url = config.qdrant.url.clone();
            if config.qdrant.local_fallback_enabled
                && url.contains(&format!(":{}", config.qdrant.local_http_port))
            {
                url = url.replace(
                    &format!(":{}", config.qdrant.local_http_port),
                    &format!(":{}", config.qdrant.local_grpc_port),
                );
            } else if url.contains(":6333") {
                url = url.replace(":6333", ":6334");
            }
            let mut builder = qdrant_client::Qdrant::from_url(&url);
            if let Some(ref api_key) = config.qdrant.api_key {
                builder = builder.api_key(api_key.as_str());
            }
            builder.build().ok()
        } else {
            None
        };
        Self {
            config,
            embedding_concurrency,
            storage_mode: StorageMode::ServerUrl(server_url),
            local_process: None,
            client,
            grpc_client,
            fastembed_model: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Dual-track construction (RFC-0004 §1.3): probe the configured server;
    /// use it when healthy, otherwise spawn the managed local process when
    /// fallback is enabled. Construction never hard-fails on server
    /// unavailability — with fallback off it degrades to the existing
    /// `Unavailable` semantics, with fallback on it serves from local.
    pub async fn init_with_fallback(
        config: LongTermMemoryConfig,
        embedding_concurrency: Option<usize>,
    ) -> Result<Self> {
        let healthy = server_healthy(&config.qdrant.url).await;
        match choose_mode(healthy, config.qdrant.local_fallback_enabled) {
            StorageMode::ServerUrl(_) => Ok(Self::new(config, embedding_concurrency)),
            StorageMode::LocalProcess => Self::spawn_local_store(config, embedding_concurrency).await,
        }
    }

    /// Spawns (downloading first if needed) the managed local Qdrant process
    /// and points a fresh store at it.
    async fn spawn_local_store(
        config: LongTermMemoryConfig,
        embedding_concurrency: Option<usize>,
    ) -> Result<Self> {
        // Fall back: ensure the binary, spawn the process, point the store at it.
        let bin_dir = QdrantLocalProcess::resolve_path(&config.qdrant.local_bin_dir);
        let storage_dir = QdrantLocalProcess::resolve_path(&config.qdrant.local_storage_dir);
        std::fs::create_dir_all(&bin_dir).context("creating qdrant bin dir")?;
        std::fs::create_dir_all(&storage_dir).context("creating qdrant storage dir")?;

        let bin_path = bin_dir.join("qdrant");
        if !bin_path.exists() {
            let client = Client::builder()
                .timeout(Duration::from_mins(1))                .build()
                .unwrap_or_default();
            QdrantLocalProcess::download_binary(
                &client,
                &config.qdrant.local_version,
                DEFAULT_TARGET,
                &bin_dir,
            )
            .await?;
        }

        let process = QdrantLocalProcess::spawn(
            &bin_path,
            &storage_dir,
            config.qdrant.local_http_port,
            config.qdrant.local_grpc_port,
        )?;

        let mut local_config = config.clone();
        local_config.qdrant.url = format!("http://127.0.0.1:{}", config.qdrant.local_http_port);
        local_config.qdrant.grpc = true;

        let mut store = Self::new(local_config, embedding_concurrency);
        store.storage_mode = StorageMode::LocalProcess;
        store.local_process = Some(process);
        Ok(store)
    }

    /// Active storage mode, exposed read-only for diagnostics (TUI status line).
    pub fn storage_mode(&self) -> &StorageMode {
        &self.storage_mode
    }

    fn get_or_init_fastembed(
        &self,
    ) -> Result<std::sync::Arc<std::sync::Mutex<Option<Bgem3Embedding>>>> {
        {
            let mut lock = self
                .fastembed_model
                .lock()
                .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
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

                if let Some(concurrency) = self.embedding_concurrency {
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
        let embedding_config = &self.config.embedding;

        if embedding_config.provider == "fastembed" {
            let texts = texts.to_vec();
            let model_arc = self.get_or_init_fastembed()?;
            let results = tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
                let mut lock = model_arc
                    .lock()
                    .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
                let model = lock
                    .as_mut()
                    .ok_or_else(|| anyhow!("Model not initialized"))?;
                let output = model
                    .embed(texts, None)
                    .map_err(|e| anyhow!("fastembed error: {}", e))?;
                drop(lock); // ponytail: early drop to avoid significant_drop_tightening
                Ok(output.dense)
            })
            .await??;
            return Ok(results);
        }

        if embedding_config.provider != "ollama" {
            return Err(anyhow!(
                "Unsupported embedding provider: {}",
                embedding_config.provider
            ));
        }

        let url = format!(
            "{}/api/embeddings",
            embedding_config.endpoint.trim_end_matches('/')
        );
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
        let mut join_set = tokio::task::JoinSet::new();

        for (idx, text) in texts.iter().cloned().enumerate() {
            let client = self.client.clone();
            let url = url.clone();
            let model = embedding_config.model.clone();
            let sem = semaphore.clone();

            join_set.spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .map_err(|e| anyhow!("Semaphore acquire failed: {}", e))?;
                let payload = serde_json::json!({
                    "model": model,
                    "prompt": text,
                });

                let res = client.post(&url).json(&payload).send().await?;
                let res = res.error_for_status()?;
                let body: Value = res.json().await?;

                body["embedding"].as_array().map_or_else(
                    || Err(anyhow!("Ollama response missing embedding array")),
                    |arr| {
                        #[allow(clippy::cast_possible_truncation)]
                        let vec: Vec<f32> = arr
                            .iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect();
                        Ok::<(usize, Vec<f32>), anyhow::Error>((idx, vec))
                    },
                )
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
        if self.grpc_client.is_some() {
            self.ensure_grpc_collection().await
        } else {
            self.ensure_rest_collection().await
        }
    }

    async fn ensure_grpc_collection(&self) -> Result<()> {
        use qdrant_client::qdrant::{
            CreateCollectionBuilder, CreateFieldIndexCollectionBuilder, Distance, FieldType,
            OptimizersConfigDiffBuilder, VectorParamsBuilder,
        };

        let Some(client) = self.grpc_client.as_ref() else {
            return Ok(());
        };
        let qdrant_config = &self.config.qdrant;
        let embedding_config = &self.config.embedding;
        let exists = client
            .collection_exists(&qdrant_config.collection)
            .await
            .unwrap_or(false);
        if exists {
            return Ok(());
        }

        let dist = match qdrant_config.distance.to_uppercase().as_str() {
            "DOT" => Distance::Dot,
            "EUCLIDEAN" => Distance::Euclid,
            _ => Distance::Cosine,
        };
        client
            .create_collection(
                CreateCollectionBuilder::new(&qdrant_config.collection)
                    .vectors_config(VectorParamsBuilder::new(
                        embedding_config.vector_size as u64,
                        dist,
                    ))
                    .optimizers_config(
                        OptimizersConfigDiffBuilder::default().indexing_threshold(0),
                    ),
            )
            .await?;

        for field in ["source_file", "kind", "language"] {
            let _ = client
                .create_field_index(
                    CreateFieldIndexCollectionBuilder::new(
                        &qdrant_config.collection,
                        field,
                        FieldType::Keyword,
                    )
                    .wait(true),
                )
                .await;
        }
        eprintln!(
            "[graphify] Auto-created Qdrant collection (gRPC): {}",
            qdrant_config.collection
        );
        Ok(())
    }

    async fn ensure_rest_collection(&self) -> Result<()> {
        let qdrant_config = &self.config.qdrant;
        let embedding_config = &self.config.embedding;
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

        eprintln!(
            "[graphify] Auto-created Qdrant collection: {}",
            qdrant_config.collection
        );
        Ok(())
    }

    /// Deletes the Qdrant collection if it exists.
    pub async fn delete_collection(&self) -> Result<()> {
        let lt_config = &self.config;
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
        let lt_config = &self.config;
        let qdrant_config = &lt_config.qdrant;

        if let Some(ref client) = self.grpc_client {
            use qdrant_client::qdrant::{OptimizersConfigDiffBuilder, UpdateCollectionBuilder};
            client
                .update_collection(
                    UpdateCollectionBuilder::new(&qdrant_config.collection).optimizers_config(
                        OptimizersConfigDiffBuilder::default().indexing_threshold(threshold as u64),
                    ),
                )
                .await?;
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
    pub async fn upsert_nodes(
        &self,
        nodes: &[graphify_core::Node],
        workspace_key: &str,
    ) -> Result<()> {
        let lt_config = &self.config;
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

        self.embed_and_upsert(&filtered_nodes, workspace_key).await
    }

    /// Incremental sync: delete obsolete vectors for changed/deleted files, then
    /// embed + upsert only nodes belonging to those files. Unchanged files are skipped.
    pub async fn sync_nodes(
        &self,
        nodes: &[graphify_core::Node],
        workspace_key: &str,
        changed_files: &std::collections::HashSet<String>,
    ) -> Result<()> {
        let lt_config = &self.config;
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

        self.embed_and_upsert(&filtered_nodes, workspace_key).await
    }

    /// Deletes all Qdrant points whose payload `source_file` matches any of `files`.
    async fn delete_points_by_source_files(
        &self,
        files: &std::collections::HashSet<String>,
    ) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let lt_config = &self.config;
        let qdrant_config = &lt_config.qdrant;
        let collection = &qdrant_config.collection;

        if let Some(ref client) = self.grpc_client {
            use qdrant_client::qdrant::{Condition, DeletePointsBuilder, Filter};

            // ponytail: OR-match on source_file keeps a single delete RPC per batch of files
            let filter = Filter::should(
                files
                    .iter()
                    .cloned()
                    .map(|f| Condition::matches("source_file", f)),
            );
            client
                .delete_points(
                    DeletePointsBuilder::new(collection)
                        .points(filter)
                        .wait(false),
                )
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
        let req = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "filter": filter_payload }));
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
    async fn embed_and_upsert(
        &self,
        filtered_nodes: &[&graphify_core::Node],
        workspace_key: &str,
    ) -> Result<()> {
        let lt_config = &self.config;
        let qdrant_config = &lt_config.qdrant;

        // ponytail: disable HNSW index construction during massive bulk upload for high ingestion speed
        let _ = self.set_indexing_threshold(0).await;

        let texts: Vec<String> = filtered_nodes
            .iter()
            .map(|node| node_to_embed_text(node))
            .collect();
        let embeddings = self.get_embeddings(&texts).await?;

        if let Some(ref client) = self.grpc_client {
            self.upsert_grpc(
                client,
                &qdrant_config.collection,
                filtered_nodes,
                embeddings,
                workspace_key,
            )
            .await?;
        } else {
            self.upsert_rest(qdrant_config, filtered_nodes, embeddings, workspace_key)
                .await?;
        }

        // ponytail: restore HNSW optimizer indexing threshold on completed bulk ingestion
        let _ = self
            .set_indexing_threshold(qdrant_config.indexing_threshold)
            .await;

        Ok(())
    }

    async fn upsert_grpc(
        &self,
        client: &qdrant_client::Qdrant,
        collection: &str,
        nodes: &[&graphify_core::Node],
        embeddings: Vec<Vec<f32>>,
        workspace_key: &str,
    ) -> Result<()> {
        use qdrant_client::Payload;
        use qdrant_client::qdrant::{PointStruct, UpsertPointsBuilder};

        let mut points = Vec::new();
        for (node, vector) in nodes.iter().zip(embeddings) {
            let mut map = std::collections::HashMap::new();
            map.insert(
                "workspace_key".to_string(),
                serde_json::Value::String(workspace_key.to_string()),
            );
            map.insert(
                "node_id".to_string(),
                serde_json::Value::String(node.id.0.clone()),
            );
            map.insert(
                "label".to_string(),
                serde_json::Value::String(node.label.clone()),
            );
            map.insert(
                "file_type".to_string(),
                serde_json::Value::String(format!("{:?}", node.file_type)),
            );
            map.insert(
                "kind".to_string(),
                serde_json::Value::String(node.kind.clone()),
            );
            map.insert(
                "language".to_string(),
                serde_json::Value::String(node.language.clone()),
            );
            map.insert(
                "source_file".to_string(),
                serde_json::Value::String(node.source_file.clone()),
            );
            map.insert(
                "start_line".to_string(),
                serde_json::Value::Number(serde_json::Number::from(node.start_line)),
            );
            map.insert(
                "end_line".to_string(),
                serde_json::Value::Number(serde_json::Number::from(node.end_line)),
            );
            if let Some(ref desc) = node.description {
                map.insert(
                    "description".to_string(),
                    serde_json::Value::String(desc.clone()),
                );
            }

            points.push(PointStruct::new(
                hash_node_id(&node.id.0),
                vector,
                Payload::from(map),
            ));
        }

        // ponytail: chunked upsert prevents gRPC packet size limits and long-timeout requests
        client
            .upsert_points_chunked(
                UpsertPointsBuilder::new(collection, points).wait(false),
                256,
            )
            .await?;
        Ok(())
    }

    async fn upsert_rest(
        &self,
        qdrant_config: &crate::config::QdrantConfig,
        nodes: &[&graphify_core::Node],
        embeddings: Vec<Vec<f32>>,
        workspace_key: &str,
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
                    "workspace_key": workspace_key,
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
            let req = self
                .client
                .put(&url)
                .json(&serde_json::json!({ "points": chunk }));
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
    pub async fn query_similar_nodes(
        &self,
        query_text: &str,
        limit: usize,
        filter: Option<Value>,
    ) -> Result<Vec<Value>> {
        let lt_config = &self.config;
        if !lt_config.enabled {
            return Ok(Vec::new());
        }

        let qdrant_config = &lt_config.qdrant;

        let query_embeddings = self.get_embeddings(&[query_text.to_string()]).await?;
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
                        payload_map
                            .into_iter()
                            .map(|(k, v)| (k, v.into_json()))
                            .collect(),
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

        body["result"]
            .as_array()
            .map_or_else(|| Ok(Vec::new()), |result_list| Ok(result_list.clone()))
    }

    /// Restricted query method that validates inputs, applies `workspace_key` filter,
    /// and returns typed results. Returns explicit error when semantic memory is unavailable.
    pub async fn query_memory(&self, input: MemoryQueryInput) -> Result<MemoryQueryResult> {
        MemorySearcher::query(self, input).await
    }
}

impl MemorySearcher for QdrantMemoryStore {
    fn is_available(&self) -> bool {
        self.config.enabled
    }

    async fn search(
        &self,
        workspace_key: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryNode>> {
        let lt_config = &self.config;
        let qdrant_config = &lt_config.qdrant;

        let query_embeddings = self
            .get_embeddings(std::slice::from_ref(&query.to_string()))
            .await?;
        if query_embeddings.is_empty() {
            return Err(anyhow!("Failed to generate query embedding"));
        }
        let vector = &query_embeddings[0];

        // Build workspace_key filter
        let workspace_filter = serde_json::json!({
            "must": [
                {
                    "key": "workspace_key",
                    "match": { "value": workspace_key }
                }
            ]
        });

        // gRPC Path
        if let Some(ref client) = self.grpc_client {
            use qdrant_client::qdrant::QueryPointsBuilder;
            let mut builder = QueryPointsBuilder::new(&qdrant_config.collection)
                .query(vector.clone())
                .limit(limit as u64)
                .with_payload(true);
            if let Some(grpc_filter) = json_to_grpc_filter(&workspace_filter) {
                builder = builder.filter(grpc_filter);
            }
            let results = client.query(builder).await?;

            let mut nodes = Vec::new();
            for scored in results.result {
                let payload_map = scored.payload;
                let payload_val = if payload_map.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Object(
                        payload_map
                            .into_iter()
                            .map(|(k, v)| (k, v.into_json()))
                            .collect(),
                    )
                };

                let node = MemoryNode::from_payload(&payload_val);
                nodes.push(node);
            }

            return Ok(nodes);
        }

        // REST Fallback Path
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
            "filter": workspace_filter,
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

        let mut nodes = Vec::new();
        if let Some(result_list) = body["result"].as_array() {
            for item in result_list {
                let node = MemoryNode::from_payload(&item["payload"]);
                nodes.push(node);
            }
        }

        Ok(nodes)
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

    for (key, target) in [
        ("must", &mut grpc_filter.must),
        ("should", &mut grpc_filter.should),
        ("must_not", &mut grpc_filter.must_not),
    ] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    /// Records search invocations for verifying scoping and limit propagation.
    struct MockSearcher {
        available: bool,
        recorded: Arc<Mutex<Vec<(String, String, usize)>>>,
        results: Result<Vec<MemoryNode>>,
    }

    impl MockSearcher {
        fn new(results: Result<Vec<MemoryNode>>) -> Self {
            Self {
                available: true,
                recorded: Arc::new(Mutex::new(Vec::new())),
                results,
            }
        }
    }

    impl MemorySearcher for MockSearcher {
        fn is_available(&self) -> bool {
            self.available
        }

        async fn search(
            &self,
            workspace_key: &str,
            query: &str,
            limit: usize,
        ) -> Result<Vec<MemoryNode>> {
            self.recorded
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((workspace_key.to_string(), query.to_string(), limit));
            match &self.results {
                Ok(nodes) => Ok(nodes.clone()),
                Err(e) => Err(anyhow!("{e}")),
            }
        }
    }

    fn sample_node() -> MemoryNode {
        MemoryNode {
            id: "node-1".to_string(),
            label: "memory_query".to_string(),
            file_type: "code".to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            source_file: "memory.rs".to_string(),
            start_line: 10,
            end_line: 20,
            description: None,
        }
    }

    fn input(workspace_key: &str, limit: usize) -> MemoryQueryInput {
        MemoryQueryInput {
            workspace_key: workspace_key.to_string(),
            query: "how does memory work".to_string(),
            limit,
        }
    }

    fn lt_config() -> LongTermMemoryConfig {
        LongTermMemoryConfig::default()
    }

    #[tokio::test]
    async fn test_query_scopes_to_workspace_key() -> Result<()> {
        let searcher = MockSearcher::new(Ok(vec![sample_node()]));

        let result = searcher.query(input("ws-alpha", 5)).await?;
        assert!(matches!(result, MemoryQueryResult::Found(_)));

        let calls = searcher.recorded.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "ws-alpha");
        assert_eq!(calls[0].2, 5);
        drop(calls);
        Ok(())
    }

    #[tokio::test]
    async fn test_query_clamps_limit_to_max() -> Result<()> {
        let searcher = MockSearcher::new(Ok(vec![sample_node()]));

        searcher
            .query(input(
                "ws-alpha",
                <MockSearcher as MemorySearcher>::MAX_QUERY_LIMIT + 100,
            ))
            .await?;

        let calls = searcher.recorded.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            calls[0].2,
            <MockSearcher as MemorySearcher>::MAX_QUERY_LIMIT
        );
        drop(calls);
        Ok(())
    }

    #[tokio::test]
    async fn test_query_clamps_limit_to_min_one() -> Result<()> {
        let searcher = MockSearcher::new(Ok(vec![sample_node()]));

        searcher.query(input("ws-alpha", 0)).await?;

        let calls = searcher.recorded.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(calls[0].2, 1);
        drop(calls);
        Ok(())
    }

    #[tokio::test]
    async fn test_query_unavailable_when_disabled() -> Result<()> {
        let mut searcher = MockSearcher::new(Ok(Vec::new()));
        searcher.available = false;

        let result = searcher.query(input("ws-alpha", 5)).await?;
        assert!(matches!(result, MemoryQueryResult::Unavailable(_)));

        // search must not be reached when unavailable
        let calls = searcher.recorded.lock().unwrap_or_else(|e| e.into_inner());
        assert!(calls.is_empty());
        drop(calls);
        Ok(())
    }

    #[tokio::test]
    async fn test_query_rejects_empty_workspace_key() -> Result<()> {
        let searcher = MockSearcher::new(Ok(Vec::new()));

        let result = searcher.query(input("", 5)).await?;
        assert!(matches!(result, MemoryQueryResult::Unavailable(_)));

        let calls = searcher.recorded.lock().unwrap_or_else(|e| e.into_inner());
        assert!(calls.is_empty());
        drop(calls);
        Ok(())
    }

    #[tokio::test]
    async fn test_query_not_found_on_empty_results() -> Result<()> {
        let searcher = MockSearcher::new(Ok(Vec::new()));

        let result = searcher.query(input("ws-alpha", 5)).await?;
        assert!(matches!(result, MemoryQueryResult::NotFound));
        Ok(())
    }

    #[tokio::test]
    async fn test_query_unavailable_on_search_error() -> Result<()> {
        let searcher = MockSearcher::new(Err(anyhow!("provider down")));

        let result = searcher.query(input("ws-alpha", 5)).await?;
        assert!(
            matches!(result, MemoryQueryResult::Unavailable(msg) if msg.contains("provider down"))
        );
        Ok(())
    }

    // ── Task 3: init_with_fallback dual-track (RFC-0004 §1.3) ──────────────

    #[test]
    fn test_choose_mode_healthy_prefers_server() {
        assert!(matches!(choose_mode(true, true), StorageMode::ServerUrl(_)));
        assert!(matches!(choose_mode(true, false), StorageMode::ServerUrl(_)));
    }

    #[test]
    fn test_choose_mode_dead_server_with_fallback_goes_local() {
        assert!(matches!(
            choose_mode(false, true),
            StorageMode::LocalProcess
        ));
    }

    #[test]
    fn test_choose_mode_dead_server_without_fallback_stays_server() {
        // Fallback disabled: keep ServerUrl; unavailability surfaces at query
        // time as MemoryQueryResult::Unavailable, never a hard failure.
        assert!(matches!(
            choose_mode(false, false),
            StorageMode::ServerUrl(_)
        ));
    }

    #[tokio::test]
    async fn test_init_with_fallback_disabled_equals_new() -> Result<()> {
        // local_fallback_enabled=false must construct identically to new():
        // no probe, no process, ServerUrl mode.
        let mut config = lt_config();
        config.qdrant.local_fallback_enabled = false;
        let store = QdrantMemoryStore::init_with_fallback(config, None).await?;
        assert!(matches!(store.storage_mode(), StorageMode::ServerUrl(_)));
        assert!(store.local_process.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_server_healthy_probe_accepts_healthz() -> Result<()> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nOK");
            }
        });
        let url = format!("http://{addr}");
        assert!(server_healthy(&url).await);
        Ok(())
    }

    #[tokio::test]
    async fn test_server_healthy_probe_rejects_dead_port() -> Result<()> {
        // Bind then drop — nothing listens on that port.
        let addr = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
        let url = format!("http://{addr}");
        assert!(!server_healthy(&url).await);
        Ok(())
    }
}
