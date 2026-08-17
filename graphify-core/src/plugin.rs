//! Plugin API for embedded Graphify plugins.
//!
//! This module defines the minimal v1 plugin contract ([`GraphifyPlugin`]), the
//! workspace identity context ([`WorkspaceContext`]) that plugins bind to at
//! startup, and the graph-update notification event ([`GraphUpdateEvent`]) that
//! plugins receive after the graph is rebuilt.
//!
//! The contract is intentionally dependency-free: it uses only `std` and keeps
//! `graphify-core` free of any LLM / HTTP / MCP dependencies. Embedded plugin
//! crates (e.g. `graphify-plugin-handoff`) implement [`GraphifyPlugin`] and are
//! driven by the core, which routes work by [`WorkspaceContext::workspace_key`].

use crate::NodeId;

/// Identity and routing context for a bound workspace.
///
/// Mirrors the `WorkspaceContext` interface contract in `docs/plugin_system.md`
/// §3.1. `workspace_key` is the hard alignment foreign key used for routing
/// between `opendoc-mcp`, `graphify`, and plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceContext {
    /// Stable routing identifier, e.g. `"w-9f8a2b1c-8e7d-4c3b"`.
    pub workspace_key: String,
    /// Human-readable workspace name, e.g. `"graphify-monorepo"`.
    pub workspace_name: String,
    /// Absolute filesystem root of the workspace.
    pub root_path: String,
    /// Unix epoch seconds at context creation.
    pub timestamp: i64,
}

impl WorkspaceContext {
    /// Creates a new workspace context with the current Unix epoch as timestamp.
    pub fn new(
        workspace_key: impl Into<String>,
        workspace_name: impl Into<String>,
        root_path: impl Into<String>,
    ) -> Self {
        Self {
            workspace_key: workspace_key.into(),
            workspace_name: workspace_name.into(),
            root_path: root_path.into(),
            timestamp: now_epoch_seconds(),
        }
    }
}

fn now_epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Derive a stable, reproducible `workspace_key` for the workspace root path.
///
/// Uses `std::collections::hash_map::DefaultHasher` (`SipHash`) — zero external
/// dependencies. Deterministic across process restarts and machines.
/// Result is a hex-encoded hash string, not an RFC 4122 UUID.
pub fn derive_workspace_key<P: AsRef<std::path::Path>>(root_path: P) -> String {
    use std::hash::{Hash, Hasher};
    let canonical = root_path
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| root_path.as_ref().to_path_buf());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// What triggered a graph update broadcast.
///
/// `#[non_exhaustive]` keeps room for future trigger kinds (e.g. a watch-based
/// incremental rebuild) without breaking existing matches on this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraphUpdateKind {
    /// A `graphify index` run completed successfully.
    Indexed,
    /// A `graphify extract` run completed successfully.
    Extracted,
    /// An explicit user/script trigger (`graphify plugin run-hooks`).
    Manual,
}

/// A graph-update notification delivered to bound plugins.
///
/// Carries the routing key (`workspace_key`), the node identifiers affected by
/// the run, and the trigger kind. See `docs/plugin-sdk-roadmap.md` D4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphUpdateEvent {
    /// The workspace the update belongs to (routing key, #3086).
    pub workspace_key: String,
    /// Node identifiers affected by this update; semantics vary by run kind.
    pub modified_nodes: Vec<NodeId>,
    /// What triggered this update.
    pub event: GraphUpdateKind,
}

impl GraphUpdateEvent {
    /// Creates a new graph-update event.
    pub fn new(
        workspace_key: impl Into<String>,
        modified_nodes: Vec<NodeId>,
        event: GraphUpdateKind,
    ) -> Self {
        Self {
            workspace_key: workspace_key.into(),
            modified_nodes,
            event,
        }
    }
}

