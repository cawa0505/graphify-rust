//! Restricted core-memory query bridge for MCP tools.
//!
//! Exposes the storage-agnostic `MemorySearcher` boundary (memory-plugin-query
//! spec) as a synchronous facade so the stdio MCP server loop can answer
//! `graphify_memory_query` calls without exposing storage internals
//! (collection names, point IDs, credentials, embedding-provider config).

use anyhow::{Result, anyhow};
use graphify_core::derive_workspace_key;
use graphify_llm::config::LLMConfig;
use graphify_memory::{MemoryQueryInput, MemoryQueryResult, QdrantMemoryStore};
use serde_json::Value;
/// Default result limit applied when the caller omits `limit`.
const DEFAULT_LIMIT: usize = 10;
/// Upper bound enforced by the core query API (mirrors core's clamp).
const MAX_LIMIT: usize = 1000;

/// Synchronous facade over the async core-memory store.
///
/// Owns a current-thread tokio runtime; each `query` call blocks on it. The
/// store is constructed best-effort: when no config exists, it falls back to
/// defaults (semantic memory disabled), so the tool reports an explicit
/// unavailable status instead of crashing the server.
pub struct MemoryQueryService {
    runtime: tokio::runtime::Runtime,
    store: QdrantMemoryStore,
}

impl MemoryQueryService {
    /// Builds the service with the configured (or defaulted) memory store.
    ///
    /// # Errors
    /// Only fails if the tokio runtime cannot be constructed.
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let config = LLMConfig::load_from_file("").unwrap_or_default();
        let lt_config = config.memory.long_term.clone();
        let concurrency = config.extraction.concurrency;
        // P4 task 5.3: prefer dual-track init (server, or managed local
        // process when the server is unreachable). `local_fallback_enabled`
        // defaults to false, so this is exactly `QdrantMemoryStore::new`
        // with zero behavior change unless the user opts in. If init fails
        // (e.g. local binary download error), fall back to the plain store;
        // queries then report `Unavailable` instead of failing the service.
        let store = runtime.block_on(async {
            QdrantMemoryStore::init_with_fallback(lt_config.clone(), concurrency)
                .await
                .unwrap_or_else(|_| QdrantMemoryStore::new(lt_config, concurrency))
        });
        Ok(Self { runtime, store })
    }

    /// Runs a restricted memory query and serializes the result.
    ///
    /// The returned JSON carries stable Graphify identifiers and bounded
    /// context only; it never includes Qdrant point IDs, collection names,
    /// credentials, or embedding-provider configuration.
    ///
    /// # Errors
    /// Returns an error for missing/empty required inputs and when semantic
    /// memory is unavailable (explicit status, never a fake empty success).
    pub fn query(&self, args: &Value) -> Result<Value> {
        let input = parse_input(args)?;
        let result = self.runtime.block_on(self.store.query_memory(input))?;
        match result {
            MemoryQueryResult::Found(nodes) => {
                Ok(serde_json::json!({ "status": "found", "nodes": nodes }))
            }
            MemoryQueryResult::NotFound => {
                Ok(serde_json::json!({ "status": "not_found", "nodes": [] }))
            }
            MemoryQueryResult::Unavailable(reason) => {
                Err(anyhow!("semantic memory unavailable: {reason}"))
            }
        }
    }
}

