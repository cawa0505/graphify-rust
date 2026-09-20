//! box-of-rain projection + rendering for `graphify compose render`.
//!
//! Graphify does not own a layout engine (design.md D4): the unified graph is
//! projected into box-of-rain's schema (children boxes + connections) and the
//! `npx box-of-rain` subprocess renders ASCII/SVG.

use std::io::Write;
use std::process::Command;

use anyhow::{Context, anyhow};
use serde_json::{Value, json};

use crate::compose_merge::UnifiedGraph;

/// Max nodes rendered per workspace box; the rest collapse into a placeholder
/// (readability guard — the full graph remains available via `compose graph`).
const MAX_CHILDREN_PER_WORKSPACE: usize = 30;

/// Projects a unified graph into box-of-rain's input schema.
///
/// Workspace container nodes become top-level boxes; their member nodes become
/// child boxes; composition edges become connections (label = relation type).
#[must_use]
pub fn project(unified: &UnifiedGraph) -> Value {
    let mut children: Vec<Value> = Vec::new();
    let mut connections: Vec<Value> = Vec::new();

    for node in &unified.nodes {
        if node.kind != "workspace" {
            continue;
        }
        let ws_id = &node.id.0;
        let prefix = format!("{ws_id}::");
        let members: Vec<&crate::types::Node> = unified
            .nodes
            .iter()
            .filter(|n| n.id.0.starts_with(&prefix))
            .collect();

        let mut child_boxes: Vec<Value> = members
            .iter()
            .take(MAX_CHILDREN_PER_WORKSPACE)
            .map(|n| json!({ "id": n.id.0, "label": n.label }))
            .collect();
        if members.len() > MAX_CHILDREN_PER_WORKSPACE {
            // ponytail: readability cap — 30 boxes per workspace keeps the
            // ASCII diagram legible; raise only if diagrams stay readable.
            child_boxes.push(json!({
                "id": format!("{prefix}_more"),
                "label": format!("... {} more", members.len() - MAX_CHILDREN_PER_WORKSPACE),
            }));
        }

        children.push(json!({
            "id": ws_id,
            "label": ws_id,
            "children": child_boxes,
        }));
    }

    for edge in &unified.edges {
        // Only composition edges cross boxes; intra-workspace edges stay
        // implicit inside each box (box-of-rain has no intra-box edges).
        let (from_ws, _) = crate::manifest::split_node_reference(&edge.source.0)
            .unwrap_or_else(|| (edge.source.0.clone(), String::new()));
        let (to_ws, _) = crate::manifest::split_node_reference(&edge.target.0)
            .unwrap_or_else(|| (edge.target.0.clone(), String::new()));
        if from_ws == to_ws {
            continue;
        }
        connections.push(json!({
            "from": edge.source.0,
            "to": edge.target.0,
            "label": edge.relation,
        }));
    }

    json!({ "children": children, "connections": connections })
}

