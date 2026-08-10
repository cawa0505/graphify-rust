//! Local→Server one-way delta rehydration (RFC-0004 §1.3.1).
//!
//! The local JSONL plugin memory acts as an offline WAL: plugin-domain
//! envelopes written while the external Qdrant server was unreachable are
//! pushed to the server idempotently once it recovers. `RehydrateJob`
//! implements the P2 [`SyncJob`] trait — it scans pending envelopes
//! (`created_at > last_synced_at`), converts them to payload-only Qdrant
//! points (no vectors; structured records), and batch-upserts them to the
//! server's `graphify_plugin_<id>` collection. Checkpoint advancement is
//! owned by the caller via `check_and_resync` → `mark_synced`; a failed run
//! leaves every pending record and the checkpoint untouched.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use anyhow::{Context, Result};
use graphify_core::plugin_memory::PluginMemoryEnvelope;
use graphify_memory::plugin_memory::PluginDomainMemory;
use graphify_registry::db::{RegistryDb, RegistryError};
use graphify_registry::resync::SyncJob;
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    CreateCollection, PointId, PointStruct, UpsertPoints, Value as QdrantValue, VectorsConfig,
};
use tokio::runtime::Runtime;

/// One-way delta rehydration: local JSONL WAL → external Qdrant server.
pub struct RehydrateJob {
    local_jsonl_store: Arc<PluginDomainMemory>,
    /// Owned runtime: qdrant-client's gRPC calls are async but [`SyncJob`]
    /// is sync, so the job carries its own runtime for `block_on`.
    rt: Runtime,
    server_client: Qdrant,
}

impl RehydrateJob {
    /// Builds a job against an external Qdrant server URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the tokio runtime or Qdrant client cannot be
    /// constructed.
    pub fn new(local_jsonl_store: Arc<PluginDomainMemory>, server_url: &str) -> Result<Self> {
        let rt = Runtime::new().context("building rehydration runtime")?;
        let server_client = Qdrant::from_url(server_url)
            .skip_compatibility_check() // Disable version compatibility check
            .build()
            .context("building qdrant client")?;
        Ok(Self {
            local_jsonl_store,
            rt,
            server_client,
        })
    }

    /// Deterministic point id for a record: 64-bit `SipHash` of
    /// `workspace_key` + `record_id`. The workspace key participates so the
    /// same `record_id` from different workspaces never collides inside the
    /// shared `graphify_plugin_<id>` collection.
    fn record_point_id(workspace_key: &str, record_id: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        workspace_key.hash(&mut hasher);
        record_id.hash(&mut hasher);
        hasher.finish()
    }

    /// Converts an envelope to a payload-only point: the full envelope JSON
    /// is the payload, no vectors are attached.
    fn envelope_to_point(env: &PluginMemoryEnvelope<serde_json::Value>) -> Result<PointStruct> {
        let payload_json = serde_json::to_value(env)?;
        let payload = payload_json
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("envelope must serialize to an object"))?;
        let payload: HashMap<String, QdrantValue> = payload
            .iter()
            .map(|(k, v)| (k.clone(), QdrantValue::from(v.clone())))
            .collect();
        Ok(PointStruct {
            id: Some(PointId::from(Self::record_point_id(
                &env.workspace_key,
                &env.record_id,
            ))),
            payload,
            vectors: None, // payload-only point
        })
    }

    /// Ensures the server-side payload-only collection exists.
    async fn ensure_collection(&self, collection: &str) -> Result<()> {
        if !self.server_client.collection_exists(collection).await? {
            self.server_client
                .create_collection(CreateCollection {
                    collection_name: collection.to_string(),
                    vectors_config: Some(VectorsConfig {
                        config: None, // payload-only collection
                    }),
                    ..Default::default()
                })
                .await?;
        }
        Ok(())
    }

    /// Batch-upserts pending envelopes into the server collection.
    async fn push_points(
        &self,
        collection: &str,
        pending: &[PluginMemoryEnvelope<serde_json::Value>],
    ) -> Result<()> {
        self.ensure_collection(collection).await?;
        let points: Vec<PointStruct> = pending
            .iter()
            .map(Self::envelope_to_point)
            .collect::<Result<_>>()?;
        self.server_client
            .upsert_points_chunked(
                UpsertPoints {
                    collection_name: collection.to_string(),
                    wait: Some(true),
                    points,
                    ..Default::default()
                },
                64,
            )
            .await?;
        Ok(())
    }
}

