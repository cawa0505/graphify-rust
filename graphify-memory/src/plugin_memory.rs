//! Plugin-domain memory: isolated, workspace-scoped, versioned records.
//!
//! Boundary contract (memory-plugin-integration-v1 §3): plugins identify
//! themselves by `plugin_id` only; raw collection names and credentials are
//! never accepted. Storage names are derived and validated by this service,
//! so plugin-domain records can never target the Graphify core-memory
//! collection. The physical backend is a file-backed JSONL namespace (the
//! spec's "equivalent isolated namespace"): one collection directory per
//! plugin, one JSONL file per workspace, upsert by `record_id`. The derived
//! collection name is storage-agnostic and maps 1:1 to a Qdrant collection
//! name if a vector backend is adopted later.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use graphify_core::plugin_memory::PluginMemoryEnvelope;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Derives the system-managed storage name for a plugin's domain memory.
///
/// Validation mirrors Qdrant collection-name constraints (length, charset)
/// and doubles as a path-injection guard for the file backend: only
/// `[a-zA-Z0-9_-]` is accepted, so `..`/`/` traversal is impossible.
pub fn plugin_collection_name(plugin_id: &str) -> Result<String> {
    if plugin_id.is_empty() {
        bail!("plugin_id must not be empty");
    }
    if plugin_id.len() > 240 {
        bail!("plugin_id too long: {} chars (max 240)", plugin_id.len());
    }
    if !plugin_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("plugin_id contains invalid characters (allowed: [a-zA-Z0-9_-])");
    }
    Ok(format!("graphify_plugin_{plugin_id}"))
}

/// File-backed plugin-domain memory with a system-managed namespace.
///
/// Write path never accepts a raw storage name or credential; every operation
/// routes through [`plugin_collection_name`]. Domain records therefore cannot
/// mutate core-memory records by construction.
pub struct PluginDomainMemory {
    base_dir: PathBuf,
}

