use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub question: String,
}

#[derive(Debug, Deserialize)]
pub struct PathParams {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryNodeParams {
    pub node_id: String,
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct TracePathParams {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct ReindexParams {
    pub file_path: String,
}
