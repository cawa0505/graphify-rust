// ponytail: allow standard clippy lints to keep stdio MCP server implementation extremely simple and robust
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::needless_pass_by_value)]

mod types;

use anyhow::{anyhow, Result};
use graphify_core::extract::extract_file;
use graphify_core::graph::build_graph;
use graphify_core::graph::path::find_shortest_path;
use graphify_core::graph::query::query_bfs;
use graphify_core::types::{Edge, GraphMetadata, GraphOutput, Node, NodeId};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::{Arc, RwLock};
use types::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, PathParams, QueryNodeParams, QueryParams,
    ReindexParams, TracePathParams,
};

struct GraphState {
    graph_data: GraphOutput,
    graph: DiGraph<Node, Edge>,
    node_map: HashMap<NodeId, NodeIndex>,
}

impl GraphState {
    fn load() -> Result<Self> {
        let graph_path = Path::new("graphify-out/graph.json");
        let fallback_path = Path::new("graph.json");

        let graph_data = if graph_path.exists() {
            let file = File::open(graph_path)
                .map_err(|e| anyhow!("Failed to open graphify-out/graph.json: {}", e))?;
            serde_json::from_reader(file)
                .map_err(|e| anyhow!("Failed to parse graphify-out/graph.json: {}", e))?
        } else if fallback_path.exists() {
            let file = File::open(fallback_path)
                .map_err(|e| anyhow!("Failed to open graph.json: {}", e))?;
            serde_json::from_reader(file)
                .map_err(|e| anyhow!("Failed to parse graph.json: {}", e))?
        } else {
            GraphOutput {
                nodes: Vec::new(),
                edges: Vec::new(),
                metadata: GraphMetadata {
                    version: "1.0".to_string(),
                    generated_at: "0".to_string(),
                    total_nodes: 0,
                    total_edges: 0,
                    languages: Vec::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                },
            }
        };

        let (graph, node_map) = build_graph(&graph_data.nodes, &graph_data.edges)?;

        Ok(Self {
            graph_data,
            graph,
            node_map,
        })
    }

    fn rebuild_graph(&mut self) -> Result<()> {
        let (graph, node_map) = build_graph(&self.graph_data.nodes, &self.graph_data.edges)?;
        self.graph = graph;
        self.node_map = node_map;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let graph_path = Path::new("graphify-out/graph.json");
        if let Some(parent) = graph_path.parent() {
            fs::create_dir_all(parent)?;
            let gitignore_path = parent.join(".gitignore");
            if !gitignore_path.exists() {
                fs::write(&gitignore_path, "*\n")?;
            }
        }
        let file = File::create(graph_path)?;
        serde_json::to_writer_pretty(file, &self.graph_data)?;
        Ok(())
    }
}

fn main() -> Result<()> {
    eprintln!("graphify-mcp: MCP server starting on stdio");

    let state = Arc::new(RwLock::new(GraphState::load()?));

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => {
                let response = handle_request(request, Arc::clone(&state));
                let response_json = serde_json::to_string(&response)?;
                writeln!(stdout, "{response_json}")?;
                stdout.flush()?;
            }
            Err(e) => {
                eprintln!("Failed to parse request: {e}");
            }
        }
    }

    Ok(())
}

fn handle_request(request: JsonRpcRequest, state_lock: Arc<RwLock<GraphState>>) -> JsonRpcResponse {
    let method = request.method.as_str();
    match method {
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "graphify-mcp",
                    "version": "0.1.0"
                }
            })),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(serde_json::json!({
                "tools": [
                    {
                        "name": "graphify_query",
                        "description": "BFS traversal of the knowledge graph (legacy compatibility)",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "question": { "type": "string" }
                            },
                            "required": ["question"]
                        }
                    },
                    {
                        "name": "graphify_path",
                        "description": "Find shortest path between two nodes (legacy compatibility)",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "source": { "type": "string" },
                                "target": { "type": "string" }
                            },
                            "required": ["source", "target"]
                        }
                    },
                    {
                        "name": "graph_summary",
                        "description": "Get high-level topology summary",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "graph_query_node",
                        "description": "Query nodes by ID with depth",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "node_id": { "type": "string" },
                                "depth": { "type": "integer", "default": 1 }
                            },
                            "required": ["node_id"]
                        }
                    },
                    {
                        "name": "graph_trace_path",
                        "description": "Find shortest path between two nodes",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "from": { "type": "string" },
                                "to": { "type": "string" }
                            },
                            "required": ["from", "to"]
                        }
                    },
                    {
                        "name": "graph_reindex",
                        "description": "Reindex a file into the graph",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "file_path": { "type": "string" }
                            },
                            "required": ["file_path"]
                        }
                    }
                ]
            })),
            error: None,
        },
        "tools/call" => {
            let params = request.params.clone();
            let tool_name = match params.get("name").and_then(|v| v.as_str()) {
                Some(name) => name,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing 'name' in tools/call parameters".to_string(),
                        }),
                    };
                }
            };

            let tool_arguments = params.get("arguments").cloned().unwrap_or_default();

            match handle_tool_call(tool_name, tool_arguments, state_lock) {
                Ok(val) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(val),
                    error: None,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: format!("Tool call error: {e}"),
                    }),
                },
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {method}"),
            }),
        },
    }
}

