use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    Code,
    Document,
    Paper,
    Image,
    Rationale,
    Concept,
}

fn default_language() -> String {
    "unknown".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub label: String,
    pub file_type: FileType,
    pub kind: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(alias = "file_path")]
    pub source_file: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

fn default_confidence() -> String {
    "EXTRACTED".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source: NodeId,
    pub target: NodeId,
    pub relation: String,
    pub source_file: String,
    #[serde(default = "default_confidence")]
    pub confidence: String,
    #[serde(default)]
    pub source_location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphMetadata {
    pub version: String,
    pub generated_at: String,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub languages: Vec<String>,
    pub input_tokens: usize,
    pub output_tokens: usize,
    /// Reserved, versioned container for plugin-owned metadata keyed by
    /// `plugin_id`. Core never interprets entries; absent or unknown plugins
    /// are tolerated. Empty when no plugin enrichment is present.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_data: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphOutput {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub metadata: GraphMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