/// Renders the projection via `npx box-of-rain` and returns its stdout.
///
/// Spike finding (design.md D4): box-of-rain exits 0 even on malformed input,
/// so failure detection checks stdout for `Error:` lines in addition to the
/// exit status. Output is never fabricated — subprocess stdout passes through.
pub fn render_via_npx(projection: &Value, svg: bool) -> anyhow::Result<String> {
    // box-of-rain 用同步 readFileSync 讀 stdin：pipe 未就緒時 read() 回 EAGAIN
    // 直接炸（npx 墊片啟動快慢不定 → 間歇性失敗）。改寫暫存檔、以檔案當 stdin，
    // 檔案讀取永不 EAGAIN。
    let payload = serde_json::to_vec_pretty(projection).context("投影 JSON 序列化失敗")?;
    let mut tmp = tempfile::NamedTempFile::new().context("建立暫存檔失敗")?;
    tmp.write_all(&payload).context("寫入暫存檔失敗")?;
    tmp.flush().ok();

    let stdin_file = tmp.reopen().context("重開暫存檔失敗")?;
    let mut cmd = Command::new("npx");
    cmd.arg("-y").arg("box-of-rain");
    if svg {
        cmd.arg("--svg");
    }
    cmd.stdin(stdin_file);

    let output = cmd
        .output()
        .map_err(|e| anyhow!("無法啟動 npx（需要 Node.js/npx 環境）: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(anyhow!(
            "box-of-rain 執行失敗 (exit {:?}): {}",
            output.status.code(),
            stderr.trim()
        ));
    }
    if stdout
        .lines()
        .any(|line| line.trim_start().starts_with("Error:"))
    {
        return Err(anyhow!("box-of-rain 回報錯誤: {}", stdout.trim()));
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, FileType, Node, NodeId};

    fn fixture_unified() -> UnifiedGraph {
        UnifiedGraph {
            nodes: vec![
                Node {
                    id: NodeId("ws-a".to_string()),
                    label: "ws-a".to_string(),
                    file_type: FileType::Document,
                    kind: "workspace".to_string(),
                    language: "unknown".to_string(),
                    source_file: String::new(),
                    start_line: 0,
                    end_line: 0,
                    doc_comment: None,
                    description: None,
                    metadata: None,
                },
                Node {
                    id: NodeId("ws-a::src/main.rs".to_string()),
                    label: "main".to_string(),
                    file_type: FileType::Code,
                    kind: "function".to_string(),
                    language: "rust".to_string(),
                    source_file: String::new(),
                    start_line: 0,
                    end_line: 0,
                    doc_comment: None,
                    description: None,
                    metadata: None,
                },
                Node {
                    id: NodeId("ws-b".to_string()),
                    label: "ws-b".to_string(),
                    file_type: FileType::Document,
                    kind: "workspace".to_string(),
                    language: "unknown".to_string(),
                    source_file: String::new(),
                    start_line: 0,
                    end_line: 0,
                    doc_comment: None,
                    description: None,
                    metadata: None,
                },
            ],
            edges: vec![
                // intra-workspace edge: must NOT become a connection
                Edge {
                    source: NodeId("ws-a::src/main.rs".to_string()),
                    target: NodeId("ws-a::src/main.rs".to_string()),
                    relation: "calls".to_string(),
                    source_file: String::new(),
                    confidence: "EXTRACTED".to_string(),
                    source_location: String::new(),
                    description: None,
                },
                // composition edge: becomes a connection with label = relation
                Edge {
                    source: NodeId("ws-a".to_string()),
                    target: NodeId("ws-b".to_string()),
                    relation: "uses".to_string(),
                    source_file: "test.yaml".to_string(),
                    confidence: "INFERRED".to_string(),
                    source_location: String::new(),
                    description: None,
                },
            ],
            workspace_count: 2,
            cross_edges: 1,
        }
    }

    #[test]
    fn test_projection_structure() {
        let projection = project(&fixture_unified());
        let Some(children) = projection.get("children").and_then(Value::as_array) else {
            panic!("children array missing");
        };
        assert_eq!(children.len(), 2);
        let Some(ws_a) = children
            .iter()
            .find(|c| c.get("id").and_then(Value::as_str) == Some("ws-a"))
        else {
            panic!("ws-a box missing");
        };
        let Some(ws_children) = ws_a.get("children").and_then(Value::as_array) else {
            panic!("ws-a children missing");
        };
        assert_eq!(ws_children.len(), 1);
        let Some(connections) = projection.get("connections").and_then(Value::as_array) else {
            panic!("connections array missing");
        };
        assert_eq!(connections.len(), 1);
        let conn = &connections[0];
        assert_eq!(conn.get("from").and_then(Value::as_str), Some("ws-a"));
        assert_eq!(conn.get("to").and_then(Value::as_str), Some("ws-b"));
        assert_eq!(conn.get("label").and_then(Value::as_str), Some("uses"));
    }

    #[test]
    fn test_projection_caps_children() {
        let mut unified = fixture_unified();
        for i in 0..40 {
            unified.nodes.push(Node {
                id: NodeId(format!("ws-a::src/file{i}.rs")),
                label: format!("file{i}"),
                file_type: FileType::Code,
                kind: "function".to_string(),
                language: "rust".to_string(),
                source_file: String::new(),
                start_line: 0,
                end_line: 0,
                doc_comment: None,
                description: None,
                metadata: None,
            });
        }
        let projection = project(&unified);
        let Some(children) = projection.get("children").and_then(Value::as_array) else {
            panic!("children array missing");
        };
        let Some(ws_a) = children
            .iter()
            .find(|c| c.get("id").and_then(Value::as_str) == Some("ws-a"))
        else {
            panic!("ws-a box missing");
        };
        let Some(ws_children) = ws_a.get("children").and_then(Value::as_array) else {
            panic!("ws-a children missing");
        };
        // 30 capped + 1 "... N more" placeholder
        assert_eq!(ws_children.len(), MAX_CHILDREN_PER_WORKSPACE + 1);
    }
}