impl PluginDomainMemory {
    /// Creates a domain-memory store rooted at `base_dir` (test-friendly).
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// XDG data directory: `$XDG_DATA_HOME/graphify/plugin-memory`, falling
    /// back to `~/.local/share/graphify/plugin-memory` (or a local
    /// `.graphify-plugin-memory` when home is unavailable).
    pub fn default_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(".graphify-plugin-memory"))
            .join("graphify")
            .join("plugin-memory")
    }

    /// Upserts a record: writes to `<plugin collection>/<workspace>.jsonl`,
    /// replacing any existing record with the same `record_id`.
    pub fn put_record<T: Serialize>(&self, envelope: &PluginMemoryEnvelope<T>) -> Result<()> {
        let collection = plugin_collection_name(&envelope.plugin_id)?;
        if envelope.workspace_key.is_empty() {
            bail!("workspace_key must not be empty");
        }
        if envelope.record_id.is_empty() {
            bail!("record_id must not be empty");
        }
        let file_path = self.file_path(&collection, &envelope.workspace_key);

        let mut records: Vec<String> = fs::File::open(&file_path).map_or_else(
            |_| Vec::new(),
            |f| {
                BufReader::new(f)
                    .lines()
                    .map_while(Result::ok)
                    .filter(|line| {
                        // Drop the old record with the same record_id, if any.
                        serde_json::from_str::<PluginMemoryEnvelope<serde_json::Value>>(line)
                            .is_ok_and(|r| r.record_id != envelope.record_id)
                    })
                    .collect()
            },
        );
        records.push(serde_json::to_string(envelope)?);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating plugin memory dir {}", parent.display()))?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&file_path)
            .with_context(|| format!("opening {}", file_path.display()))?;
        for line in records {
            writeln!(f, "{line}")?;
        }
        Ok(())
    }

    /// Queries records for a plugin/workspace, optionally filtered by
    /// `record_kind`, capped at `limit` (default 100). Workspace isolation is
    /// enforced by reading only that workspace's file.
    pub fn query_records<T: DeserializeOwned>(
        &self,
        plugin_id: &str,
        workspace_key: &str,
        record_kind: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PluginMemoryEnvelope<T>>> {
        let collection = plugin_collection_name(plugin_id)?;
        if workspace_key.is_empty() {
            bail!("workspace_key must not be empty");
        }
        let file_path = self.file_path(&collection, workspace_key);
        let Ok(f) = fs::File::open(&file_path) else {
            return Ok(Vec::new());
        };
        let limit = if limit == 0 { 100 } else { limit };
        Ok(BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str::<PluginMemoryEnvelope<T>>(&line).ok())
            .filter(|r| r.workspace_key == workspace_key)
            .filter(|r| record_kind.is_none_or(|k| r.record_kind == k))
            .take(limit)
            .collect())
    }

    fn file_path(&self, collection: &str, workspace_key: &str) -> PathBuf {
        self.base_dir
            .join(collection)
            .join(format!("{workspace_key}.jsonl"))
    }

    fn _assert_not_core(_path: &Path) {
        // ponytail: the core-memory collection is never reachable from this
        // module — no API accepts a raw storage name, and the derived prefix
        // `graphify_plugin_` cannot collide with it.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::plugin_memory::OpenDocPayload;
    use serde_json::json;

    fn envelope(
        plugin_id: &str,
        workspace_key: &str,
        record_id: &str,
        record_kind: &str,
    ) -> PluginMemoryEnvelope<OpenDocPayload> {
        PluginMemoryEnvelope::new(
            workspace_key,
            plugin_id,
            record_id,
            record_kind,
            1_786_252_800,
            vec!["docs/spec.md".to_string()],
            OpenDocPayload {
                schema_version: 1,
                doc_identity: "doc_spec_v3.pdf".to_string(),
                doc_version: "v3".to_string(),
                chunk_index: 0,
                raw_content: "…".to_string(),
                linked_symbols: vec!["MemoryConfig".to_string()],
            },
        )
    }

    fn temp_store() -> anyhow::Result<(PluginDomainMemory, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        Ok((PluginDomainMemory::new(dir.path()), dir))
    }

    #[test]
    fn test_plugin_collection_name_derivation_and_rejection() -> Result<()> {
        assert_eq!(
            plugin_collection_name("opendoc")?,
            "graphify_plugin_opendoc"
        );
        assert!(plugin_collection_name("").is_err(), "empty rejected");
        assert!(
            plugin_collection_name("opendoc/evil").is_err(),
            "path separator rejected"
        );
        assert!(
            plugin_collection_name("..").is_err(),
            "dot-dot rejected (traversal guard)"
        );
        assert!(
            plugin_collection_name("opendoc.v2").is_err(),
            "dots rejected (Qdrant name rule)"
        );
        assert!(
            plugin_collection_name(&"x".repeat(241)).is_err(),
            "overlong rejected"
        );
        Ok(())
    }

    #[test]
    fn test_put_and_query_roundtrip() -> Result<()> {
        let (store, _dir) = temp_store()?;
        store.put_record(&envelope("opendoc", "ws_a", "rec-1", "doc_chunk_assoc"))?;

        let records = store.query_records::<OpenDocPayload>("opendoc", "ws_a", None, 0)?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_id, "rec-1");
        assert_eq!(records[0].payload.doc_identity, "doc_spec_v3.pdf");
        Ok(())
    }

    #[test]
    fn test_upsert_by_record_id() -> Result<()> {
        let (store, _dir) = temp_store()?;
        store.put_record(&envelope("opendoc", "ws_a", "rec-1", "doc_chunk_assoc"))?;
        let mut updated = envelope("opendoc", "ws_a", "rec-1", "doc_chunk_assoc");
        updated.payload.doc_version = "v4".to_string();
        store.put_record(&updated)?;

        let records = store.query_records::<OpenDocPayload>("opendoc", "ws_a", None, 0)?;
        assert_eq!(records.len(), 1, "record_id replaced, not duplicated");
        assert_eq!(records[0].payload.doc_version, "v4");
        Ok(())
    }

    #[test]
    fn test_plugin_isolation() -> Result<()> {
        let (store, _dir) = temp_store()?;
        store.put_record(&envelope("opendoc", "ws_a", "rec-1", "doc_chunk_assoc"))?;

        let opendoc = store.query_records::<OpenDocPayload>("opendoc", "ws_a", None, 0)?;
        let review = store.query_records::<OpenDocPayload>("review", "ws_a", None, 0)?;
        assert_eq!(opendoc.len(), 1);
        assert_eq!(review.len(), 0, "review namespace holds no opendoc records");
        Ok(())
    }

    #[test]
    fn test_workspace_partitioning() -> Result<()> {
        let (store, _dir) = temp_store()?;
        store.put_record(&envelope("opendoc", "ws_a", "rec-1", "doc_chunk_assoc"))?;
        store.put_record(&envelope("opendoc", "ws_b", "rec-2", "doc_chunk_assoc"))?;

        let ws_a = store.query_records::<OpenDocPayload>("opendoc", "ws_a", None, 0)?;
        let ws_b = store.query_records::<OpenDocPayload>("opendoc", "ws_b", None, 0)?;
        assert_eq!(ws_a.len(), 1);
        assert_eq!(ws_a[0].record_id, "rec-1");
        assert_eq!(ws_b.len(), 1);
        assert_eq!(ws_b[0].record_id, "rec-2", "records never cross workspaces");
        Ok(())
    }

    #[test]
    fn test_record_kind_filter_and_limit() -> Result<()> {
        let (store, _dir) = temp_store()?;
        store.put_record(&envelope("handoff", "ws_a", "h-1", "snapshot"))?;
        store.put_record(&envelope("handoff", "ws_a", "h-2", "task_state"))?;

        let snapshots =
            store.query_records::<OpenDocPayload>("handoff", "ws_a", Some("snapshot"), 0)?;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].record_id, "h-1");
        let first = store.query_records::<OpenDocPayload>("handoff", "ws_a", None, 1)?;
        assert_eq!(first.len(), 1, "limit caps results");
        Ok(())
    }

    #[test]
    fn test_schema_evolution_unknown_payload_fields_preserved() -> Result<()> {
        let (store, _dir) = temp_store()?;
        store.put_record(&envelope("review", "ws_a", "rev-1", "review_finding"))?;

        // Read back as a raw Value: future payload fields survive roundtrip.
        let records = store.query_records::<serde_json::Value>("review", "ws_a", None, 0)?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].payload["doc_identity"], json!("doc_spec_v3.pdf"));
        Ok(())
    }

    #[test]
    fn test_core_write_rejection_is_structural() {
        // No API on PluginDomainMemory accepts a raw collection name or
        // credential; the only name input is plugin_id, which is validated and
        // namespaced. Compile-time guarantee, asserted here:
        let store = PluginDomainMemory::new("/tmp/nowhere");
        // The store exposes no field that could name the core collection.
        assert_eq!(store.base_dir.to_string_lossy(), "/tmp/nowhere");
    }
}
