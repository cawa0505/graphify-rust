// ponytail: allow missing errors doc as this is an internal parser function propagating standard errors
#![allow(clippy::missing_errors_doc)]
// ponytail: allow collapsible_if for cleaner matching of AST node patterns
#![allow(clippy::collapsible_if)]

use crate::types::{Edge, ExtractionResult, FileType, Node, NodeId};
use anyhow::{Result, anyhow};
use tree_sitter::{Node as TSNode, Parser};

pub fn extract(content: &str, file_path: &str) -> Result<ExtractionResult> {
    let mut parser = Parser::new();
    let language = tree_sitter_java::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| anyhow!("Failed to load Java parser: {e}"))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("Failed to parse Java file: {file_path}"))?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // The module/class file itself is a Node
    let module_id = NodeId(format!("{file_path}:module"));
    nodes.push(Node {
        id: module_id.clone(),
        label: file_path.to_string(),
        file_type: FileType::Code,
        kind: "module".to_string(),
        language: "java".to_string(),
        source_file: file_path.to_string(),
        start_line: 0,
        end_line: content.lines().count(),
        doc_comment: None,
        description: Some(format!("Java module: {file_path}")),
        metadata: None,
    });

    let source_bytes = content.as_bytes();
    traverse_tree(
        tree.root_node(),
        source_bytes,
        file_path,
        &module_id,
        &mut nodes,
        &mut edges,
    )?;

    Ok(ExtractionResult { nodes, edges })
}

// ponytail: allow unnecessary_wraps as keeping Result<()> signature preserves consistent structure across all language extractors
#[allow(clippy::unnecessary_wraps)]
fn traverse_tree(
    node: TSNode,
    source_bytes: &[u8],
    file_path: &str,
    parent_id: &NodeId,
    nodes: &mut Vec<Node>,
    edges: &mut Vec<Edge>,
) -> Result<()> {
    let mut stack = vec![(node, parent_id.clone())];

    while let Some((current, current_parent_id)) = stack.pop() {
        let kind = current.kind();
        let mut next_parent_id = current_parent_id.clone();

        match kind {
            "class_declaration"
            | "interface_declaration"
            | "record_declaration"
            | "enum_declaration"
            | "annotation_type_declaration" => {
                if let Some(name_node) = current.child_by_field_name("name") {
                    let name = name_node.utf8_text(source_bytes).unwrap_or("UnknownClass");
                    let node_id = NodeId(format!("{file_path}:class:{name}"));
                    let start_line = current.start_position().row + 1;

                    nodes.push(Node {
                        id: node_id.clone(),
                        label: name.to_string(),
                        file_type: FileType::Code,
                        kind: "class".to_string(),
                        language: "java".to_string(),
                        source_file: file_path.to_string(),
                        start_line,
                        end_line: current.end_position().row + 1,
                        doc_comment: None,
                        description: Some(format!("{kind} {name}")),
                        metadata: None,
                    });
                    edges.push(Edge {
                        source: current_parent_id.clone(),
                        target: node_id.clone(),
                        relation: "contains".to_string(),
                        source_file: file_path.to_string(),
                        confidence: "EXTRACTED".to_string(),
                        source_location: format!("{file_path}:{start_line}"),
                        description: None,
                    });
                    next_parent_id = node_id;
                }
            }
            "method_declaration" | "constructor_declaration" => {
                if let Some(name_node) = current.child_by_field_name("name") {
                    let name = name_node.utf8_text(source_bytes).unwrap_or("UnknownMethod");
                    let node_id = NodeId(format!("{file_path}:function:{name}"));
                    let start_line = current.start_position().row + 1;

                    nodes.push(Node {
                        id: node_id.clone(),
                        label: name.to_string(),
                        file_type: FileType::Code,
                        kind: "function".to_string(),
                        language: "java".to_string(),
                        source_file: file_path.to_string(),
                        start_line,
                        end_line: current.end_position().row + 1,
                        doc_comment: None,
                        description: Some(format!("method {name}")),
                        metadata: None,
                    });
                    edges.push(Edge {
                        source: current_parent_id.clone(),
                        target: node_id.clone(),
                        relation: "contains".to_string(),
                        source_file: file_path.to_string(),
                        confidence: "EXTRACTED".to_string(),
                        source_location: format!("{file_path}:{start_line}"),
                        description: None,
                    });

                    find_calls(current, source_bytes, file_path, &node_id, edges);
                }
            }
            _ => {}
        }

        // Push children to stack
        let count = current.child_count();
        for i in (0..count).rev() {
            if let Some(child) = current.child(i) {
                stack.push((child, next_parent_id.clone()));
            }
        }
    }
    Ok(())
}

fn find_calls(
    node: TSNode,
    source_bytes: &[u8],
    file_path: &str,
    caller_id: &NodeId,
    edges: &mut Vec<Edge>,
) {
    let mut cursor = node.walk();
    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        if current.kind() == "method_invocation" {
            if let Some(name_node) = current.child_by_field_name("name") {
                let callee_name = name_node.utf8_text(source_bytes).unwrap_or("UnknownMethod");
                let target_id = NodeId(format!("{file_path}:function:{callee_name}"));
                let start_line = current.start_position().row + 1;
                edges.push(Edge {
                    source: caller_id.clone(),
                    target: target_id,
                    relation: "calls".to_string(),
                    source_file: file_path.to_string(),
                    confidence: "INFERRED".to_string(),
                    source_location: format!("{file_path}:{start_line}"),
                    description: None,
                });
            }
        } else if current.kind() == "object_creation_expression" {
            if let Some(type_node) = current.child_by_field_name("type") {
                let callee_name = type_node.utf8_text(source_bytes).unwrap_or("UnknownClass");
                let target_id = NodeId(format!("{file_path}:class:{callee_name}"));
                let start_line = current.start_position().row + 1;
                edges.push(Edge {
                    source: caller_id.clone(),
                    target: target_id,
                    relation: "calls".to_string(),
                    source_file: file_path.to_string(),
                    confidence: "INFERRED".to_string(),
                    source_location: format!("{file_path}:{start_line}"),
                    description: None,
                });
            }
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}
