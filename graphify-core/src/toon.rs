// ponytail: allow missing errors doc for internal TOON serialization functions
#![allow(clippy::missing_errors_doc)]
// ponytail: allow uninlined format args to support legacy python-compatible style
#![allow(clippy::uninlined_format_args)]

use crate::types::{FileType, GraphMetadata, GraphOutput, Node, NodeId, Edge};
use anyhow::Result;
use std::fmt::Write;

/// Helper to escape a string for TOON format.
/// If `in_tabular` is true, also treats commas as structural characters that require quoting.
fn escape_string(s: &str, in_tabular: bool) -> String {
    let needs_quoting = s.is_empty()
        || s == "null"
        || s == "true"
        || s == "false"
        || s.chars().any(|c| {
            c.is_whitespace()
                || c == ':'
                || c == '['
                || c == ']'
                || c == '{'
                || c == '}'
                || c == '-'
                || c == '\\'
                || c == '"'
                || (in_tabular && c == ',')
        })
        || s.starts_with('-')
        || s.chars().next().is_some_and(|c| c.is_ascii_digit()); // Numeric looking

    if needs_quoting {
        let mut escaped = String::new();
        escaped.push('"');
        for c in s.chars() {
            match c {
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                _ => escaped.push(c),
            }
        }
        escaped.push('"');
        escaped
    } else {
        s.to_string()
    }
}

/// Helper to unescape a string from TOON format.
fn unescape_string(s: &str) -> String {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let mut unescaped = String::new();
        let chars: Vec<char> = s[1..s.len() - 1].chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                match chars[i + 1] {
                    '"' => unescaped.push('"'),
                    '\\' => unescaped.push('\\'),
                    'n' => unescaped.push('\n'),
                    'r' => unescaped.push('\r'),
                    't' => unescaped.push('\t'),
                    _ => {
                        unescaped.push('\\');
                        unescaped.push(chars[i + 1]);
                    }
                }
                i += 2;
            } else {
                unescaped.push(chars[i]);
                i += 1;
            }
        }
        unescaped
    } else {
        s.to_string()
    }
}

