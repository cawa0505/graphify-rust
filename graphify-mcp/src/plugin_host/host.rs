//! `PluginHost`: owns all plugin subprocesses and aggregates their tools.
//!
//! graphify-mcp is the Mode 1 gateway: plugin tools are namespaced as
//! `graphify_plugin_<id>_<tool>` so multiple plugins can never collide, and
//! `tools/call` routes the prefixed name back to the owning subprocess.

use super::process::{PluginProcess, PluginState};
use graphify_llm::config::{PluginConfig, PluginsConfig};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Timeout for the initialize handshake and each tool call.
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);

/// The namespace prefix for every plugin tool exposed by the gateway.
pub const TOOL_PREFIX: &str = "graphify_plugin_";

/// Prefixed tool name: `graphify_plugin_<id>_<tool>`.
pub fn prefixed_tool_name(plugin_id: &str, tool: &str) -> String {
    format!("{TOOL_PREFIX}{plugin_id}_{tool}")
}

/// Splits a prefixed tool name back into `(plugin_id, tool)`, if it matches.
fn unprefix(tool_name: &str) -> Option<(String, String)> {
    let rest = tool_name.strip_prefix(TOOL_PREFIX)?;
    let (plugin_id, tool) = rest.split_once('_')?;
    Some((plugin_id.to_string(), tool.to_string()))
}

/// Manages the plugin subprocesses of one gateway run.
pub struct PluginHost {
    processes: HashMap<String, PluginProcess>,
}

impl PluginHost {
    /// Starts all declared plugins from `[plugins.<id>]` config.
    ///
    /// A plugin that fails to spawn or handshake is recorded as `Failed` and
    /// simply contributes no tools — it never aborts the gateway (design D3:
    /// per-plugin failure isolation).
    pub fn scan(config: &PluginsConfig) -> Self {
        let mut host = Self {
            processes: HashMap::new(),
        };
        for (id, plugin_config) in &config.plugins {
            host.spawn_plugin(id, plugin_config);
        }
        host
    }

    fn spawn_plugin(&mut self, id: &str, config: &PluginConfig) {
        match PluginProcess::spawn(id, config) {
            Ok(mut proc) => match proc.initialize(PLUGIN_TIMEOUT) {
                Ok(()) => {
                    self.processes.insert(id.to_string(), proc);
                }
                Err(e) => {
                    // Process already moved to Failed state; still record it so
                    // state is introspectable.
                    eprintln!("[plugin:{id}] handshake failed: {e}");
                    self.processes.insert(id.to_string(), proc);
                }
            },
            Err(e) => {
                eprintln!("[plugin:{id}] spawn failed: {e}");
            }
        }
    }

    /// All tools from ready plugins, prefixed and JSON-serializable.
    pub fn list_tools(&self) -> Vec<Value> {
        let mut tools = Vec::new();
        for (id, proc) in &self.processes {
            let PluginState::Ready {
                tools: plugin_tools,
            } = proc.state()
            else {
                continue;
            };
            for tool in plugin_tools {
                let mut entry = tool.clone();
                if let Some(name) = entry.get("name").and_then(Value::as_str) {
                    entry["name"] = Value::String(prefixed_tool_name(id, name));
                }
                tools.push(entry);
            }
        }
        tools
    }

    /// Routes a prefixed tool call to its owning plugin.
    pub fn call_tool(&mut self, tool_name: &str, arguments: &Value) -> anyhow::Result<Value> {
        let (plugin_id, tool) = unprefix(tool_name).ok_or_else(|| {
            anyhow::anyhow!("tool '{tool_name}' is not a namespaced plugin tool (expected '{TOOL_PREFIX}<id>_<tool>')")
        })?;
        let proc = self
            .processes
            .get_mut(&plugin_id)
            .ok_or_else(|| anyhow::anyhow!("unknown plugin '{plugin_id}'"))?;
        proc.call_tool(&tool, arguments, PLUGIN_TIMEOUT)
    }

