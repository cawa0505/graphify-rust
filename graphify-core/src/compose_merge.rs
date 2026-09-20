//! Merges per-workspace graphs + manifest relations into a unified model.
//!
//! Node ids get a `{ws_id}::` prefix so identical local ids across workspaces
//! cannot collide. Manifest relations become composition edges.

use std::collections::HashSet;
use std::path::Path;

use crate::manifest::split_node_reference;
use crate::types::{Edge, FileType, Node, NodeId};
use anyhow::anyhow;

use crate::compose_manifest::{LoadedManifest, load_toon_or_json};

/// A merged, flattened view: all workspace nodes/edges plus composition edges.
#[derive(Debug, Clone)]
pub struct UnifiedGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Number of workspaces merged in.
    pub workspace_count: usize,
    /// Number of composition (cross-workspace) edges added from the manifest.
    pub cross_edges: usize,
}

// ponytail: FileType::Document for workspace container nodes — `code`/`paper`/
// `image`/`rationale`/`concept` all misdescribe a workspace root; `document`
// is the least-wrong bucket for a non-code synthetic node.
const fn container_file_type() -> FileType {
    FileType::Document
}

/// Builds a synthetic workspace container node (`kind: workspace`).
fn container_node(ws_id: &str) -> Node {
    Node {
        id: NodeId(ws_id.to_string()),
        label: ws_id.to_string(),
        file_type: container_file_type(),
        kind: "workspace".to_string(),
        language: "unknown".to_string(),
        source_file: String::new(),
        start_line: 0,
        end_line: 0,
        doc_comment: None,
        description: None,
        metadata: None,
    }
}

fn prefix_id(ws_id: &str, local: &str) -> String {
    format!("{ws_id}::{local}")
}

// ponytail: allow too_many_lines — merge + relation resolution in one pass keeps
// the unified-model invariants (unique ids, endpoint checks) in one place.
#[allow(clippy::too_many_lines)]
pub fn build_unified_graph(loaded: &LoadedManifest) -> anyhow::Result<UnifiedGraph> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // 1) Project each workspace graph with prefixed ids.
    for (ws_id, root) in &loaded.roots {
        let graph = load_toon_or_json(root).map_err(|e| anyhow!("workspace `{ws_id}`: {e}"))?;
        nodes.push(container_node(ws_id));
        seen_ids.insert(ws_id.clone());
        for node in &graph.nodes {
            let prefixed = prefix_id(ws_id, &node.id.0);
            if !seen_ids.insert(prefixed.clone()) {
                return Err(anyhow!("合併時節點 id 衝突: {prefixed}"));
            }
            let mut n: Node = node.clone();
            n.id = NodeId(prefixed);
            nodes.push(n);
        }
        for edge in &graph.edges {
            let mut e: Edge = edge.clone();
            e.source = NodeId(prefix_id(ws_id, &edge.source.0));
            e.target = NodeId(prefix_id(ws_id, &edge.target.0));
            edges.push(e);
        }
    }

    // 2) Manifest relations become composition edges. Endpoint resolution rules:
    //    - `ws::node_id`  -> edge between the prefixed node ids
    //    - `ws` (bare)    -> edge between workspace container nodes
    let manifest_str = loaded.manifest_path.display().to_string();
    let mut cross_edges = 0usize;
    for rel in &loaded.manifest.relations {
        let resolve = |endpoint: &str| -> (String, NodeId) {
            match split_node_reference(endpoint) {
                Some((ws, node)) => {
                    let id = prefix_id(&ws, &node);
                    (ws, NodeId(id))
                }
                None => (endpoint.to_string(), NodeId(endpoint.to_string())),
            }
        };
        let (from_ws, from_id) = resolve(&rel.from);
        let (to_ws, to_id) = resolve(&rel.to);
        // Workspace container nodes exist from step 1; node-level endpoints
        // were validated during manifest load, so both ends must exist here.
        for end in [&from_id, &to_id] {
            if !seen_ids.contains(&end.0) {
                return Err(anyhow!(
                    "composition edge 端點不存在於合併圖: {}（relation `{}` → `{}`）",
                    end.0,
                    rel.from,
                    rel.to
                ));
            }
        }
        edges.push(Edge {
            source: from_id,
            target: to_id,
            relation: rel.relation.clone(),
            source_file: manifest_str.clone(),
            confidence: "INFERRED".to_string(),
            source_location: String::new(),
            description: None,
        });
        cross_edges += 1;
        // Silence unused warnings when from_ws/to_ws carry no extra info beyond
        // the endpoint id itself.
        let _ = (&from_ws, &to_ws);
    }

    Ok(UnifiedGraph {
        nodes,
        edges,
        workspace_count: loaded.roots.len(),
        cross_edges,
    })
}