/// Robust CSV splitter that respects double-quoted strings and escaped characters.
fn split_csv_line(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for c in s.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if c == '\\' {
            current.push(c);
            escaped = true;
        } else if c == '"' {
            current.push(c);
            in_quotes = !in_quotes;
        } else if c == ',' && !in_quotes {
            parts.push(current.clone());
            current.clear();
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

/// Serialize a `GraphOutput` into TOON format string.
#[must_use]
pub fn to_toon(graph: &GraphOutput) -> String {
    let mut out = String::new();

    // 1. Metadata
    out.push_str("metadata:\n");
    let _ = writeln!(out, "  version: {}", escape_string(&graph.metadata.version, false));
    let _ = writeln!(out, "  generated_at: {}", escape_string(&graph.metadata.generated_at, false));
    let _ = writeln!(out, "  total_nodes: {}", graph.metadata.total_nodes);
    let _ = writeln!(out, "  total_edges: {}", graph.metadata.total_edges);
    
    if graph.metadata.languages.is_empty() {
        out.push_str("  languages[0]:\n");
    } else {
        let escaped_langs: Vec<String> = graph.metadata.languages.iter().map(|l| escape_string(l, true)).collect();
        let _ = writeln!(out, "  languages[{}]: {}", escaped_langs.len(), escaped_langs.join(","));
    }
    let _ = writeln!(out, "  input_tokens: {}", graph.metadata.input_tokens);
    let _ = writeln!(out, "  output_tokens: {}", graph.metadata.output_tokens);
    out.push('\n');

    // 2. Nodes
    let _ = writeln!(out, "nodes[{},]{{id,label,file_type,kind,language,source_file,start_line,end_line,doc_comment,description,metadata}}:", graph.nodes.len());
    for node in &graph.nodes {
        let file_type_str = match node.file_type {
            FileType::Code => "code",
            FileType::Document => "document",
            FileType::Paper => "paper",
            FileType::Image => "image",
            FileType::Rationale => "rationale",
            FileType::Concept => "concept",
        };
        let doc_comment_str = node.doc_comment.as_deref().map_or_else(|| "null".to_string(), |s| escape_string(s, true));
        let description_str = node.description.as_deref().map_or_else(|| "null".to_string(), |s| escape_string(s, true));
        let metadata_str = node.metadata.as_ref().map_or_else(|| "null".to_string(), |m| escape_string(&serde_json::to_string(m).unwrap_or_default(), true));

        let _ = writeln!(
            out,
            "  {},{},{},{},{},{},{},{},{},{},{}",
            escape_string(&node.id.0, true),
            escape_string(&node.label, true),
            file_type_str,
            escape_string(&node.kind, true),
            escape_string(&node.language, true),
            escape_string(&node.source_file, true),
            node.start_line,
            node.end_line,
            doc_comment_str,
            description_str,
            metadata_str
        );
    }
    out.push('\n');

    // 3. Edges
    let _ = writeln!(out, "edges[{},]{{source,target,relation,source_file,confidence,source_location,description}}:", graph.edges.len());
    for edge in &graph.edges {
        let description_str = edge.description.as_deref().map_or_else(|| "null".to_string(), |s| escape_string(s, true));

        let _ = writeln!(
            out,
            "  {},{},{},{},{},{},{}",
            escape_string(&edge.source.0, true),
            escape_string(&edge.target.0, true),
            escape_string(&edge.relation, true),
            escape_string(&edge.source_file, true),
            escape_string(&edge.confidence, true),
            escape_string(&edge.source_location, true),
            description_str
        );
    }

    out
}

/// Deserialize a TOON format string into `GraphOutput`.
pub fn from_toon(toon_str: &str) -> Result<GraphOutput> {
    let mut version = String::new();
    let mut generated_at = String::new();
    let mut total_nodes_meta = 0;
    let mut total_edges_meta = 0;
    let mut languages = Vec::new();
    let mut input_tokens = 0;
    let mut output_tokens = 0;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let mut lines = toon_str.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "metadata:" {
            // Read metadata block
            for m_line in lines.by_ref() {
                let m_trimmed = m_line.trim();
                if m_trimmed.is_empty() {
                    break;
                }
                if !m_line.starts_with("  ") {
                    break;
                }

                if let Some((k, v)) = m_trimmed.split_once(':') {
                    let k = k.trim();
                    let v = v.trim();
                    match k {
                        "version" => version = unescape_string(v),
                        "generated_at" => generated_at = unescape_string(v),
                        "total_nodes" => total_nodes_meta = v.parse().unwrap_or(0),
                        "total_edges" => total_edges_meta = v.parse().unwrap_or(0),
                        "input_tokens" => input_tokens = v.parse().unwrap_or(0),
                        "output_tokens" => output_tokens = v.parse().unwrap_or(0),
                        _ if k.starts_with("languages[") && !v.is_empty() => {
                            for lang in split_csv_line(v) {
                                languages.push(unescape_string(&lang));
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else if trimmed.starts_with("nodes[") {
            // Parse tabular array
            for n_line in lines.by_ref() {
                let n_trimmed = n_line.trim();
                if n_trimmed.is_empty() {
                    break;
                }
                if !n_line.starts_with("  ") {
                    break;
                }
                let parts = split_csv_line(n_trimmed);
                if parts.len() >= 8 {
                    let id = NodeId(unescape_string(&parts[0]));
                    let label = unescape_string(&parts[1]);
                    let file_type = match parts[2].as_str() {
                        "document" => FileType::Document,
                        "paper" => FileType::Paper,
                        "image" => FileType::Image,
                        "rationale" => FileType::Rationale,
                        "concept" => FileType::Concept,
                        _ => FileType::Code,
                    };
                    let kind = unescape_string(&parts[3]);
                    let language = unescape_string(&parts[4]);
                    let source_file = unescape_string(&parts[5]);
                    let start_line = parts[6].parse().unwrap_or(0);
                    let end_line = parts[7].parse().unwrap_or(0);

                    let doc_comment = if parts.len() > 8 && parts[8] != "null" {
                        Some(unescape_string(&parts[8]))
                    } else {
                        None
                    };

                    let description = if parts.len() > 9 && parts[9] != "null" {
                        Some(unescape_string(&parts[9]))
                    } else {
                        None
                    };

                    let metadata = if parts.len() > 10 && parts[10] != "null" {
                        serde_json::from_str(&unescape_string(&parts[10])).ok()
                    } else {
                        None
                    };

                    nodes.push(Node {
                        id,
                        label,
                        file_type,
                        kind,
                        language,
                        source_file,
                        start_line,
                        end_line,
                        doc_comment,
                        description,
                        metadata,
                    });
                }
            }
        } else if trimmed.starts_with("edges[") {
            // Parse tabular array
            for e_line in lines.by_ref() {
                let e_trimmed = e_line.trim();
                if e_trimmed.is_empty() {
                    break;
                }
                if !e_line.starts_with("  ") {
                    break;
                }
                let parts = split_csv_line(e_trimmed);
                if parts.len() >= 6 {
                    let source = NodeId(unescape_string(&parts[0]));
                    let target = NodeId(unescape_string(&parts[1]));
                    let relation = unescape_string(&parts[2]);
                    let source_file = unescape_string(&parts[3]);
                    let confidence = unescape_string(&parts[4]);
                    let source_location = unescape_string(&parts[5]);

                    let description = if parts.len() > 6 && parts[6] != "null" {
                        Some(unescape_string(&parts[6]))
                    } else {
                        None
                    };

                    edges.push(Edge {
                        source,
                        target,
                        relation,
                        source_file,
                        confidence,
                        source_location,
                        description,
                    });
                }
            }
        }
    }

    Ok(GraphOutput {
        nodes,
        edges,
        metadata: GraphMetadata {
            version,
            generated_at,
            total_nodes: total_nodes_meta,
            total_edges: total_edges_meta,
            languages,
            input_tokens,
            output_tokens,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toon_serialization_roundtrip() -> Result<()> {
        let original = GraphOutput {
            metadata: GraphMetadata {
                version: "1.0.0".to_string(),
                generated_at: "2026-08-05".to_string(),
                total_nodes: 2,
                total_edges: 1,
                languages: vec!["rust".to_string(), "python".to_string()],
                input_tokens: 1500,
                output_tokens: 300,
            },
            nodes: vec![
                Node {
                    id: NodeId("node1".to_string()),
                    label: "My Label".to_string(),
                    file_type: FileType::Code,
                    kind: "struct".to_string(),
                    language: "rust".to_string(),
                    source_file: "src/lib.rs".to_string(),
                    start_line: 10,
                    end_line: 25,
                    doc_comment: Some("This is a doc\ncomment".to_string()),
                    description: Some("My description with , commas".to_string()),
                    metadata: None,
                },
                Node {
                    id: NodeId("node2".to_string()),
                    label: "Other".to_string(),
                    file_type: FileType::Concept,
                    kind: "concept".to_string(),
                    language: "unknown".to_string(),
                    source_file: "doc.md".to_string(),
                    start_line: 0,
                    end_line: 0,
                    doc_comment: None,
                    description: None,
                    metadata: {
                        let mut meta_map = serde_json::Map::new();
                        meta_map.insert("key".to_string(), serde_json::Value::String("val".to_string()));
                        Some(meta_map)
                    },
                },
            ],
            edges: vec![Edge {
                source: NodeId("node1".to_string()),
                target: NodeId("node2".to_string()),
                relation: "depends_on".to_string(),
                source_file: "src/lib.rs".to_string(),
                confidence: "EXTRACTED".to_string(),
                source_location: "src/lib.rs:12".to_string(),
                description: Some("Some edge desc".to_string()),
            }],
        };

        let serialized = to_toon(&original);
        let deserialized = from_toon(&serialized)?;

        assert_eq!(deserialized.metadata.version, original.metadata.version);
        assert_eq!(deserialized.metadata.generated_at, original.metadata.generated_at);
        assert_eq!(deserialized.metadata.total_nodes, original.metadata.total_nodes);
        assert_eq!(deserialized.metadata.total_edges, original.metadata.total_edges);
        assert_eq!(deserialized.metadata.languages, original.metadata.languages);
        assert_eq!(deserialized.metadata.input_tokens, original.metadata.input_tokens);
        assert_eq!(deserialized.metadata.output_tokens, original.metadata.output_tokens);

        assert_eq!(deserialized.nodes.len(), original.nodes.len());
        assert_eq!(deserialized.nodes[0].id, original.nodes[0].id);
        assert_eq!(deserialized.nodes[0].label, original.nodes[0].label);
        assert_eq!(deserialized.nodes[0].file_type, original.nodes[0].file_type);
        assert_eq!(deserialized.nodes[0].kind, original.nodes[0].kind);
        assert_eq!(deserialized.nodes[0].language, original.nodes[0].language);
        assert_eq!(deserialized.nodes[0].source_file, original.nodes[0].source_file);
        assert_eq!(deserialized.nodes[0].start_line, original.nodes[0].start_line);
        assert_eq!(deserialized.nodes[0].end_line, original.nodes[0].end_line);
        assert_eq!(deserialized.nodes[0].doc_comment, original.nodes[0].doc_comment);
        assert_eq!(deserialized.nodes[0].description, original.nodes[0].description);
        assert_eq!(deserialized.nodes[0].metadata, original.nodes[0].metadata);

        assert_eq!(deserialized.nodes[1].metadata, original.nodes[1].metadata);

        assert_eq!(deserialized.edges.len(), original.edges.len());
        assert_eq!(deserialized.edges[0].source, original.edges[0].source);
        assert_eq!(deserialized.edges[0].target, original.edges[0].target);
        assert_eq!(deserialized.edges[0].relation, original.edges[0].relation);
        assert_eq!(deserialized.edges[0].confidence, original.edges[0].confidence);
        assert_eq!(deserialized.edges[0].source_location, original.edges[0].source_location);
        assert_eq!(deserialized.edges[0].description, original.edges[0].description);

        Ok(())
    }
}