impl SyncJob for RehydrateJob {
    fn run(
        &self,
        db: &RegistryDb,
        plugin_id: &str,
        workspace_key: &str,
    ) -> Result<(), RegistryError> {
        let reg = db
            .get_registration(plugin_id, workspace_key)?
            .ok_or_else(|| RegistryError::Schema(format!("no registration for {plugin_id}")))?;
        let pending = self
            .local_jsonl_store
            .pending_records_since::<serde_json::Value>(
                plugin_id,
                workspace_key,
                reg.last_synced_at,
            )
            .map_err(|e| RegistryError::Schema(format!("reading pending deltas: {e}")))?;
        if pending.is_empty() {
            return Ok(());
        }
        self.rt
            .block_on(self.push_points(&reg.qdrant_collection_name, &pending))
            .map_err(|e| RegistryError::Schema(format!("rehydration push failed: {e:#}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::plugin_memory::PluginMemoryEnvelope;
    use graphify_registry::db::RegistryDb;

    fn envelope(
        workspace_key: &str,
        record_id: &str,
        created_at: i64,
    ) -> PluginMemoryEnvelope<serde_json::Value> {
        PluginMemoryEnvelope::new(
            workspace_key,
            "handoff",
            record_id,
            "handoff",
            created_at,
            vec![],
            serde_json::json!({"note": "x"}),
        )
    }

    #[test]
    fn record_point_id_is_deterministic_and_workspace_scoped() {
        let a1 = RehydrateJob::record_point_id("ws-a", "rec-1");
        let a2 = RehydrateJob::record_point_id("ws-a", "rec-1");
        assert_eq!(a1, a2);
        let b = RehydrateJob::record_point_id("ws-b", "rec-1");
        assert_ne!(
            a1, b,
            "same record_id in different workspaces must not collide"
        );
        let a2b = RehydrateJob::record_point_id("ws-a", "rec-2");
        assert_ne!(a1, a2b);
    }

    #[test]
    fn envelope_to_point_carries_full_payload_without_vectors() -> Result<()> {
        let env = envelope("ws-a", "rec-1", 1_800_000_000);
        let point = RehydrateJob::envelope_to_point(&env)?;
        assert!(point.vectors.is_none());
        assert_eq!(
            point.payload.get("record_id"),
            Some(&QdrantValue::from(serde_json::json!("rec-1")))
        );
        assert_eq!(
            point.payload.get("workspace_key"),
            Some(&QdrantValue::from(serde_json::json!("ws-a")))
        );
        assert_eq!(
            point.id,
            Some(PointId::from(RehydrateJob::record_point_id(
                "ws-a", "rec-1"
            )))
        );
        Ok(())
    }

    #[test]
    fn run_without_pending_records_is_a_noop() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = RegistryDb::open(&dir.path().join("registry.db"))?;
        db.upsert_workspace("ws-a", "/tmp/ws-a")?;
        db.upsert_plugin_registration("handoff", "ws-a", "graphify_plugin_handoff")?;

        let store = Arc::new(PluginDomainMemory::new(dir.path().join("mem")));
        // No records at all → run returns Ok without touching the server.
        let job = RehydrateJob::new(store, "http://127.0.0.1:1")?;
        job.run(&db, "handoff", "ws-a")?;
        Ok(())
    }

    #[test]
    fn failed_run_preserves_pending_records_and_checkpoint() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = RegistryDb::open(&dir.path().join("registry.db"))?;
        db.upsert_workspace("ws-a", "/tmp/ws-a")?;
        db.upsert_plugin_registration("handoff", "ws-a", "graphify_plugin_handoff")?;

        let store = Arc::new(PluginDomainMemory::new(dir.path().join("mem")));
        // One pending envelope (created_at > last_synced_at = 0).
        let env = envelope("ws-a", "rec-1", 1_800_000_000);
        store.put_record(&env)?;

        // Server is unreachable → the push fails, the registry must stay
        // untouched: status Unavailable, last_synced_at still 0.
        let job = RehydrateJob::new(store.clone(), "http://127.0.0.1:1")?;
        assert!(job.run(&db, "handoff", "ws-a").is_err());

        let reg = db
            .get_registration("handoff", "ws-a")?
            .ok_or_else(|| anyhow::anyhow!("registration missing"))?;
        assert_eq!(reg.last_synced_at, 0);
        assert_eq!(reg.status, graphify_registry::db::PluginStatus::Unavailable);

        // Pending record is still readable (nothing was drained locally).
        let pending = store.pending_records_since::<serde_json::Value>("handoff", "ws-a", 0)?;
        assert_eq!(pending.len(), 1);
        Ok(())
    }
}