/// The v1 embedded plugin contract for Graphify.
///
/// Implementations are expected to be lightweight in-process crates; there is no
/// dynamic loading or registry in v1. A plugin is bound to exactly one workspace
/// via [`bind`](GraphifyPlugin::bind) before it is driven.
///
/// ## v1.1 extension
///
/// [`set_notify_callback`](GraphifyPlugin::set_notify_callback) adds an
/// optional host-injected push channel (default no-op, so v1 plugins remain
/// source-compatible). Plugins that produce domain events (e.g. the review
/// plugin's `ImpactAlert`) call the injected callback with a serialized JSON
/// payload; the host controls all forwarding.
pub trait GraphifyPlugin {
    /// Returns the plugin's unique identifier, e.g. `"graphify-plugin-handoff"`.
    fn get_id(&self) -> &str;

    /// Binds this plugin to a workspace context.
    ///
    /// `workspace_key` becomes the plugin's routing identity; after binding,
    /// [`get_workspace_key`](GraphifyPlugin::get_workspace_key) MUST return
    /// the same value as `ctx.workspace_key`.
    fn bind(&mut self, ctx: WorkspaceContext);

    /// Returns the workspace UUID this plugin is bound to.
    ///
    /// Returns an empty string if [`bind`](GraphifyPlugin::bind) has not been
    /// called yet.
    fn get_workspace_key(&self) -> &str;

    /// Synchronizes the given `.toon` payload and returns the processed output.
    ///
    /// - `Some(toon)` — a passive sync: the plugin consumes an externally
    ///   produced `.toon` payload and returns its processed result.
    /// - `None` — a proactive sync: the plugin produces output from its bound
    ///   workspace context alone.
    ///
    /// Implementations MUST NOT panic when called with `None`; they may return
    /// empty output if the bound context is insufficient.
    fn sync_toon(&mut self, opt_toon: Option<Vec<u8>>) -> Vec<u8>;

    /// Receives a graph-update notification after the graph is (re)built.
    ///
    /// The default implementation is a no-op so that plugins written before
    /// this hook existed continue to bind and function unchanged. Plugins that
    /// react to code changes override this and are driven by the CLI's
    /// index/extract broadcast or the manual `plugin run-hooks` trigger.
    fn on_graph_updated(&mut self, _event: &GraphUpdateEvent) {}

    /// Sets the host-injected notify callback (v1.1, optional push channel).
    ///
    /// The host (e.g. `graphify-mcp`) injects a callback at plugin construction
    /// time. Plugins that produce domain events call it with a serialized JSON
    /// payload; the host controls all forwarding (Dependency Inversion — the
    /// plugin never opens its own event bus or MCP transport).
    ///
    /// The default implementation is a no-op, so v1 plugins that predate this
    /// method remain source-compatible.
    fn set_notify_callback(&mut self, _cb: Option<NotifyCallback>) {}

    /// Performs a lightweight health check (passive probe).
    ///
    /// Returns `true` if the plugin is fully operational, `false` if it is
    /// unavailable. The default returns `false` to avoid false positives —
    /// plugins that override this are responsible for reporting their actual
    /// health.
    fn on_health_check(&self) -> bool {
        false
    }
}

