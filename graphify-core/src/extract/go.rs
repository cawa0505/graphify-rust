use crate::types::{Node, Edge, ExtractionResult, NodeId, FileType};
use anyhow::{Result, anyhow};
use tree_sitter::{Parser, Node as TSNode};

pub fn extract(content: &str, file_path: &str) -> Result<ExtractionResult> {
    let mut parser = Parser::new();
    let language = tree_sitter_go::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| anyhow!("Failed to load Go parser: {e}"))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("Failed to parse Go file: {file_path}"))?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // The module itself is a Node
    let module_id = NodeId(format!("{file_path}:module"));
    nodes.push(Node {
        id: module_id.clone(),
        label: file_path.to_string(),
        file_type: FileType::Code,
        kind: "module".to_string(),
        language: "go".to_string(),
        source_file: file_path.to_string(),
        start_line: 0,
        end_line: content.lines().count(),
        doc_comment: None,
        description: Some(format!("Go module: {file_path}")),
        metadata: None,
    });

    let source_bytes = content.as_bytes();
    traverse_tree(tree.root_node(), source_bytes, file_path, &module_id, &mut nodes, &mut edges)?;

    Ok(ExtractionResult { nodes, edges })
}

// ponytail: allow unnecessary_wraps as keeping Result<()> signature preserves consistent structure across all language extractors
#[allow(clippy::unnecessary_wraps)]
fn traverse_tree(
    node: TSNode,
    source_bytes: &[u8],
    file_path: &str,
    parent_module_id: &NodeId,
    nodes: &mut Vec<Node>,
    edges: &mut Vec<Edge>,
) -> Result<()> {
    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        let kind = current.kind();
        match kind {
            "package_clause" => {
                if let Some(name_node) = current.child_by_field_name("name") {
                    let name = name_node.utf8_text(source_bytes).unwrap_or("main");
                    let node_id = NodeId(format!("{}:package:{}", file_path, name));
                    let start_line = current.start_position().row + 1;
                    nodes.push(Node {
                        id: node_id.clone(),
                        label: name.to_string(),
                        file_type: FileType::Code,
                        kind: "module".to_string(),
                        language: "go".to_string(),
                        source_file: file_path.to_string(),
                        start_line,
                        end_line: current.end_position().row + 1,
                        doc_comment: None,
                        description: Some(format!("package {}", name)),
                        metadata: None,
                    });
                    edges.push(Edge {
                        source: parent_module_id.clone(),
                        target: node_id,
                        relation: "contains".to_string(),
                        source_file: file_path.to_string(),
                        confidence: "EXTRACTED".to_string(),
                        source_location: format!("{file_path}:{start_line}"),
                        description: None,
                    });
                }
            }
            "function_declaration" => {
                if let Some(name_node) = current.child_by_field_name("name") {
                    let name = name_node.utf8_text(source_bytes).unwrap_or("UnknownFunction");
                    let node_id = NodeId(format!("{file_path}:function:{name}"));
                    let start_line = current.start_position().row + 1;
                    nodes.push(Node {
                        id: node_id.clone(),
                        label: name.to_string(),
                        file_type: FileType::Code,
                        kind: "function".to_string(),
                        language: "go".to_string(),
                        source_file: file_path.to_string(),
                        start_line,
                        end_line: current.end_position().row + 1,
                        doc_comment: None,
                        description: Some(format!("func {name}")),
                        metadata: None,
                    });
                    edges.push(Edge {
                        source: parent_module_id.clone(),
                        target: node_id.clone(),
                        relation: "contains".to_string(),
                        source_file: file_path.to_string(),
                        confidence: "EXTRACTED".to_string(),
                        source_location: format!("{file_path}:{start_line}"),
                        description: None,
                    });

                    // Find any nested function calls inside this function
                    find_calls(current, source_bytes, file_path, &node_id, edges);
                }
            }
            "import_declaration" => {
                let path_str = current.utf8_text(source_bytes).unwrap_or("");
                let cleaned = path_str.trim_start_matches("import ").trim_end_matches(';').to_string();
                let target_id = NodeId(format!("import:{cleaned}"));
                let start_line = current.start_position().row + 1;
                edges.push(Edge {
                    source: parent_module_id.clone(),
                    target: target_id,
                    relation: "imports".to_string(),
                    source_file: file_path.to_string(),
                    confidence: "EXTRACTED".to_string(),
                    source_location: format!("{file_path}:{start_line}"),
                    description: Some(cleaned),
                });
            }
            "type_spec" => {
                if let Some(name_node) = current.child_by_field_name("name") {
                    let name = name_node.utf8_text(source_bytes).unwrap_or("UnknownType");
                    let node_id = NodeId(format!("{file_path}:type:{name}"));
                    let start_line = current.start_position().row + 1;
                    nodes.push(Node {
                        id: node_id.clone(),
                        label: name.to_string(),
                        file_type: FileType::Code,
                        kind: "struct".to_string(),
                        language: "go".to_string(),
                        source_file: file_path.to_string(),
                        start_line,
                        end_line: current.end_position().row + 1,
                        doc_comment: None,
                        description: Some(format!("type {name}")),
                        metadata: None,
                    });
                    edges.push(Edge {
                        source: parent_module_id.clone(),
                        target: node_id,
                        relation: "contains".to_string(),
                        source_file: file_path.to_string(),
                        confidence: "EXTRACTED".to_string(),
                        source_location: format!("{file_path}:{start_line}"),
                        description: None,
                    });
                }
            }
            _ => {}
        }

        let count = current.child_count();
        for i in (0..count).rev() {
            if let Some(child) = current.child(i) {
                stack.push(child);
            }
        }
    }

    Ok(())
}

fn find_calls(node: TSNode, source_bytes: &[u8], file_path: &str, caller_id: &NodeId, edges: &mut Vec<Edge>) {
    let mut cursor = node.walk();
    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        if current.kind() == "call_expression" {
            if let Some(function_node) = current.child_by_field_name("function") {
                let name = function_node.utf8_text(source_bytes).unwrap_or("");
                if !name.is_empty() && !name.contains('.') && !name.contains(':') {
                    let start_line = current.start_position().row + 1;
                    edges.push(Edge {
                        source: caller_id.clone(),
                        target: NodeId(format!("{file_path}:function:{name}")),
                        relation: "calls".to_string(),
                        source_file: file_path.to_string(),
                        confidence: "EXTRACTED".to_string(),
                        source_location: format!("{file_path}:{start_line}"),
                        description: Some(format!("calls {name}")),
                    });
                }
            }
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}
