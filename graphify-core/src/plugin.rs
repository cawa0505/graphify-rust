//! Plugin API for embedded Graphify plugins.
//!
//! This module defines the minimal v1 plugin contract ([`GraphifyPlugin`]) and the
//! workspace identity context ([`WorkspaceContext`]) that plugins bind to at startup.
//!
//! The contract is intentionally dependency-free: it uses only `std` and keeps
//! `graphify-core` free of any LLM / HTTP / MCP dependencies. Embedded plugin
//! crates (e.g. `graphify-plugin-handoff`) implement [`GraphifyPlugin`] and are
//! driven by the core, which routes work by [`WorkspaceContext::workspace_uuid`].

/// Identity and routing context for a bound workspace.
///
/// Mirrors the `WorkspaceContext` interface contract in `docs/plugin_system.md`
/// §3.1. `workspace_uuid` is the hard alignment foreign key used for routing
/// between `opendoc-mcp`, `graphify`, and plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceContext {
    /// Stable routing identifier, e.g. `"w-9f8a2b1c-8e7d-4c3b"`.
    pub workspace_uuid: String,
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
        workspace_uuid: impl Into<String>,
        workspace_name: impl Into<String>,
        root_path: impl Into<String>,
    ) -> Self {
        Self {
            workspace_uuid: workspace_uuid.into(),
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

/// The v1 embedded plugin contract for Graphify.
///
/// Implementations are expected to be lightweight in-process crates; there is no
/// dynamic loading or registry in v1. A plugin is bound to exactly one workspace
/// via [`bind`](GraphifyPlugin::bind) before it is driven.
pub trait GraphifyPlugin {
    /// Returns the plugin's unique identifier, e.g. `"graphify-plugin-handoff"`.
    fn get_id(&self) -> &str;

    /// Binds this plugin to a workspace context.
    ///
    /// `workspace_uuid` becomes the plugin's routing identity; after binding,
    /// [`get_workspace_uuid`](GraphifyPlugin::get_workspace_uuid) MUST return
    /// the same value as `ctx.workspace_uuid`.
    fn bind(&mut self, ctx: WorkspaceContext);

    /// Returns the workspace UUID this plugin is bound to.
    ///
    /// Returns an empty string if [`bind`](GraphifyPlugin::bind) has not been
    /// called yet.
    fn get_workspace_uuid(&self) -> &str;

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
}

#[cfg(test)]
mod tests {
    use super::{GraphifyPlugin, WorkspaceContext};

    /// Reference implementation proving an external crate can implement the
    /// trait and drive the bind / sync flow without any external services.
    #[derive(Debug, Default)]
    struct EchoHandoffPlugin {
        id: &'static str,
        workspace_uuid: String,
    }

    impl GraphifyPlugin for EchoHandoffPlugin {
        fn get_id(&self) -> &str {
            self.id
        }

        fn bind(&mut self, ctx: WorkspaceContext) {
            self.workspace_uuid = ctx.workspace_uuid;
        }

        fn get_workspace_uuid(&self) -> &str {
            &self.workspace_uuid
        }

        fn sync_toon(&mut self, opt_toon: Option<Vec<u8>>) -> Vec<u8> {
            match opt_toon {
                Some(toon) => toon,
                None => format!("toon:{}", self.workspace_uuid).into_bytes(),
            }
        }
    }

    #[test]
    fn plugin_id_is_stable() {
        let plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_uuid: String::new(),
        };
        assert_eq!(plugin.get_id(), "graphify-plugin-handoff");
    }

    #[test]
    fn bind_roundtrips_workspace_uuid() {
        let mut plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_uuid: String::new(),
        };
        let ctx = WorkspaceContext::new("w-9f8a2b1c-8e7d-4c3b", "graphify-monorepo", "/tmp/ws");
        plugin.bind(ctx);
        assert_eq!(plugin.get_workspace_uuid(), "w-9f8a2b1c-8e7d-4c3b");
    }

    #[test]
    fn sync_toon_passthrough_and_proactive() {
        let mut plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_uuid: String::new(),
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
    fn unbound_workspace_uuid_is_empty() {
        let plugin = EchoHandoffPlugin {
            id: "graphify-plugin-handoff",
            workspace_uuid: String::new(),
        };
        assert_eq!(plugin.get_workspace_uuid(), "");
    }
}
