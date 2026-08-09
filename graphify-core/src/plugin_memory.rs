//! Plugin-domain memory contracts.
//!
//! Versioned data contracts for plugin-owned domain knowledge (RFC-0004 §2.2).
//! These types are pure serde data — no storage, network, or LLM dependencies —
//! so `graphify-core` stays dependency-free and plugins can construct records
//! without touching the memory service. The storage boundary itself lives in
//! `graphify-llm` (`PluginDomainMemory`), which derives the physical namespace
//! from `plugin_id` and never accepts raw collection names or credentials.
//!
//! Payload schemas version independently: each plugin owns its `schema_version`
//! and records from one plugin never acquire mandatory fields from another.

use serde::{Deserialize, Serialize};

/// Versioned envelope wrapping every plugin-domain memory record.
///
/// `format_version` versions the envelope contract itself; `payload` carries
/// plugin-owned data whose own version lives inside the payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMemoryEnvelope<T> {
    pub format_version: u32,
    pub workspace_key: String,
    pub plugin_id: String,
    pub record_id: String,
    pub record_kind: String,
    pub created_at: i64,
    pub source_refs: Vec<String>,
    pub payload: T,
}

impl<T> PluginMemoryEnvelope<T> {
    /// Current envelope contract version.
    pub const FORMAT_VERSION: u32 = 1;

    /// Builds an envelope stamped with the current [`FORMAT_VERSION`](Self::FORMAT_VERSION).
    pub fn new(
        workspace_key: impl Into<String>,
        plugin_id: impl Into<String>,
        record_id: impl Into<String>,
        record_kind: impl Into<String>,
        created_at: i64,
        source_refs: Vec<String>,
        payload: T,
    ) -> Self {
        Self {
            format_version: Self::FORMAT_VERSION,
            workspace_key: workspace_key.into(),
            plugin_id: plugin_id.into(),
            record_id: record_id.into(),
            record_kind: record_kind.into(),
            created_at,
            source_refs,
            payload,
        }
    }
}

/// `OpenDoc` domain payload: document chunk associations and symbol links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenDocPayload {
    pub schema_version: u32,
    pub doc_identity: String,
    pub doc_version: String,
    pub chunk_index: usize,
    pub raw_content: String,
    pub linked_symbols: Vec<String>,
}

impl OpenDocPayload {
    pub const SCHEMA_VERSION: u32 = 1;
}

/// Review domain payload: historical review findings tied to symbols/commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPayload {
    pub schema_version: u32,
    pub review_id: String,
    pub git_commit_sha: Option<String>,
    pub affected_symbols: Vec<String>,
    pub finding_severity: String,
    pub resolution_status: String,
    pub review_comment: String,
}

impl ReviewPayload {
    pub const SCHEMA_VERSION: u32 = 1;
}

/// Strongly-typed query conditions that reconstruct a memory neighborhood.
///
/// Deliberately replaces Qdrant point IDs: re-running these criteria against
/// the memory service re-finds the same records after re-index or collection
/// migration (RFC-0004 §4.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQueryCriteria {
    pub target_symbols: Vec<String>,
    pub domain_categories: Vec<String>,
    pub search_terms: Vec<String>,
}

/// Handoff domain payload: task state and reconstructable memory references.
///
/// `reconstructable_query_metadata` deliberately replaces Qdrant point IDs with
/// query conditions, so snapshots survive memory migrations and rebuilds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffPayload {
    pub schema_version: u32,
    pub task_goal: String,
    pub pinned_node_ids: Vec<String>,
    pub focused_subgraph_toon: String,
    pub reconstructable_query_metadata: MemoryQueryCriteria,
}

impl HandoffPayload {
    pub const SCHEMA_VERSION: u32 = 1;
}

/// Session-tracking container wrapping a [`HandoffPayload`] (two-tier model).
///
/// Tier 1 — [`HandoffPayload`]: domain record persisted to plugin storage.
/// Tier 2 — [`HandoffSnapshot`]: global container tracked by the agent, TUI,
/// and the `SQLite` `` `handoff_registry` ``; carries the TTL window (`expires_at`)
/// used for auto-pruning. Mirrors RFC-0004 §4.2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffSnapshot {
    pub snapshot_id: String,
    pub session_id: String,
    pub workspace_key: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub payload: HandoffPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_roundtrip_with_opendoc_payload() -> Result<(), serde_json::Error> {
        let envelope = PluginMemoryEnvelope::new(
            "ws_backend_core_9921",
            "opendoc",
            "rec-001",
            "doc_chunk_assoc",
            1_786_252_800,
            vec!["docs/spec.md".to_string()],
            OpenDocPayload {
                schema_version: OpenDocPayload::SCHEMA_VERSION,
                doc_identity: "doc_spec_v3.pdf".to_string(),
                doc_version: "v3".to_string(),
                chunk_index: 0,
                raw_content: "…".to_string(),
                linked_symbols: vec!["MemoryConfig".to_string()],
            },
        );
        let encoded = serde_json::to_string(&envelope)?;
        let decoded: PluginMemoryEnvelope<OpenDocPayload> = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, envelope);
        assert_eq!(
            decoded.format_version,
            PluginMemoryEnvelope::<()>::FORMAT_VERSION
        );
        Ok(())
    }

    #[test]
    fn test_payloads_version_independently() {
        // Each payload carries its own schema_version; the envelope does not
        // unify them, so one plugin's evolution cannot leak into another.
        assert_eq!(OpenDocPayload::SCHEMA_VERSION, 1);
        assert_eq!(ReviewPayload::SCHEMA_VERSION, 1);
        assert_eq!(HandoffPayload::SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_handoff_payload_carries_query_metadata_not_point_ids() -> Result<(), serde_json::Error>
    {
        let payload = HandoffPayload {
            schema_version: HandoffPayload::SCHEMA_VERSION,
            task_goal: "wire memory query tool".to_string(),
            pinned_node_ids: vec!["N42".to_string()],
            focused_subgraph_toon: "metadata:\n  format_version: \"1\"\n".to_string(),
            reconstructable_query_metadata: MemoryQueryCriteria {
                target_symbols: vec!["memory_query".to_string()],
                domain_categories: vec!["mcp".to_string()],
                search_terms: vec!["memory query API".to_string()],
            },
        };
        let encoded = serde_json::to_string(&payload)?;
        assert!(
            !encoded.contains("point_id"),
            "point IDs must not leak: {encoded}"
        );
        Ok(())
    }

    #[test]
    fn test_handoff_snapshot_roundtrip() -> Result<(), serde_json::Error> {
        let snapshot = HandoffSnapshot {
            snapshot_id: "snap-001".to_string(),
            session_id: "sess-001".to_string(),
            workspace_key: "ws_backend_core_9921".to_string(),
            created_at: 1_786_252_800,
            expires_at: 1_786_857_600,
            payload: HandoffPayload {
                schema_version: HandoffPayload::SCHEMA_VERSION,
                task_goal: "continue review".to_string(),
                pinned_node_ids: vec!["N42".to_string()],
                focused_subgraph_toon: "metadata:\n  format_version: \"1\"\n".to_string(),
                reconstructable_query_metadata: MemoryQueryCriteria {
                    target_symbols: vec!["ReviewPayload".to_string()],
                    domain_categories: vec!["review".to_string()],
                    search_terms: vec!["finding severity".to_string()],
                },
            },
        };
        let encoded = serde_json::to_string(&snapshot)?;
        let decoded: HandoffSnapshot = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, snapshot);
        assert!(
            !encoded.contains("point_id"),
            "snapshot must not leak point IDs: {encoded}"
        );
        Ok(())
    }
}
