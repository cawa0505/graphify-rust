//! Circuit breaker for plugin subprocess failure isolation (plugin-circuit-breaker).
//!
//! Tracks consecutive failures per plugin_id. On the 3rd consecutive failure,
//! transitions the plugin to in-memory bypass set. Successful invocations reset
//! the counter. Quarantine persistence to the SQLite Global Registry happens at
//! startup (seed) and via the CLI probe/reset commands — not at call_tool time.

use graphify_registry::{PluginStatus, RegistryDb};
use std::collections::{HashMap, HashSet};

/// Number of consecutive failures before a plugin is auto-quarantined.
pub const FAILURE_THRESHOLD: u32 = 3;

/// In-process circuit breaker tracking consecutive failures per plugin_id.
pub struct CircuitBreaker {
    failures: HashMap<String, u32>,
    quarantined: HashSet<String>,
    workspace_key: String,
}

impl CircuitBreaker {
    pub fn new(workspace_key: &str) -> Self {
        Self {
            failures: HashMap::new(),
            quarantined: HashSet::new(),
            workspace_key: workspace_key.to_string(),
        }
    }

    /// Seeds the bypass set from the registry on startup.
    pub fn seed_quarantined(&mut self, db: &RegistryDb, plugin_ids: &[String]) {
        for id in plugin_ids {
            if let Ok(Some(row)) = db.get_registration(id, &self.workspace_key) {
                if row.status == PluginStatus::Quarantined {
                    self.quarantined.insert(id.clone());
                }
            }
        }
    }

    /// Returns true if the plugin is currently bypassed (quarantined).
    pub fn is_bypassed(&self, plugin_id: &str) -> bool {
        self.quarantined.contains(plugin_id)
    }

    /// Records a failure. On the 3rd consecutive failure, enters the
    /// in-memory bypass set. Does NOT persist to the registry — that
    /// happens via probe/reset CLI commands.
    pub fn record_failure(&mut self, plugin_id: &str) {
        let count = self.failures.entry(plugin_id.to_string()).or_insert(0);
        *count += 1;
        if *count >= FAILURE_THRESHOLD {
            self.quarantined.insert(plugin_id.to_string());
            self.failures.remove(plugin_id);
        }
    }

    /// Resets the failure counter on a successful invocation.
    pub fn record_success(&mut self, plugin_id: &str) {
        self.failures.remove(plugin_id);
    }

    /// Clears quarantine for a plugin (manual reset path).
    pub fn clear_quarantine(&mut self, plugin_id: &str) {
        self.quarantined.remove(plugin_id);
        self.failures.remove(plugin_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_registry::RegistryDb;
    use tempfile::TempDir;

    fn seeded_db() -> (RegistryDb, TempDir) {
        let dir = TempDir::new().expect("temp");
        let db_path = dir.path().join("test.db");
        let db = RegistryDb::open(&db_path).expect("open db");
        db.upsert_workspace("test_workspace", "/tmp/test").expect("register ws");
        db.upsert_plugin_registration("plugin_a", "test_workspace", "collection_a")
            .expect("register plugin");
        (db, dir)
    }

    #[test]
    fn record_failure_quarantines_on_third() {
        let (_db, _dir) = seeded_db();
        let mut breaker = CircuitBreaker::new("test_workspace");
        assert!(!breaker.is_bypassed("plugin_a"));

        breaker.record_failure("plugin_a");
        breaker.record_failure("plugin_a");
        assert!(!breaker.is_bypassed("plugin_a"), "2 failures should not quarantine");

        breaker.record_failure("plugin_a");
        assert!(breaker.is_bypassed("plugin_a"), "3rd failure should quarantine");
    }

    #[test]
    fn success_resets_counter() {
        let mut breaker = CircuitBreaker::new("test_workspace");
        breaker.record_failure("plugin_a");
        breaker.record_failure("plugin_a");
        breaker.record_success("plugin_a");
        breaker.record_failure("plugin_a");
        breaker.record_failure("plugin_a");
        assert!(!breaker.is_bypassed("plugin_a"));
    }

    #[test]
    fn clear_quarantine_works() {
        let mut breaker = CircuitBreaker::new("test_workspace");
        breaker.record_failure("plugin_a");
        breaker.record_failure("plugin_a");
        breaker.record_failure("plugin_a");
        assert!(breaker.is_bypassed("plugin_a"));

        breaker.clear_quarantine("plugin_a");
        assert!(!breaker.is_bypassed("plugin_a"));
    }

    #[test]
    fn seed_quarantined_loads_from_registry() {
        let (db, _dir) = seeded_db();
        db.set_status("plugin_a", "test_workspace", PluginStatus::Quarantined).expect("set status");
        let mut breaker = CircuitBreaker::new("test_workspace");
        breaker.seed_quarantined(&db, &["plugin_a".to_string()]);
        assert!(breaker.is_bypassed("plugin_a"));
    }
}