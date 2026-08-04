use crate::types::{Node, Edge, ExtractionResult, NodeId, FileType, NodeKind};
use anyhow::{Result, anyhow};
use tree_sitter::{Parser, Node as TSNode};

pub fn extract(content: &str, file_path: &str) -> Result<ExtractionResult> {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| anyhow!("Failed to load Rust parser: {e}"))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("Failed to parse Rust file: {file_path}"))?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // The module itself is a Node
    let module_id = NodeId(format!("{file_path}:module"));
    nodes.push(Node {
        id: module_id.clone(),
        label: file_path.to_string(),
        file_type: FileType::Code,
        kind: NodeKind::Module,
        file_path: file_path.to_string(),
        start_line: 0,
        end_line: content.lines().count(),
        description: Some(format!("Rust module: {file_path}")),
    });

    let source_bytes = content.as_bytes();
    traverse_tree(tree.root_node(), source_bytes, file_path, &module_id, &mut nodes, &mut edges)?;

    Ok(ExtractionResult { nodes, edges })
}

fn traverse_tree(
    node: TSNode,
    source_bytes: &[u8],
    file_path: &str,
    parent_module_id: &NodeId,
    nodes: &mut Vec<Node>,
    edges: &mut Vec<Edge>,
) -> Result<()> {
    let kind = node.kind();
    match kind {
        "struct_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_bytes).unwrap_or("UnknownStruct");
                let node_id = NodeId(format!("{}:struct:{}", file_path, name));
                nodes.push(Node {
                    id: node_id.clone(),
                    label: name.to_string(),
                    file_type: FileType::Code,
                    kind: NodeKind::Struct,
                    file_path: file_path.to_string(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    description: Some(format!("struct {}", name)),
                });
                edges.push(Edge {
                    source: parent_module_id.clone(),
                    target: node_id,
                    relation: "contains".to_string(),
                    source_file: file_path.to_string(),
                    confidence: "EXTRACTED".to_string(),
                    description: None,
                });
            }
        }
        "trait_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_bytes).unwrap_or("UnknownTrait");
                let node_id = NodeId(format!("{}:trait:{}", file_path, name));
                nodes.push(Node {
                    id: node_id.clone(),
                    label: name.to_string(),
                    file_type: FileType::Code,
                    kind: NodeKind::Trait,
                    file_path: file_path.to_string(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    description: Some(format!("trait {}", name)),
                });
                edges.push(Edge {
                    source: parent_module_id.clone(),
                    target: node_id,
                    relation: "contains".to_string(),
                    source_file: file_path.to_string(),
                    confidence: "EXTRACTED".to_string(),
                    description: None,
                });
            }
        }
        "function_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_bytes).unwrap_or("UnknownFunction");
                let node_id = NodeId(format!("{}:function:{}", file_path, name));
                nodes.push(Node {
                    id: node_id.clone(),
                    label: name.to_string(),
                    file_type: FileType::Code,
                    kind: NodeKind::Function,
                    file_path: file_path.to_string(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    description: Some(format!("fn {}", name)),
                });
                edges.push(Edge {
                    source: parent_module_id.clone(),
                    target: node_id.clone(),
                    relation: "contains".to_string(),
                    source_file: file_path.to_string(),
                    confidence: "EXTRACTED".to_string(),
                    description: None,
                });

                // Find any nested function calls inside this function
                find_calls(node, source_bytes, file_path, &node_id, edges);
            }
        }
        "use_declaration" => {
            let path_str = node.utf8_text(source_bytes).unwrap_or("");
            // Clean up use statement
            let cleaned = path_str.trim_start_matches("use ").trim_end_matches(';').to_string();
            let target_id = NodeId(format!("import:{}", cleaned));
            edges.push(Edge {
                source: parent_module_id.clone(),
                target: target_id,
                relation: "imports".to_string(),
                source_file: file_path.to_string(),
                confidence: "EXTRACTED".to_string(),
                description: Some(cleaned),
            });
        }
        _ => {}
    }

    // Traverse children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree(child, source_bytes, file_path, parent_module_id, nodes, edges)?;
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
                    edges.push(Edge {
                        source: caller_id.clone(),
                        target: NodeId(format!("{}:function:{}", file_path, name)),
                        relation: "calls".to_string(),
                        source_file: file_path.to_string(),
                        confidence: "EXTRACTED".to_string(),
                        description: Some(format!("calls {}", name)),
                    });
                }
            }
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}