    /// Sends a `notifications/graph_updated` notification to every ready
    /// plugin. Failures are logged and isolated per plugin (design D3).
    pub fn broadcast_graph_updated(&mut self, payload: &Value) {
        for (id, proc) in &mut self.processes {
            if !proc.state().is_ready() {
                continue;
            }
            if let Err(e) = proc.send_notification("notifications/graph_updated", payload.clone()) {
                eprintln!("[plugin:{id}] graph_updated delivery failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A mock plugin config: one `hello` tool that replies to tools/call.
    fn mock_config(respond_to_call: bool) -> PluginConfig {
        // Reads two requests (initialize, tools/list), then replies to each
        // tools/call it sees. `respond_to_call=false` replies with an error.
        let script = if respond_to_call {
            r#"
read -r l
r1='{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"mock","version":"1.0.0"}}}'
l1=$(printf '%s' "$r1" | wc -c)
printf 'Content-Length: %s\r\n\r\n%s' "$l1" "$r1"
read -r l
r2='{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hello","description":"mock tool","inputSchema":{"type":"object"}}]}}'
l2=$(printf '%s' "$r2" | wc -c)
printf 'Content-Length: %s\r\n\r\n%s' "$l2" "$r2"
while read -r l; do
  r3='{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"hi from mock"}]}}'
  l3=$(printf '%s' "$r3" | wc -c)
  printf 'Content-Length: %s\r\n\r\n%s' "$l3" "$r3"
done
"#
        } else {
            r#"
read -r l
r1='{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"mock","version":"1.0.0"}}}'
l1=$(printf '%s' "$r1" | wc -c)
printf 'Content-Length: %s\r\n\r\n%s' "$l1" "$r1"
read -r l
r2='{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hello","description":"mock tool","inputSchema":{"type":"object"}}]}}'
l2=$(printf '%s' "$r2" | wc -c)
printf 'Content-Length: %s\r\n\r\n%s' "$l2" "$r2"
read -r l
r3='{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"mock failure"}}'
l3=$(printf '%s' "$r3" | wc -c)
printf 'Content-Length: %s\r\n\r\n%s' "$l3" "$r3"
"#
        };
        PluginConfig {
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), script.trim().to_string()],
            env: std::collections::HashMap::default(),
            cwd: None,
        }
    }

    fn plugins_config(entries: HashMap<String, PluginConfig>) -> PluginsConfig {
        let mut cfg = PluginsConfig::default();
        cfg.plugins.extend(entries);
        cfg
    }

    #[test]
    fn test_scan_and_list_tools_namespaced() {
        let mut plugins = HashMap::new();
        plugins.insert("mock".to_string(), mock_config(true));
        let host = PluginHost::scan(&plugins_config(plugins));
        let tools = host.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "graphify_plugin_mock_hello");
    }

    #[test]
    fn test_call_tool_routes_and_returns() -> anyhow::Result<()> {
        let mut plugins = HashMap::new();
        plugins.insert("mock".to_string(), mock_config(true));
        let mut host = PluginHost::scan(&plugins_config(plugins));
        let result = host.call_tool("graphify_plugin_mock_hello", &json!({}))?;
        assert_eq!(result["content"][0]["text"], "hi from mock");
        Ok(())
    }

    #[test]
    fn test_call_tool_error_propagates() {
        let mut plugins = HashMap::new();
        plugins.insert("mock".to_string(), mock_config(false));
        let mut host = PluginHost::scan(&plugins_config(plugins));
        let err = host.call_tool("graphify_plugin_mock_hello", &json!({}));
        assert!(err.is_err(), "plugin error must propagate");
    }

    #[test]
    fn test_unprefixed_name_is_rejected() {
        let mut plugins = HashMap::new();
        plugins.insert("mock".to_string(), mock_config(true));
        let mut host = PluginHost::scan(&plugins_config(plugins));
        assert!(host.call_tool("graphify_query", &json!({})).is_err());
    }

    #[test]
    fn test_failed_plugin_contributes_no_tools() {
        let cfg = PluginConfig {
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exit 1".to_string()],
            env: std::collections::HashMap::default(),
            cwd: None,
        };
        let mut plugins = HashMap::new();
        plugins.insert("dead".to_string(), cfg);
        let host = PluginHost::scan(&plugins_config(plugins));
        assert!(
            host.list_tools().is_empty(),
            "dead plugin must add no tools"
        );
    }

    #[test]
    fn test_broadcast_to_ready_plugin_is_isolated() {
        // The mock replies to the notification with an id-3 response nobody is
        // waiting for; broadcast must not panic and must not mark the plugin
        // failed (notification delivery is fire-and-forget).
        let mut plugins = HashMap::new();
        plugins.insert("mock".to_string(), mock_config(true));
        let mut host = PluginHost::scan(&plugins_config(plugins));
        host.broadcast_graph_updated(&json!({"workspace_key": "test"}));
        assert_eq!(
            host.list_tools().len(),
            1,
            "plugin stays ready after broadcast"
        );
    }
}
