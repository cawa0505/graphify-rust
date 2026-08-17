//! Plugin host: manages bound plugins and broadcasts graph-update events.
//!
//! Plugins are embedded (compiled into the graphify binary) and registered
//! at startup. When the graph is rebuilt (index/extract) or a user manually
//! triggers hooks, the host broadcasts a `GraphUpdateEvent` to every bound
//! plugin. A single plugin's hook failure (panic) is isolated and does not
//! interrupt delivery to the remaining plugins.
//!
//! Also provides health probe + quarantine reset for the `graphify plugin`
//! CLI subcommands (P3 plugin-health-admission).

use graphify_core::plugin::{GraphUpdateEvent, GraphifyPlugin};
use graphify_registry::db::{PluginStatus, RegistryDb};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Registry of bound plugins for a single graphify process.
pub struct PluginHost {
    plugins: Vec<Box<dyn GraphifyPlugin>>,
}

impl PluginHost {
    /// Create an empty host (no plugins bound).
    #[must_use]
    pub fn new() -> Self {
        Self { plugins: vec![] }
    }

    /// Register a plugin so it receives graph-update events.
    pub fn register(&mut self, plugin: Box<dyn GraphifyPlugin>) {
        self.plugins.push(plugin);
    }

    /// Number of bound plugins.
    #[must_use]
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether no plugins are bound.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Return the IDs of all registered plugins.
    #[must_use]
    pub fn get_ids(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.get_id()).collect()
    }

    /// Broadcast a graph-update event to every bound plugin.
    ///
    /// Each plugin's `on_graph_updated` is wrapped in `catch_unwind` so a
    /// panicking plugin is reported on stderr and skipped without aborting
    /// delivery to the remaining plugins.
    pub fn broadcast(&mut self, event: &GraphUpdateEvent) {
        for plugin in &mut self.plugins {
            let result = catch_unwind(AssertUnwindSafe(|| {
                plugin.on_graph_updated(event);
            }));
            if let Err(payload) = result {
                let msg = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("<non-string panic>");
                eprintln!(
                    "[graphify] plugin '{}' hook panicked: {msg}",
                    plugin.get_id()
                );
            }
        }
    }

    /// Run a passive health probe on every bound plugin, persist statuses
    /// to the registry, and return the results.
    ///
    /// A plugin's `on_health_check` returning `true` → `Healthy`,
    /// returning `false` → `Unavailable`. No timeout; the check is
    /// expected to complete in <10 ms per spec.
    pub fn probe_all(
        &mut self,
        db: &RegistryDb,
        workspace_key: &str,
    ) -> Vec<(String, PluginStatus)> {
        let mut results: Vec<(String, PluginStatus)> = Vec::with_capacity(self.plugins.len());
        for plugin in &self.plugins {
            // ponytail: on_health_check is a sync bool, no timeout needed
            let status = if plugin.on_health_check() {
                PluginStatus::Healthy
            } else {
                PluginStatus::Unavailable
            };
            let id = plugin.get_id().to_string();
            let _ = db.set_status(&id, workspace_key, status);
            results.push((id, status));
        }
        results
    }

    /// Reset a plugin's quarantine: set status to `Healthy`, run probe,
    /// return the post-probe status.
    ///
    /// Returns `None` if no plugin with that id is registered.
    pub fn reset_quarantine(
        &mut self,
        db: &RegistryDb,
        workspace_key: &str,
        plugin_id: &str,
    ) -> Option<PluginStatus> {
        if !self.plugins.iter().any(|p| p.get_id() == plugin_id) {
            return None;
        }
        // Clear quarantine to Healthy before probing.
        let _ = db.set_status(plugin_id, workspace_key, PluginStatus::Healthy);
        // Re-probe this plugin.
        for plugin in &self.plugins {
            if plugin.get_id() != plugin_id {
                continue;
            }
            let status = if plugin.on_health_check() {
                PluginStatus::Healthy
            } else {
                PluginStatus::Unavailable
            };
            let _ = db.set_status(plugin_id, workspace_key, status);
            return Some(status);
        }
        None
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::plugin::{GraphUpdateEvent, GraphUpdateKind, WorkspaceContext};
    use std::sync::{Arc, Mutex};

    /// A test plugin that records received events.
    struct RecordingPlugin {
        id: &'static str,
        received: Arc<Mutex<Vec<GraphUpdateEvent>>>,
    }

    impl GraphifyPlugin for RecordingPlugin {
        fn get_id(&self) -> &'static str {
            self.id
        }
        fn bind(&mut self, _ctx: WorkspaceContext) {}
        fn get_workspace_key(&self) -> &'static str {
            ""
        }
        fn sync_toon(&mut self, _opt_toon: Option<Vec<u8>>) -> Vec<u8> {
            Vec::new()
        }
        fn on_graph_updated(&mut self, event: &GraphUpdateEvent) {
            self.received
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(event.clone());
        }
    }

    /// A plugin that panics on every hook call.
    struct PanickingPlugin {
        id: &'static str,
    }

    impl GraphifyPlugin for PanickingPlugin {
        fn get_id(&self) -> &'static str {
            self.id
        }
        fn bind(&mut self, _ctx: WorkspaceContext) {}
        fn get_workspace_key(&self) -> &'static str {
            ""
        }
        fn sync_toon(&mut self, _opt_toon: Option<Vec<u8>>) -> Vec<u8> {
            Vec::new()
        }
        fn on_graph_updated(&mut self, _event: &GraphUpdateEvent) {
            panic!("intentional test panic");
        }
    }

    fn sample_event() -> GraphUpdateEvent {
        GraphUpdateEvent {
            workspace_key: "test-key".to_string(),
            modified_nodes: vec![],
            event: GraphUpdateKind::Manual,
        }
    }

    #[test]
    fn broadcast_delivers_to_all_plugins() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        host.register(Box::new(RecordingPlugin {
            id: "recorder",
            received: Arc::clone(&received),
        }));
        host.register(Box::new(RecordingPlugin {
            id: "recorder2",
            received: Arc::clone(&received),
        }));

        host.broadcast(&sample_event());

        assert_eq!(
            received
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            2
        );
    }

    #[test]
    fn broadcast_isolates_panic_from_other_plugins() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        host.register(Box::new(PanickingPlugin { id: "panicker" }));
        host.register(Box::new(RecordingPlugin {
            id: "survivor",
            received: Arc::clone(&received),
        }));

        host.broadcast(&sample_event());

        // The panicking plugin is skipped; the survivor still receives the event.
        assert_eq!(
            received
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            1
        );
    }

    #[test]
    fn broadcast_on_empty_host_is_noop() {
        let mut host = PluginHost::new();
        host.broadcast(&sample_event());
        // No panic, no output — just returns.
    }
}
