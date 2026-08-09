//! `PluginProcess`: spawn and drive a third-party MCP plugin subprocess.
//!
//! The child speaks standard MCP Content-Length framing (see `framing`).
//! graphify-mcp's own client connection is synchronous stdio, so this module
//! stays synchronous: a reader thread pushes framed messages into a channel
//! and callers wait with `recv_timeout` for responses.

use super::framing;
use graphify_llm::config::PluginConfig;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

/// Protocol version sent in the initialize handshake (MCP 2025-11-25).
const PROTOCOL_VERSION: &str = "2025-11-25";

/// Lifecycle state of a plugin subprocess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    /// Process spawned, handshake not yet completed.
    Spawning,
    /// Handshake + tools/list completed; tools are callable.
    Ready { tools: Vec<Value> },
    /// Failed for a concrete reason; its tools are excluded from `tools/list`.
    Failed(String),
}

impl PluginState {
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

/// A running MCP plugin subprocess.
pub struct PluginProcess {
    id: String,
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<std::io::Result<String>>,
    state: PluginState,
    next_request_id: u64,
}

impl PluginProcess {
    /// Spawns the plugin subprocess from a `[plugins.<id>]` declaration.
    ///
    /// stdout is claimed by a reader thread; stderr is drained to this
    /// process's own stderr (design D4: never let it pollute the JSON-RPC
    /// stream). The handshake is performed by [`Self::initialize`].
    pub fn spawn(id: &str, config: &PluginConfig) -> std::io::Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args).envs(&config.env);
        if let Some(cwd) = &config.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "plugin stdin was not piped")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "plugin stdout was not piped",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "plugin stderr was not piped",
            )
        })?;

        // Drain stderr to this process's stderr so a chatty plugin can never
        // block on a full pipe or corrupt the framed stdout stream.
        let id_for_thread = id.to_string();
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut buf = String::new();
            while reader.read_line(&mut buf).is_ok_and(|n| n > 0) {
                eprintln!("[plugin:{id_for_thread}] {}", buf.trim_end());
                buf.clear();
            }
        });

        let (tx, rx) = mpsc::channel();
        let mut reader = BufReader::new(stdout);
        thread::spawn(move || {
            loop {
                match framing::read_message(&mut reader) {
                    Ok(Some(body)) => {
                        if tx.send(Ok(body)).is_err() {
                            break; // consumer dropped
                        }
                    }
                    Ok(None) => break, // clean EOF
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            id: id.to_string(),
            child,
            stdin,
            rx,
            state: PluginState::Spawning,
            next_request_id: 1,
        })
    }

    pub const fn state(&self) -> &PluginState {
        &self.state
    }

    /// Sends `initialize`, expects the result, then sends
    /// `notifications/initialized` and collects `tools/list`.
    ///
    /// On timeout or a framing/JSON error the process is marked `Failed`.
    pub fn initialize(&mut self, timeout: Duration) -> anyhow::Result<()> {
        let init_id = self.next_id();
        self.send_request(
            init_id,
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "graphify-mcp", "version": env!("CARGO_PKG_VERSION") },
            }),
        )?;
        let init_resp = self.await_response(init_id, timeout)?;
        if init_resp.get("error").is_some() {
            return Err(self.fail(format!("initialize returned an error: {init_resp}")));
        }

        // Notify initialized; no response is expected for notifications.
        self.send_notification("notifications/initialized", json!({}))?;

        let tools_id = self.next_id();
        self.send_request(tools_id, "tools/list", json!({}))?;
        let tools_resp = self.await_response(tools_id, timeout)?;
        if let Some(err) = tools_resp.get("error") {
            return Err(self.fail(format!("tools/list returned an error: {err}")));
        }
        let tools = tools_resp
            .get("result")
            .and_then(|r| r.get("tools"))
            .cloned()
            .and_then(|t| t.as_array().cloned())
            .unwrap_or_default();
        self.state = PluginState::Ready { tools };
        Ok(())
    }

    /// Forwards a `tools/call` request and returns the `result` value.
    pub fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: &Value,
        timeout: Duration,
    ) -> anyhow::Result<Value> {
        let PluginState::Ready { .. } = self.state else {
            return Err(anyhow::anyhow!(
                "plugin '{}' is not ready (state: {:?})",
                self.id,
                self.state
            ));
        };
        let call_id = self.next_id();
        self.send_request(
            call_id,
            "tools/call",
            json!({
                "name": tool_name,
                "arguments": arguments,
            }),
        )?;
        let resp = self.await_response(call_id, timeout)?;
        if let Some(err) = resp.get("error") {
            return Err(anyhow::anyhow!(
                "plugin '{}' tool '{tool_name}' error: {err}",
                self.id
            ));
        }
        resp.get("result").cloned().ok_or_else(|| {
            anyhow::anyhow!("plugin '{}' tool '{tool_name}' missing result", self.id)
        })
    }

    /// Sends a notification (no response expected).
    pub fn send_notification(&mut self, method: &str, params: Value) -> anyhow::Result<()> {
        let body = json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string();
        framing::write_message(&mut self.stdin, &body)
            .map_err(|e| anyhow::anyhow!("write to plugin '{}' failed: {e}", self.id))
    }

    fn send_request(&mut self, id: u64, method: &str, params: Value) -> anyhow::Result<()> {
        let body =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        framing::write_message(&mut self.stdin, &body)
            .map_err(|e| anyhow::anyhow!("write to plugin '{}' failed: {e}", self.id))
    }

    const fn next_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }

    /// Waits for the response whose `id` matches, skipping notifications.
    fn await_response(&mut self, want_id: u64, timeout: Duration) -> anyhow::Result<Value> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let body = match self.rx.recv_timeout(remaining) {
                Ok(Ok(body)) => body,
                Ok(Err(e)) => {
                    return Err(self.fail(format!("plugin '{}' framing error: {e}", self.id)));
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(self.fail(format!(
                        "plugin '{}' did not respond to id {want_id} within {:?}",
                        self.id, timeout
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(self.fail(format!("plugin '{}' exited unexpectedly", self.id)));
                }
            };
            let msg: Value = serde_json::from_str(&body)
                .map_err(|e| anyhow::anyhow!("plugin '{}' sent invalid JSON: {e}", self.id))?;
            if msg.get("id").and_then(Value::as_u64) == Some(want_id) {
                return Ok(msg);
            }
            // Notifications (no id) and unrelated responses are skipped.
        }
    }

    fn fail(&mut self, reason: String) -> anyhow::Error {
        self.state = PluginState::Failed(reason.clone());
        anyhow::anyhow!("{reason}")
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        // Best-effort reaping: a plugin that never exits on stdin EOF would
        // otherwise linger as an orphan after the server exits.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a `PluginConfig` pointing at a tiny inline MCP plugin: a
    /// `/bin/sh` script that replies to `initialize` and `tools/list` with
    /// fixed framed responses. POSIX `sh` + `wc` exist on every target.
    fn mock_plugin_config() -> PluginConfig {
        // Responses are ASCII JSON, so byte length == char length for `wc -c`.
        let script = r#"
read -r line1
resp1='{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"mock","version":"1.0.0"}}}'
len1=$(printf '%s' "$resp1" | wc -c)
printf 'Content-Length: %s\r\n\r\n%s' "$len1" "$resp1"
read -r line2
resp2='{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hello","description":"mock tool","inputSchema":{"type":"object"}}]}}'
len2=$(printf '%s' "$resp2" | wc -c)
printf 'Content-Length: %s\r\n\r\n%s' "$len2" "$resp2"
"#;
        PluginConfig {
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), script.trim().to_string()],
            env: std::collections::HashMap::default(),
            cwd: None,
        }
    }

    #[test]
    fn test_initialize_reaches_ready() -> anyhow::Result<()> {
        let mut proc = PluginProcess::spawn("mock", &mock_plugin_config())?;
        assert_eq!(proc.id, "mock");
        proc.initialize(Duration::from_secs(5))?;
        assert!(proc.state().is_ready(), "state = {:?}", proc.state());
        let PluginState::Ready { tools } = proc.state() else {
            unreachable!()
        };
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "hello");
        Ok(())
    }

    #[test]
    fn test_spawn_missing_command_fails() {
        let cfg = PluginConfig {
            command: "/nonexistent/plugin-bin".to_string(),
            args: vec![],
            env: std::collections::HashMap::default(),
            cwd: None,
        };
        assert!(PluginProcess::spawn("missing", &cfg).is_err());
    }
}