/// Validates and normalizes the tool arguments into a scoped query input.
///
/// `workspace_key` is optional: when omitted, auto-detects from the current
/// directory using the same hash as the graphify index/extract pipeline.
///
/// # Errors
/// Returns an error when `query` is missing or empty, or when both
/// `workspace_key` and auto-detection fail.
fn parse_input(args: &Value) -> Result<MemoryQueryInput> {
    let workspace_key = match args.get("workspace_key").and_then(Value::as_str) {
        Some(key) if !key.trim().is_empty() => key.to_string(),
        _ => {
            // Auto-detect from current working directory
            let cwd = std::env::current_dir()
                .map_err(|e| anyhow!("cannot determine current directory: {e}"))?;
            derive_workspace_key(&cwd)
        }
    };
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required argument: query"))?;
    if workspace_key.trim().is_empty() {
        return Err(anyhow!("workspace_key must not be empty"));
    }
    if query.trim().is_empty() {
        return Err(anyhow!("query must not be empty"));
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map_or(DEFAULT_LIMIT, |v| {
            usize::try_from(v).unwrap_or(DEFAULT_LIMIT)
        })
        .clamp(1, MAX_LIMIT);
    Ok(MemoryQueryInput {
        workspace_key,
        query: query.to_string(),
        limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_node() -> graphify_memory::memory::MemoryNode {
        graphify_memory::memory::MemoryNode {
            id: "node-42".to_string(),
            label: "MemoryConfig".to_string(),
            file_type: "code".to_string(),
            kind: "struct".to_string(),
            language: "rust".to_string(),
            source_file: "crates/core/src/memory.rs".to_string(),
            start_line: 10,
            end_line: 40,
            description: Some("Config for long-term memory".to_string()),
        }
    }

    #[test]
    fn parse_input_applies_defaults_and_clamps() -> Result<()> {
        let input = parse_input(&json!({
            "workspace_key": "ws_backend",
            "query": "memory config"
        }))?;
        assert_eq!(input.workspace_key, "ws_backend");
        assert_eq!(input.query, "memory config");
        assert_eq!(input.limit, DEFAULT_LIMIT);

        let huge = parse_input(&json!({
            "workspace_key": "ws_backend",
            "query": "q",
            "limit": 999_999
        }))?;
        assert_eq!(huge.limit, MAX_LIMIT);

        let zero = parse_input(&json!({
            "workspace_key": "ws_backend",
            "query": "q",
            "limit": 0
        }))?;
        assert_eq!(zero.limit, 1);
        Ok(())
    }

    #[test]
    fn parse_input_rejects_missing_or_empty_inputs() {
        // Missing workspace_key is now auto-detected (not an error)
        let auto_detected = parse_input(&json!({ "query": "q" }));
        assert!(auto_detected.is_ok(), "missing workspace_key should auto-detect");
        let wk = match auto_detected {
            Ok(v) => v.workspace_key,
            Err(_) => return,
        };
        assert!(!wk.is_empty(), "auto-detected workspace_key must not be empty");

        let missing_query = parse_input(&json!({ "workspace_key": "ws" }));
        assert!(missing_query.is_err());

        // Empty workspace_key is now auto-detected (not an error)
        let auto_detected2 = parse_input(&json!({ "workspace_key": "", "query": "q" }));
        assert!(auto_detected2.is_ok(), "empty workspace_key should auto-detect");

        let empty_query = parse_input(&json!({ "workspace_key": "ws", "query": "  " }));
        assert!(empty_query.is_err());
    }

    #[test]
    fn serialized_node_never_exposes_storage_internals() -> Result<()> {
        let value = serde_json::to_value(sample_node())?;
        let keys = value
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>());
        let keys = keys.ok_or_else(|| anyhow!("node did not serialize to an object"))?;
        for forbidden in ["point_id", "collection", "payload", "api_key", "config"] {
            assert!(
                !keys.iter().any(|k| k.to_lowercase().contains(forbidden)),
                "serialized node leaked storage-internal key: {forbidden}"
            );
        }
        assert!(keys.contains(&"id".to_string()));
        assert!(keys.contains(&"source_file".to_string()));
        Ok(())
    }

    #[test]
    fn query_reports_unavailable_when_memory_disabled() -> Result<()> {
        // Default config has semantic memory disabled; the query must return
        // an explicit error rather than an empty successful result.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let store = QdrantMemoryStore::new(LLMConfig::default().memory.long_term, None);
        let service = MemoryQueryService { runtime, store };
        let result = service.query(&json!({
            "workspace_key": "ws_backend",
            "query": "anything"
        }));
        assert!(result.is_err());
        let Err(err) = result else {
            panic!("expected unavailable status, got Ok result");
        };
        assert!(
            err.to_string().contains("unavailable"),
            "expected unavailable status, got: {err}"
        );
        Ok(())
    }
}