fn handle_tool_call(
    name: &str,
    args: serde_json::Value,
    state_lock: Arc<RwLock<GraphState>>,
) -> Result<serde_json::Value> {
    match name {
        "graphify_query" => {
            let params: QueryParams = serde_json::from_value(args)?;
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            let node_id = NodeId(params.question.clone());
            let node_id_resolved = find_node_by_id_or_label(&r_state.graph_data, &node_id)
                .ok_or_else(|| anyhow!("Node not found: {}", params.question))?;

            let res = query_bfs(&r_state.graph, &r_state.node_map, &node_id_resolved, 2)?;
            Ok(serde_json::to_value(res)?)
        }
        "graphify_path" => {
            let params: PathParams = serde_json::from_value(args)?;
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            let src_resolved = find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.source.clone()))
                .ok_or_else(|| anyhow!("Source node not found: {}", params.source))?;
            let tgt_resolved = find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.target.clone()))
                .ok_or_else(|| anyhow!("Target node not found: {}", params.target))?;

            let path_opt = find_shortest_path(&r_state.graph, &r_state.node_map, &src_resolved, &tgt_resolved)?;
            Ok(serde_json::to_value(path_opt)?)
        }
        "graph_summary" => {
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            // Return top-level module topology, core structs and classes
            let mut summary_nodes = Vec::new();
            for node in &r_state.graph_data.nodes {
                if node.kind == "module" || node.kind == "class" || node.kind == "struct" || node.kind == "trait" || node.kind == "interface" {
                    summary_nodes.push(node.clone());
                }
            }
            // Keep summary lightweight
            summary_nodes.truncate(15);

            let mut summary_edges = Vec::new();
            let summary_node_ids: std::collections::HashSet<&NodeId> = summary_nodes.iter().map(|n| &n.id).collect();
            for edge in &r_state.graph_data.edges {
                if summary_node_ids.contains(&edge.source) && summary_node_ids.contains(&edge.target) {
                    summary_edges.push(edge.clone());
                }
            }

            Ok(serde_json::json!({
                "version": r_state.graph_data.metadata.version,
                "total_nodes": r_state.graph_data.metadata.total_nodes,
                "total_edges": r_state.graph_data.metadata.total_edges,
                "languages": r_state.graph_data.metadata.languages,
                "summary_nodes": summary_nodes,
                "summary_edges": summary_edges,
            }))
        }
        "graph_query_node" => {
            let params: QueryNodeParams = serde_json::from_value(args)?;
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            let node_id = NodeId(params.node_id.clone());
            let node_id_resolved = find_node_by_id_or_label(&r_state.graph_data, &node_id)
                .ok_or_else(|| anyhow!("Node not found: {}", params.node_id))?;

            let depth = params.depth.unwrap_or(1);
            let res = query_bfs(&r_state.graph, &r_state.node_map, &node_id_resolved, depth)?;
            Ok(serde_json::to_value(res)?)
        }
        "graph_trace_path" => {
            let params: TracePathParams = serde_json::from_value(args)?;
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            let src_resolved = find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.from.clone()))
                .ok_or_else(|| anyhow!("Source node not found: {}", params.from))?;
            let tgt_resolved = find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.to.clone()))
                .ok_or_else(|| anyhow!("Target node not found: {}", params.to))?;

            let path_opt = find_shortest_path(&r_state.graph, &r_state.node_map, &src_resolved, &tgt_resolved)?;
            Ok(serde_json::to_value(path_opt)?)
        }
        "graph_reindex" => {
            let params: ReindexParams = serde_json::from_value(args)?;
            let file_path = Path::new(&params.file_path);
            if !file_path.exists() {
                anyhow::bail!("File not found: {}", params.file_path);
            }

            // Extract file AST
            let extracted = extract_file(file_path)?;

            let mut w_state = state_lock.write().map_err(|_| anyhow!("RwLock poisoned"))?;

            // Remove old nodes and edges originating from this file path
            w_state.graph_data.nodes.retain(|n| n.source_file != params.file_path);
            w_state.graph_data.edges.retain(|e| e.source_file != params.file_path);

            // Add new nodes and edges
            w_state.graph_data.nodes.extend(extracted.nodes);
            w_state.graph_data.edges.extend(extracted.edges);

            // Update metadata
            w_state.graph_data.metadata.total_nodes = w_state.graph_data.nodes.len();
            w_state.graph_data.metadata.total_edges = w_state.graph_data.edges.len();

            // Rebuild the in-memory petgraph model
            w_state.rebuild_graph()?;

            // Save back to disk
            w_state.save()?;

            Ok(serde_json::json!({
                "status": "success",
                "file_path": params.file_path,
                "total_nodes": w_state.graph_data.metadata.total_nodes,
                "total_edges": w_state.graph_data.metadata.total_edges,
            }))
        }
        _ => anyhow::bail!("Unsupported tool call: {}", name),
    }
}

fn find_node_by_id_or_label(graph_data: &GraphOutput, input: &NodeId) -> Option<NodeId> {
    // 1. Direct match by ID
    for node in &graph_data.nodes {
        if node.id.0 == input.0 {
            return Some(node.id.clone());
        }
    }

    // 2. Case-insensitive case/label matching fallback
    let input_lower = input.0.to_lowercase();
    for node in &graph_data.nodes {
        if node.label.to_lowercase() == input_lower || node.id.0.to_lowercase() == input_lower {
            return Some(node.id.clone());
        }
    }

    None
}
