//! Passive-triggered provider resync boundary (no daemon polling).
//!
//! P2 defines the trigger interface (`ProviderProbe` + `SyncJob`) and the
//! status transitions (`Unavailable` → `Ready`); the actual batch upsert job
//! body lands in P4 (Qdrant Local fallback rehydration) per design D4.

use crate::db::{RegistryDb, RegistryError};

/// Embedding-provider health probe, injectable for tests.
///
/// The concrete implementation wraps `QdrantMemoryStore::is_available` with
/// a 10ms timeout boundary at the caller (graphify-cli / graphify-mcp).
pub trait ProviderProbe {
    /// `true` when the embedding provider answers within the ping budget.
    fn is_available(&self) -> bool;
}

/// Rehydration job executed once the provider is back.
///
/// P4 implements the body (query pending records via `SQLite`
/// `created_at > last_synced_at`, batch upsert over the Qdrant REST API);
/// P2 only wires the boundary and the state transition.
pub trait SyncJob {
    /// Run the sync for one registration.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` when the sync fails partway; the caller keeps
    /// `status = 'Unavailable'` and preserves all pending records.
    fn run(
        &self,
        db: &RegistryDb,
        plugin_id: &str,
        workspace_key: &str,
    ) -> Result<(), RegistryError>;
}

/// Outcome of a `check_and_resync` pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResyncOutcome {
    /// Probe failed; the registry stays `Unavailable` and the caller prints
    /// a warning and continues without memory queries.
    ProviderUnavailable,
    /// Probe succeeded and every registration job completed; registrations
    /// were flipped to `Ready`.
    Synced,
}

/// Startup-boundary check: probe once, and if the provider is back, run the
/// sync job for every registration of `workspace_key` and flip them `Ready`.
///
/// A probe failure never blocks the caller (returns `ProviderUnavailable`).
///
/// # Errors
///
/// Returns `RegistryError` on `SQLite` failure; job failures are converted
/// to `ProviderUnavailable` and reported so the caller can warn.
pub fn check_and_resync(
    db: &RegistryDb,
    probe: &dyn ProviderProbe,
    job: &dyn SyncJob,
    workspace_key: &str,
) -> Result<ResyncOutcome, RegistryError> {
    if !probe.is_available() {
        return Ok(ResyncOutcome::ProviderUnavailable);
    }
    let registrations = db.list_registrations(workspace_key)?;
    for reg in registrations {
        match job.run(db, &reg.plugin_id, workspace_key) {
            Ok(()) => db.mark_synced(&reg.plugin_id, workspace_key, crate::db::now_unix())?,
            Err(_) => return Ok(ResyncOutcome::ProviderUnavailable),
        }
    }
    Ok(ResyncOutcome::Synced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{PluginStatus, RegistryDb, RegistryError};
    use graphify_core::HandoffSnapshot;
    use std::cell::RefCell;

    struct FakeProbe {
        available: bool,
    }
    impl ProviderProbe for FakeProbe {
        fn is_available(&self) -> bool {
            self.available
        }
    }

    struct FakeJob {
        fail: bool,
        ran: RefCell<Vec<(String, String)>>,
    }
    impl SyncJob for FakeJob {
        fn run(
            &self,
            _db: &RegistryDb,
            plugin_id: &str,
            workspace_key: &str,
        ) -> Result<(), RegistryError> {
            if self.fail {
                return Err(RegistryError::Schema("job failed".into()));
            }
            self.ran
                .borrow_mut()
                .push((plugin_id.to_string(), workspace_key.to_string()));
            Ok(())
        }
    }

    fn open_temp() -> Result<(RegistryDb, tempfile::TempDir), RegistryError> {
        let dir = tempfile::tempdir().map_err(RegistryError::Io)?;
        let db = RegistryDb::open(&dir.path().join("graphify.db"))?;
        Ok((db, dir))
    }

    fn snapshot(workspace_key: &str, created_at: i64) -> HandoffSnapshot {
        HandoffSnapshot {
            snapshot_id: format!("snap-{created_at}"),
            session_id: "sess-1".into(),
            workspace_key: workspace_key.into(),
            created_at,
            expires_at: 0,
            payload: graphify_core::HandoffPayload {
                schema_version: 1,
                task_goal: "goal".into(),
                pinned_node_ids: Vec::new(),
                focused_subgraph_toon: String::new(),
                reconstructable_query_metadata: graphify_core::MemoryQueryCriteria::default(),
            },
        }
    }

    #[test]
    fn probe_failure_keeps_unavailable_and_continues() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        let probe = FakeProbe { available: false };
        let job = FakeJob {
            fail: false,
            ran: RefCell::new(Vec::new()),
        };
        let outcome = check_and_resync(&db, &probe, &job, "ws-a")?;
        assert_eq!(outcome, ResyncOutcome::ProviderUnavailable);
        assert!(
            job.ran.borrow().is_empty(),
            "job must not run on failed probe"
        );
        let reg = db
            .get_registration("opendoc", "ws-a")?
            .ok_or_else(|| RegistryError::Schema("missing".into()))?;
        assert_eq!(reg.status, PluginStatus::Unavailable);
        Ok(())
    }

    #[test]
    fn probe_success_runs_job_and_flips_ready() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        let probe = FakeProbe { available: true };
        let job = FakeJob {
            fail: false,
            ran: RefCell::new(Vec::new()),
        };
        let outcome = check_and_resync(&db, &probe, &job, "ws-a")?;
        assert_eq!(outcome, ResyncOutcome::Synced);
        assert_eq!(job.ran.borrow().len(), 1);
        let reg = db
            .get_registration("opendoc", "ws-a")?
            .ok_or_else(|| RegistryError::Schema("missing".into()))?;
        assert_eq!(reg.status, PluginStatus::Ready);
        assert!(
            reg.last_synced_at > 0,
            "mark_synced advances the checkpoint"
        );
        Ok(())
    }

    #[test]
    fn job_failure_preserves_unavailable_and_data() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        let future = crate::db::now_unix() + 3600;
        db.put_snapshot(&snapshot("ws-a", future))?;
        let probe = FakeProbe { available: true };
        let job = FakeJob {
            fail: true,
            ran: RefCell::new(Vec::new()),
        };
        let outcome = check_and_resync(&db, &probe, &job, "ws-a")?;
        assert_eq!(outcome, ResyncOutcome::ProviderUnavailable);
        let reg = db
            .get_registration("opendoc", "ws-a")?
            .ok_or_else(|| RegistryError::Schema("missing".into()))?;
        assert_eq!(reg.status, PluginStatus::Unavailable);
        assert_eq!(
            db.get_pending_snapshots_since("ws-a", 0)?.len(),
            1,
            "pending records survive a failed job"
        );
        Ok(())
    }
}