/// Workspace container node ids in a unified graph (`{ws_id}` per workspace).
#[must_use]
pub fn container_ids(unified: &UnifiedGraph) -> Vec<String> {
    unified
        .nodes
        .iter()
        .filter(|n| n.kind == "workspace")
        .map(|n| n.id.0.clone())
        .collect()
}

/// Counts nodes belonging to one workspace (excluding its container node).
#[must_use]
pub fn workspace_node_count(unified: &UnifiedGraph, ws_id: &str) -> usize {
    let prefix = format!("{ws_id}::");
    unified
        .nodes
        .iter()
        .filter(|n| n.id.0.starts_with(&prefix))
        .count()
}

/// True when `path` points at the graphify CLI compose module (used by tests).
#[must_use]
pub fn is_compose_marker(path: &Path) -> bool {
    path.ends_with("compose")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GraphOutput;

    fn test_graph(node_local_id: &str) -> GraphOutput {
        GraphOutput {
            nodes: vec![Node {
                id: NodeId(node_local_id.to_string()),
                label: node_local_id.to_string(),
                file_type: FileType::Code,
                kind: "function".to_string(),
                language: "rust".to_string(),
                source_file: node_local_id.to_string(),
                start_line: 1,
                end_line: 2,
                doc_comment: None,
                description: None,
                metadata: None,
            }],
            edges: vec![Edge {
                source: NodeId(node_local_id.to_string()),
                target: NodeId(node_local_id.to_string()),
                relation: "calls".to_string(),
                source_file: node_local_id.to_string(),
                confidence: "EXTRACTED".to_string(),
                source_location: String::new(),
                description: None,
            }],
            metadata: crate::GraphMetadata::default(),
        }
    }

    #[test]
    fn test_prefix_id_format() {
        assert_eq!(prefix_id("ws-a", "src/main.rs"), "ws-a::src/main.rs");
        // test_graph exercises the GraphOutput fixture shape (see task 3.3
        // spec scenario test for real merge assertions).
        let g = test_graph("src/main.rs");
        assert_eq!(g.nodes.len(), 1);
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn test_container_node_fields() {
        let node = container_node("svc");
        assert_eq!(node.id, NodeId("svc".to_string()));
        assert_eq!(node.label, "svc");
        assert_eq!(node.language, "unknown");
        assert!(node.source_file.is_empty());
    }

    #[test]
    fn test_compose_marker() {
        assert!(is_compose_marker(Path::new("src/compose")));
        assert!(!is_compose_marker(Path::new("src/extract")));
    }

    fn loaded_with(
        workspaces: &[(&str, &str)],
        relations: Vec<crate::manifest::RelationEntry>,
    ) -> LoadedManifest {
        use crate::manifest::{AssemblyManifest, WorkspaceEntry};
        LoadedManifest {
            manifest: AssemblyManifest {
                workspaces: workspaces
                    .iter()
                    .map(|(id, path)| WorkspaceEntry {
                        id: (*id).to_string(),
                        path: (*path).to_string(),
                    })
                    .collect(),
                relations,
            },
            roots: workspaces
                .iter()
                .map(|(id, path)| ((*id).to_string(), std::path::PathBuf::from(path)))
                .collect(),
            manifest_path: std::path::PathBuf::from("test.yaml"),
        }
    }

    #[test]
    fn test_loaded_with_carries_relations() {
        let loaded = loaded_with(
            &[("ws-a", "/tmp/a"), ("ws-b", "/tmp/b")],
            vec![crate::manifest::RelationEntry {
                from: "ws-a".to_string(),
                relation: "uses".to_string(),
                to: "ws-b".to_string(),
            }],
        );
        assert_eq!(loaded.roots.len(), 2);
        assert_eq!(loaded.manifest.relations.len(), 1);
        assert_eq!(loaded.manifest.relations[0].relation, "uses");
    }

    #[test]
    fn test_container_node_uses_document_file_type() {
        let node = container_node("ws-a");
        assert_eq!(node.kind, "workspace");
        assert!(matches!(node.file_type, FileType::Document));
    }

    #[test]
    fn test_workspace_node_count_prefixes() {
        let unified = UnifiedGraph {
            nodes: vec![
                container_node("ws-a"),
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
                container_node("ws-b"),
            ],
            edges: vec![],
            workspace_count: 2,
            cross_edges: 0,
        };
        assert_eq!(workspace_node_count(&unified, "ws-a"), 1);
        assert_eq!(workspace_node_count(&unified, "ws-b"), 0);
    }
}