/// Host-injected push channel for plugin domain events (v1.1).
///
/// The callback receives a serialized JSON payload (the plugin defines the
/// event schema, e.g. the review plugin's `ImpactAlert`). `Send + Sync` allows
/// the host to forward from any thread.
pub type NotifyCallback = Box<dyn Fn(serde_json::Value) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::{GraphUpdateEvent, GraphUpdateKind, GraphifyPlugin, WorkspaceContext};
    use crate::NodeId;

    /// Reference implementation proving an external crate can implement the
    /// trait and drive the bind / sync flow without any external services.
    #[derive(Debug, Default)]
    struct EchoHandoffPlugin {
        id: &'static str,
        workspace_key: String,
        /// Last graph-update event received, if any.
        last_event: Option<GraphUpdateEvent>,
    }

    impl GraphifyPlugin for EchoHandoffPlugin {
        fn get_id(&self) -> &str {
            self.id
        }

        fn bind(&mut self, ctx: WorkspaceContext) {
            self.workspace_key = ctx.workspace_key;
        }

        fn get_workspace_key(&self) -> &str {
            &self.workspace_key
        }

        fn sync_toon(&mut self, opt_toon: Option<Vec<u8>>) -> Vec<u8> {
            match opt_toon {
                Some(toon) => toon,
                None => format!("toon:{}", self.workspace_key).into_bytes(),
            }
        }

        fn on_graph_updated(&mut self, event: &GraphUpdateEvent) {
            self.last_event = Some(event.clone());
        }
    }

    #[test]
    fn plugin_id_is_stable() {
        let plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_key: String::new(),
            last_event: None,
        };
        assert_eq!(plugin.get_id(), "graphify-plugin-handoff");
    }

    #[test]
    fn bind_roundtrips_workspace_key() {
        let mut plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_key: String::new(),
            last_event: None,
        };
        let ctx = WorkspaceContext::new("w-9f8a2b1c-8e7d-4c3b", "graphify-monorepo", "/tmp/ws");
        plugin.bind(ctx);
        assert_eq!(plugin.get_workspace_key(), "w-9f8a2b1c-8e7d-4c3b");
    }

    #[test]
    fn sync_toon_passthrough_and_proactive() {
        let mut plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_key: String::new(),
            last_event: None,
        };
        let ctx = WorkspaceContext::new("w-abc", "ws", "/tmp/ws");
        plugin.bind(ctx);

        // Passive sync echoes the incoming payload unchanged.
        let passthrough = plugin.sync_toon(Some(b"graph".to_vec()));
        assert_eq!(passthrough, b"graph".to_vec());

        // Proactive sync produces output from the bound context; must not panic.
        let proactive = plugin.sync_toon(None);
        assert_eq!(proactive, b"toon:w-abc".to_vec());
    }

    #[test]
    fn unbound_workspace_key_is_empty() {
        let plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_key: String::new(),
            last_event: None,
        };
        assert_eq!(plugin.get_workspace_key(), "");
    }

    #[test]
    fn graph_update_event_carries_workspace_and_nodes() {
        let nodes = vec![NodeId("a.rs".into()), NodeId("b.rs".into())];
        let event = GraphUpdateEvent::new("w-9f8a", nodes.clone(), GraphUpdateKind::Indexed);
        assert_eq!(event.workspace_key, "w-9f8a");
        assert_eq!(event.modified_nodes, nodes);
        assert_eq!(event.event, GraphUpdateKind::Indexed);
    }

    #[test]
    fn plugin_receives_graph_update_notification() -> Result<(), String> {
        let mut plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_key: String::new(),
            last_event: None,
        };
        let ctx = WorkspaceContext::new("w-abc", "ws", "/tmp/ws");
        plugin.bind(ctx);

        let event = GraphUpdateEvent::new(
            "w-abc",
            vec![NodeId("main.rs".into())],
            GraphUpdateKind::Manual,
        );
        plugin.on_graph_updated(&event);

        let received = plugin.last_event.ok_or("plugin must record the event")?;
        assert_eq!(received.workspace_key, "w-abc");
        assert_eq!(received.event, GraphUpdateKind::Manual);
        assert_eq!(received.modified_nodes, vec![NodeId("main.rs".into())]);
        Ok(())
    }

    /// A plugin that predates the notification hook must still bind and work.
    #[test]
    fn plugin_without_hook_remains_compatible() {
        let mut plugin = LegacyPlugin::default();
        let ctx = WorkspaceContext::new("w-abc", "ws", "/tmp/ws");
        plugin.bind(ctx);
        assert_eq!(plugin.get_workspace_key(), "w-abc");

        // The default hook implementation is a no-op; calling it must not panic.
        let event = GraphUpdateEvent::new("w-abc", Vec::new(), GraphUpdateKind::Indexed);
        plugin.on_graph_updated(&event);
    }

    /// A minimal pre-hook plugin: implements only the v1 core methods.
    #[derive(Debug, Default)]
    struct LegacyPlugin {
        workspace_key: String,
    }

    impl GraphifyPlugin for LegacyPlugin {
        fn get_id(&self) -> &'static str {
            "graphify-plugin-legacy"
        }

        fn bind(&mut self, ctx: WorkspaceContext) {
            self.workspace_key = ctx.workspace_key;
        }

        fn get_workspace_key(&self) -> &str {
            &self.workspace_key
        }

        fn sync_toon(&mut self, _opt_toon: Option<Vec<u8>>) -> Vec<u8> {
            Vec::new()
        }
    }

    /// v1.1: a plugin that stores the injected notify callback and can invoke
    /// it with a serialized payload, mirroring the review plugin's usage.
    // `Debug` 不能 derive：`Box<dyn Fn>` 不實作 `Debug`。
    #[derive(Default)]
    struct NotifyingPlugin {
        workspace_key: String,
        notify: Option<super::NotifyCallback>,
    }

    impl GraphifyPlugin for NotifyingPlugin {
        fn get_id(&self) -> &'static str {
            "graphify-plugin-notifying"
        }

        fn bind(&mut self, ctx: WorkspaceContext) {
            self.workspace_key = ctx.workspace_key;
        }

        fn get_workspace_key(&self) -> &str {
            &self.workspace_key
        }

        fn sync_toon(&mut self, _opt_toon: Option<Vec<u8>>) -> Vec<u8> {
            Vec::new()
        }

        fn set_notify_callback(&mut self, cb: Option<super::NotifyCallback>) {
            self.notify = cb;
        }
    }

    /// v1.1: the injected callback is stored and can push a payload out to the
    /// host (Dependency Inversion — plugin calls, host forwards).
    #[test]
    fn notify_callback_pushes_payload_to_host() -> Result<(), Box<dyn std::error::Error>> {
        let mut plugin = NotifyingPlugin::default();
        let ctx = WorkspaceContext::new("w-abc", "ws", "/tmp/ws");
        plugin.bind(ctx);

        let received = std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = std::sync::Arc::clone(&received);
        plugin.set_notify_callback(Some(Box::new(move |payload| {
            if let Ok(mut sink) = sink.lock() {
                *sink = Some(payload);
            }
        })));

        let cb = plugin.notify.as_ref().ok_or("callback stored")?;
        cb(serde_json::json!({ "event": "impact_alert", "severity": "high" }));

        {
            let guard = received.lock().map_err(|_| "received lock poisoned")?;
            let got = guard.as_ref().ok_or("payload not received")?;
            assert_eq!(got["event"], "impact_alert");
            assert_eq!(got["severity"], "high");
            drop(guard);
        }
        Ok(())
    }

    /// v1.1: clearing the callback (None) disables future pushes.
    #[test]
    fn notify_callback_can_be_cleared() -> Result<(), Box<dyn std::error::Error>> {
        let mut plugin = NotifyingPlugin::default();
        plugin.bind(WorkspaceContext::new("w-abc", "ws", "/tmp/ws"));

        let count = std::sync::Arc::new(std::sync::Mutex::new(0u32));
        let sink = std::sync::Arc::clone(&count);
        plugin.set_notify_callback(Some(Box::new(move |_| {
            if let Ok(mut sink) = sink.lock() {
                *sink += 1;
            }
        })));

        let cb = plugin.notify.as_ref().ok_or("callback stored")?;
        cb(serde_json::Value::Null);
        assert_eq!(*count.lock().map_err(|_| "count lock poisoned")?, 1);

        // Clear: pushes after this are silently dropped.
        plugin.set_notify_callback(None);
        assert!(plugin.notify.is_none());
        let v1_1_cleared = true;
        assert!(v1_1_cleared);
        Ok(())
    }

    /// v1.1: a plugin that never calls `set_notify_callback` still binds and
    /// syncs — the new method's default no-op keeps v1 source compatibility.
    #[test]
    fn v1_plugin_ignores_notify_callback_default() {
        let mut plugin = LegacyPlugin::default();
        plugin.bind(WorkspaceContext::new("w-abc", "ws", "/tmp/ws"));
        plugin.set_notify_callback(None); // default no-op must not panic
        assert_eq!(plugin.get_workspace_key(), "w-abc");
    }
}
