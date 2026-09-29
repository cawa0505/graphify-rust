//! Remote graph source (relay-remote-transport Task 2b, graphify-tui spec).
//!
//! `graphify tui --remote <URL>` / `GRAPHIFY_REMOTE`: fetch the server-side
//! graph snapshot over the same streamable HTTP endpoint as nexus, restore it
//! with `from_toon`, then run the existing local TUI unchanged on the result.
//!
//! No polling, no per-frame network: one snapshot per launch. An unreachable
//! server fails explicitly with the URL in the error — never a silent
//! fallback to the local graph.

use anyhow::{Context, Result};
use graphify_core::{GraphOutput, toon};

/// Blocking HTTP is fine here: `run_tui` runs from the synchronous CLI main
/// before the terminal UI loop starts.
pub struct RemoteClient {
    url: String,
    bearer: Option<String>,
    inner: reqwest::blocking::Client,
}

impl RemoteClient {
    #[must_use]
    pub fn new(url: &str, bearer: Option<String>) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            bearer,
            inner: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Fetch the full graph snapshot via `tools/call graphify_graph_snapshot`
    /// (same streamable-HTTP endpoint nexus uses: one endpoint, one auth
    /// path). Returns the restored [`GraphOutput`].
    pub fn fetch_graph(&self) -> Result<GraphOutput> {
        let mut req = self
            .inner
            .post(format!("{}/mcp", self.url))
            .header("content-type", "application/json");
        if let Some(token) = &self.bearer {
            req = req.bearer_auth(token);
        }
        let resp = req
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": "graphify_graph_snapshot", "arguments": {} }
            }))
            .send()
            .with_context(|| format!("cannot reach graphify server {}/mcp", self.url))?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().context("server returned non-JSON")?;
        // JSON-RPC error surface (e.g. missing/invalid Bearer) must be locatable.
        if let Some(err) = body.get("error") {
            anyhow::bail!("server {}/mcp rejected request: {err}", self.url);
        }
        let toon_text = body
            .pointer("/result/content/0/text")
            .and_then(serde_json::Value::as_str)
            .with_context(|| {
                let url = self.url.clone();
                format!("server {url} returned no snapshot content (HTTP {status})")
            })?;
        toon::from_toon(toon_text).context("remote snapshot is not a valid .toon graph")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // tests assert on wire payloads and error text

    use super::*;

    use std::io::{Read, Write as IoWrite};

    /// One-shot canned JSON-RPC HTTP responder (std only, no new deps).
    fn spawn_canned(body: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    /// Remote 200 + content-blocks TOON → `fetch_graph` restores the graph.
    #[test]
    fn test_remote_fetch_parses_content_blocks_snapshot() {
        let graph = GraphOutput {
            nodes: vec![graphify_core::Node {
                id: graphify_core::NodeId("src/main.rs".to_string()),
                label: "src/main.rs".to_string(),
                file_type: graphify_core::FileType::Code,
                kind: "module".to_string(),
                language: "rust".to_string(),
                source_file: "src/main.rs".to_string(),
                start_line: 1,
                end_line: 2,
                doc_comment: None,
                description: None,
                metadata: None,
            }],
            edges: vec![],
            metadata: graphify_core::GraphMetadata::default(),
        };
        let toon = toon::to_toon(&graph);
        let payload = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "content": [{ "type": "text", "text": toon }] }
        });
        let body = Box::leak(serde_json::to_string(&payload).unwrap().into_boxed_str());
        let url = spawn_canned(body);

        let fetched = RemoteClient::new(&url, None).fetch_graph().unwrap();
        assert_eq!(fetched.nodes.len(), 1);
        assert_eq!(fetched.nodes[0].id.0, "src/main.rs");
    }

    /// JSON-RPC error (e.g. Bearer rejected) surfaces as a locatable failure.
    #[test]
    fn test_remote_jsonrpc_error_surfaces_url() {
        let payload = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "error": { "code": -32000, "message": "unauthorized" }
        });
        let body = Box::leak(serde_json::to_string(&payload).unwrap().into_boxed_str());
        let url = spawn_canned(body);

        let err = RemoteClient::new(&url, None)
            .fetch_graph()
            .expect_err("error envelope must fail");
        assert!(err.to_string().contains(&url), "{err}");
        assert!(err.to_string().contains("unauthorized"), "{err}");
    }

    /// Unreachable server fails explicitly, error carries the URL (spec:
    /// 顯示連線失敗與 server URL，不退化 local）。
    #[test]
    fn test_remote_unreachable_fails_with_url() {
        // Port 1 on loopback: nothing listens, connect refused immediately.
        let err = RemoteClient::new("http://127.0.0.1:1", None)
            .fetch_graph()
            .expect_err("closed port must fail");
        assert!(err.to_string().contains("127.0.0.1:1"), "{err}");
    }
}
