// ponytail: allow standard clippy lints to keep stdio MCP server implementation extremely simple and robust
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::needless_pass_by_value)]

mod memory_query;
mod plugin_host;
mod types;

use anyhow::{Result, anyhow};
use graphify_core::extract::extract_file;
use graphify_core::graph::build_graph;
use graphify_core::graph::path::find_shortest_path;
use graphify_core::graph::query::query_bfs;
use graphify_core::types::{Edge, GraphMetadata, GraphOutput, Node, NodeId};
use graphify_llm::config::PluginsConfig;
use graphify_plugin_handoff::relay::SaveArgs;
use graphify_plugin_handoff::RelayPlugin;
use graphify_plugin_opendoc::OpendocPlugin;
use memory_query::MemoryQueryService;
use petgraph::graph::{DiGraph, NodeIndex};
use plugin_host::host::PluginHost;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::rc::Rc;
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
        let toon_path = Path::new("graphify-out/graph.toon");
        let fallback_toon_path = Path::new("graph.toon");
        let json_path = Path::new("graphify-out/graph.json");
        let fallback_json_path = Path::new("graph.json");

        let graph_data = if toon_path.exists() {
            let content = fs::read_to_string(toon_path)
                .map_err(|e| anyhow!("Failed to read graphify-out/graph.toon: {}", e))?;
            graphify_core::from_toon(&content)
                .map_err(|e| anyhow!("Failed to parse graphify-out/graph.toon: {}", e))?
        } else if fallback_toon_path.exists() {
            let content = fs::read_to_string(fallback_toon_path)
                .map_err(|e| anyhow!("Failed to read graph.toon: {}", e))?;
            graphify_core::from_toon(&content)
                .map_err(|e| anyhow!("Failed to parse graph.toon: {}", e))?
        } else if json_path.exists() {
            let file = File::open(json_path)
                .map_err(|e| anyhow!("Failed to open graphify-out/graph.json: {}", e))?;
            serde_json::from_reader(file)
                .map_err(|e| anyhow!("Failed to parse graphify-out/graph.json: {}", e))?
        } else if fallback_json_path.exists() {
            let file = File::open(fallback_json_path)
                .map_err(|e| anyhow!("Failed to open graph.json: {}", e))?;
            serde_json::from_reader(file)
                .map_err(|e| anyhow!("Failed to parse graph.json: {}", e))?
        } else {
            return Self::empty();
        };

        let (graph, node_map) = build_graph(&graph_data.nodes, &graph_data.edges)?;

        Ok(Self {
            graph_data,
            graph,
            node_map,
        })
    }

    fn empty() -> Result<Self> {
        let graph_data = GraphOutput {
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
                ..Default::default()
            },
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
        let toon_path = Path::new("graphify-out/graph.toon");
        if let Some(parent) = toon_path.parent() {
            fs::create_dir_all(parent)?;
            let gitignore_path = parent.join(".gitignore");
            if !gitignore_path.exists() {
                fs::write(&gitignore_path, "*\n")?;
            }
        }
        let toon_str = graphify_core::to_toon(&self.graph_data);
        fs::write(toon_path, toon_str)?;
        Ok(())
    }
}

fn main() -> Result<()> {
    eprintln!("graphify-mcp: MCP server starting on stdio");

    // A stale or corrupt graph file must not kill the server at startup;
    // degrade to an empty graph so tools keep responding with honest
    // "node not found" results instead of dropping the MCP connection.
    let state = Arc::new(RwLock::new(match GraphState::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("graphify-mcp: failed to load graph, starting empty: {e}");
            GraphState::empty()?
        }
    }));

    // Plugin declarations are best-effort: a broken config or a failing
    // plugin process must never prevent the MCP server from starting.
    let plugin_host = Rc::new(RefCell::new(match PluginsConfig::load() {
        Ok(config) => PluginHost::scan(&config),
        Err(e) => {
            eprintln!("graphify-mcp: failed to load plugin config, starting without plugins: {e}");
            PluginHost::scan(&PluginsConfig::default())
        }
    }));

    // Core-memory bridge is best-effort too: a missing config falls back to
    // defaults (semantic memory disabled), so graphify_memory_query reports
    // an explicit unavailable status instead of failing at startup.
    let memory_query = Rc::new(RefCell::new(match MemoryQueryService::new() {
        Ok(service) => service,
        Err(e) => {
            eprintln!("graphify-mcp: failed to build memory query service: {e}");
            return Err(e);
        }
    }));

    // Embedded handoff relay plugin: bound once to the server cwd (root
    // walk-up per PROTOCOL.md); relay* tools answer honestly with a NoRoot
    // error when no relay.json exists, mirroring the legacy plugin behavior.
    let relay = Rc::new(RefCell::new(build_relay_plugin()));
    // Embedded opendoc plugin: same bind-once / global-registry pattern as
    // relay; opendoc* tools degrade to empty results when Layer 2 is absent.
    let opendoc = Rc::new(RefCell::new(build_opendoc_plugin()));

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => {
                // MCP spec: notifications (no id) must never receive a
                // response. Skipping them keeps the stdio stream aligned
                // with the client's pending-request bookkeeping.
                let is_notification = request.id.is_none();
                let response = handle_request(
                    request,
                    Arc::clone(&state),
                    Rc::clone(&plugin_host),
                    Rc::clone(&memory_query),
                    Rc::clone(&relay),
                    Rc::clone(&opendoc),
                );
                if is_notification {
                    continue;
                }
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

/// Build the embedded handoff plugin: global registry DB injected so
/// relayClose snapshots land in the same graphify.db as the rest of the
/// toolchain (best-effort; a missing relay.json only affects relay* calls).
fn build_relay_plugin() -> RelayPlugin {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut plugin = RelayPlugin::new().with_registry_path(graphify_registry::registry_db_path());
    plugin.bind_for_cli(&cwd);
    plugin
}

/// Build the embedded opendoc plugin: same registry path pattern as relay
/// (global `graphify.db`). Layer 2 backend defaults to `NoOp` (no dependency on the `OpenDocuments` crate);
/// MCP tools below degrade to empty results when no workspace mapping is set.
fn build_opendoc_plugin() -> OpendocPlugin {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    OpendocPlugin::new()
        .with_registry_path(graphify_registry::registry_db_path())
        .bind_for_cli(&cwd)
}

fn handle_request(
    request: JsonRpcRequest,
    state_lock: Arc<RwLock<GraphState>>,
    plugin_host: Rc<RefCell<PluginHost>>,
    memory_query: Rc<RefCell<MemoryQueryService>>,
    relay: Rc<RefCell<RelayPlugin>>,
    opendoc: Rc<RefCell<OpendocPlugin>>,
) -> JsonRpcResponse {
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
        "tools/list" => {
            let mut tools = serde_json::json!([
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
                },
                {
                    "name": "graphify_notify_plugins",
                    "description": "Manually broadcast a graph_updated notification to all healthy plugin subprocesses (kind: indexed|extracted|manual)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string" }
                        },
                        "required": []
                    }
                },
                {
                    "name": "graphify_memory_query",
                    "description": "Bounded, workspace-scoped semantic query over Graphify core memory (read-only; returns explicit unavailable status when semantic memory is off)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "workspace_key": { "type": "string" },
                            "query": { "type": "string" },
                            "limit": { "type": "integer", "default": 10 }
                        },
                        "required": ["workspace_key", "query"]
                    }
                },
                {
                    "name": "relayInit",
                    "description": "Initialize a relay.json at the current workspace to start cross-session / cross-repo state handoff",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project_context": { "type": "string" },
                            "kind": { "type": "string", "enum": ["backend", "frontend", "infra"] }
                        },
                        "required": ["project_context"]
                    }
                },
                {
                    "name": "relaySave",
                    "description": "Save the current repo's volatile state, phase, confidence and next-step into relay.json and render RESUME.md",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "repo": { "type": "string" },
                            "role": { "type": "string" },
                            "phase": { "type": "string" },
                            "volatile": { "type": "string" },
                            "conf": { "type": "number" },
                            "next": { "type": "string" },
                            "debt": { "type": "string" },
                            "kind": { "type": "string" }
                        },
                        "required": []
                    }
                },
                {
                    "name": "relayClose",
                    "description": "Run the closing ritual: consistency check, spec diff, next_step.md, atomic commit, and a best-effort HandoffSnapshot into the registry",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "repo": { "type": "string" },
                            "next": { "type": "string" }
                        },
                        "required": []
                    }
                },
                {
                    "name": "relaySwitch",
                    "description": "Pass the baton to another registered repo and render its RESUME handover",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "repo": { "type": "string" },
                            "kind": { "type": "string" }
                        },
                        "required": ["repo"]
                    }
                },
                {
                    "name": "relayResume",
                    "description": "Render the RESUME handover for the active (or given) repo — used to bootstrap a new session",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "repo": { "type": "string" },
                            "kind": { "type": "string" }
                        },
                        "required": []
                    }
                },
                {
                    "name": "relayStatus",
                    "description": "Show relay summary: repos, active baton, spec drift, last update",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "relayAdd",
                    "description": "Ingest an existing TODO/handoff doc from an old project into relay.json: stores the raw text and parses each non-empty line into open_threads",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file": { "type": "string" },
                            "repo": { "type": "string" }
                        },
                        "required": ["file"]
                    }
                },
                {
                    "name": "opendocIndex",
                    "description": "Index all `.md` spec blocks in the current workspace: parses markdown, extracts `# Symbol:` annotations, persists spec↔symbol hard links into the opendoc_links SQLite registry (Layer 1, no OpenDocuments dependency)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "doc_paths": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Optional explicit doc paths (relative to workspace root); if omitted, all `.md` files under the root are indexed"
                            }
                        },
                        "required": []
                    }
                },
                {
                    "name": "opendocGetContext",
                    "description": "Given a code symbol (e.g. `crate::auth::verify_token`), return the spec blocks documenting it (Layer 1 hard-link priority; falls back to Layer 2 vector search only when a workspace mapping is set and a backend is injected)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "symbol": { "type": "string" }
                        },
                        "required": ["symbol"]
                    }
                },
                {
                    "name": "opendocAuditDrift",
                    "description": "Audit doc-side drift: for each indexed spec↔symbol link, re-read the source doc, re-parse the block, and compare its signature (sha1) against the indexed one. Returns per-link status: UpToDate / DocChanged / DocMissing",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "required": []
                    }
                }
            ]);
            // A poisoned plugin lock must not hide the built-in tools;
            // degrade to the base list and let the next call retry.
            let host = plugin_host.borrow();
            if let Some(tools) = tools.as_array_mut() {
                tools.extend(host.list_tools());
            }
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::json!({ "tools": tools })),
                error: None,
            }
        }
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

            // This is a built-in gateway tool, not a namespaced plugin tool.
            if tool_name == "graphify_notify_plugins" {
                let kind = tool_arguments
                    .get("kind")
                    .and_then(|value| value.as_str())
                    .unwrap_or("manual");
                if !matches!(kind, "indexed" | "extracted" | "manual") {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "kind must be indexed, extracted, or manual".to_string(),
                        }),
                    };
                }
                let mut host = plugin_host.borrow_mut();
                let workspace_root =
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                let workspace_key = graphify_core::derive_workspace_key(&workspace_root);
                host.broadcast_graph_updated(&serde_json::json!({
                    "kind": kind,
                    "workspace_key": workspace_key,
                }));
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(serde_json::json!({
                        "status": "success",
                        "kind": kind,
                    })),
                    error: None,
                };
            }

            // Restricted core-memory query (Safe Memory Gateway). Read-only,
            // workspace-scoped; explicit unavailable status when memory is off.
            if tool_name == "graphify_memory_query" {
                let service = memory_query.borrow();
                return match service.query(&tool_arguments) {
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
                            message: format!("Memory query error: {e}"),
                        }),
                    },
                };
            }

            // Embedded handoff relay tools: rendered text contract frozen by
            // PROTOCOL.md, errors surface as tool errors (never a panic).
            if matches!(
                tool_name,
                "relayInit" | "relaySave" | "relayClose" | "relaySwitch" | "relayResume"
                    | "relayStatus" | "relayAdd"
            ) {
                let mut relay = relay.borrow_mut();
                return match run_relay_tool(tool_name, &tool_arguments, &mut relay) {
                    Ok(val) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        // MCP spec: tools/call result must be an object with content
                        // blocks. A bare string result is dropped by strict clients
                        // (opencode 1.18.11 times out waiting for the response).
                        result: Some(serde_json::json!({
                            "content": [{ "type": "text", "text": val }]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Relay tool error: {e}"),
                        }),
                    },
                };
            }

            // Embedded opendoc tools: the spec↔code link registry is a pure file/SQLite
            // domain (Layer 1, zero OD dependency); Layer 2 only activates when
            // both a backend and a workspace mapping are configured.
            if matches!(tool_name, "opendocIndex" | "opendocGetContext" | "opendocAuditDrift") {
                let opendoc = opendoc.borrow();
                return match run_opendoc_tool(tool_name, &tool_arguments, &opendoc) {
                    Ok(val) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        // MCP spec: tools/call result must be an object with content
                        // blocks (the relay* fix from 351d366 carries over here).
                        result: Some(serde_json::json!({
                            "content": [{ "type": "text", "text": val }]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Opendoc tool error: {e}"),
                        }),
                    },
                };
            }

            // Route graphify_plugin_* tools to the plugin host before the
            // built-in tool matcher, so plugin tools can never shadow core tools.
            if tool_name.starts_with("graphify_plugin_") {
                let mut host = plugin_host.borrow_mut();
                return match host.call_tool(tool_name, &tool_arguments) {
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
                            message: format!("Plugin tool call error: {e}"),
                        }),
                    },
                };
            }

            match handle_tool_call(tool_name, tool_arguments, state_lock) {
                Ok(val) => {
                    // After a successful reindex the graph changed: notify
                    // every ready plugin subprocess (design D5). The payload
                    // carries the workspace_key routing key so plugins can
                    // correlate the update with the workspace they were bound
                    // to; failures are isolated inside the host.
                    if tool_name == "graph_reindex" {
                        let mut host = plugin_host.borrow_mut();
                        let workspace_key = graphify_core::derive_workspace_key(
                            std::env::current_dir().unwrap_or_default(),
                        );
                        host.broadcast_graph_updated(&serde_json::json!({
                            "kind": "indexed",
                            "workspace_key": workspace_key,
                        }));
                    }
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(val),
                        error: None,
                    }
                }
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

/// Dispatch one relay* tool to the embedded handoff plugin. Returns the
/// frozen rendered text as a JSON string.
fn run_relay_tool(
    name: &str,
    args: &serde_json::Value,
    relay: &mut RelayPlugin,
) -> Result<serde_json::Value> {
    let get_str = |key: &str| args.get(key).and_then(|v| v.as_str());
    let out = match name {
        "relayInit" => {
            let project = get_str("project_context")
                .ok_or_else(|| anyhow!("Missing 'project_context'"))?;
            relay.relay_init(project, get_str("kind"))?
        }
        "relaySave" => relay.relay_save(SaveArgs {
            repo: get_str("repo"),
            role: get_str("role"),
            active_phase: get_str("phase"),
            volatile_state: get_str("volatile"),
            confidence: args.get("conf").and_then(serde_json::Value::as_f64),
            next_session_starter: get_str("next"),
            debt_tag: get_str("debt"),
            kind: get_str("kind"),
        })?,
        "relayClose" => relay.relay_close(get_str("repo"), get_str("next"))?,
        "relaySwitch" => {
            let repo = get_str("repo").ok_or_else(|| anyhow!("Missing 'repo'"))?;
            relay.relay_switch(repo, get_str("kind"))?
        }
        "relayResume" => relay.relay_resume(get_str("repo"), get_str("kind"))?,
        "relayStatus" => relay.relay_status()?,
        "relayAdd" => {
            let file = get_str("file").ok_or_else(|| anyhow!("Missing 'file'"))?;
            relay.relay_add(Path::new(file), get_str("repo"))?
        }
        _ => anyhow::bail!("Unsupported relay tool: {name}"),
    };
    Ok(serde_json::Value::String(out))
}

/// Dispatch one `opendoc*` tool to the embedded opendoc plugin. Returns a
/// human-readable text rendering (same contract as the relay* tools).
fn run_opendoc_tool(
    name: &str,
    args: &serde_json::Value,
    opendoc: &OpendocPlugin,
) -> Result<String> {
    use std::fmt::Write as _;
    let get_str = |key: &str| args.get(key).and_then(|v| v.as_str());
    match name {
        "opendocIndex" => {
            let doc_paths: Vec<String> = args
                .get("doc_paths")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let n = if doc_paths.is_empty() {
                opendoc
                    .index_all_docs()
                    .map_err(|e| anyhow!("opendoc index_all_docs: {e}"))?
            } else {
                opendoc
                    .index_doc_paths(&doc_paths)
                    .map_err(|e| anyhow!("opendoc index_doc_paths: {e}"))?
            };
            Ok(format!("[opendoc] indexed: {n} link rows"))
        }
        "opendocGetContext" => {
            let symbol = get_str("symbol")
                .ok_or_else(|| anyhow!("Missing 'symbol'"))?
                .to_string();
            let rows = opendoc
                .fetch_code_to_doc_context(&symbol)
                .map_err(|e| anyhow!("opendoc fetch_code_to_doc_context: {e}"))?;
            if rows.is_empty() {
                Ok(format!("[opendoc] {symbol}: no spec coverage"))
            } else {
                let mut out = format!("[opendoc] {symbol} — {} spec block(s):\n", rows.len());
                for r in &rows {
                    let _ = writeln!(
                        out,
                        "  {}\t{}\t{}",
                        r.spec_id, r.doc_path, r.symbol
                    );
                }
                Ok(out)
            }
        }
        "opendocAuditDrift" => {
            let items = opendoc
                .audit_drift()
                .map_err(|e| anyhow!("opendoc audit_drift: {e}"))?;
            if items.is_empty() {
                Ok("[opendoc] no indexed links to audit".to_string())
            } else {
                let mut out = format!("[opendoc] {} drift item(s):\n", items.len());
                for item in &items {
                    let _ = writeln!(
                        out,
                        "  {}\t{}\t{}\t{:?}",
                        item.spec_id, item.symbol, item.doc_path, item.status
                    );
                }
                Ok(out)
            }
        }
        _ => anyhow::bail!("Unsupported opendoc tool: {name}"),
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
            let src_resolved =
                find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.source.clone()))
                    .ok_or_else(|| anyhow!("Source node not found: {}", params.source))?;
            let tgt_resolved =
                find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.target.clone()))
                    .ok_or_else(|| anyhow!("Target node not found: {}", params.target))?;

            let path_opt = find_shortest_path(
                &r_state.graph,
                &r_state.node_map,
                &src_resolved,
                &tgt_resolved,
            )?;
            Ok(serde_json::to_value(path_opt)?)
        }
        "graph_summary" => {
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            // Return top-level module topology, core structs and classes
            let mut summary_nodes = Vec::new();
            for node in &r_state.graph_data.nodes {
                if node.kind == "module"
                    || node.kind == "class"
                    || node.kind == "struct"
                    || node.kind == "trait"
                    || node.kind == "interface"
                {
                    summary_nodes.push(node.clone());
                }
            }
            // Keep summary lightweight
            summary_nodes.truncate(15);

            let mut summary_edges = Vec::new();
            let summary_node_ids: std::collections::HashSet<&NodeId> =
                summary_nodes.iter().map(|n| &n.id).collect();
            for edge in &r_state.graph_data.edges {
                if summary_node_ids.contains(&edge.source)
                    && summary_node_ids.contains(&edge.target)
                {
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
            let src_resolved =
                find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.from.clone()))
                    .ok_or_else(|| anyhow!("Source node not found: {}", params.from))?;
            let tgt_resolved =
                find_node_by_id_or_label(&r_state.graph_data, &NodeId(params.to.clone()))
                    .ok_or_else(|| anyhow!("Target node not found: {}", params.to))?;

            let path_opt = find_shortest_path(
                &r_state.graph,
                &r_state.node_map,
                &src_resolved,
                &tgt_resolved,
            )?;
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
            w_state
                .graph_data
                .nodes
                .retain(|n| n.source_file != params.file_path);
            w_state
                .graph_data
                .edges
                .retain(|e| e.source_file != params.file_path);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_failure_falls_back_to_empty_graph() -> Result<()> {
        // A corrupt graph file must not kill the server: load() reports the
        // error, and empty() still produces a usable, queryable state.
        let dir = std::env::temp_dir().join(format!("graphify-mcp-test-{}", std::process::id()));
        let out = dir.join("graphify-out");
        fs::create_dir_all(&out)?;
        let mut f = fs::File::create(out.join("graph.json"))?;
        f.write_all(br#"{"edges":[{"kind":"contains"}]}"#)?;

        let cwd = std::env::current_dir()?;
        std::env::set_current_dir(&dir)?;
        let load_result = GraphState::load();
        let empty = GraphState::empty()?;
        std::env::set_current_dir(cwd)?;
        fs::remove_dir_all(&dir)?;

        assert!(
            load_result.is_err(),
            "corrupt graph file must surface a load error"
        );
        assert_eq!(empty.graph_data.nodes.len(), 0);
        assert_eq!(empty.graph_data.edges.len(), 0);
        assert!(find_node_by_id_or_label(&empty.graph_data, &NodeId("x".into())).is_none());
        Ok(())
    }

    #[test]
    fn test_relay_tools_full_cycle() -> Result<()> {
        // Full relay lifecycle through the MCP dispatch: init -> save ->
        // status -> close. Registry path is injected to a temp file so the
        // close snapshot stays hermetic.
        let dir = std::env::temp_dir().join(format!(
            "graphify-mcp-relay-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir)?;
        let cwd = std::env::current_dir()?;
        std::env::set_current_dir(&dir)?;

        let mut plugin = RelayPlugin::new().with_registry_path(dir.join("graphify-test.db"));
        plugin.bind_for_cli(&dir);
        let empty_args = serde_json::json!({});

        let init_out = run_relay_tool(
            "relayInit",
            &serde_json::json!({ "project_context": "test project" }),
            &mut plugin,
        )?;
        assert!(init_out.as_str().is_some_and(|s| s.contains("Initialized relay at")));

        let save_out = run_relay_tool(
            "relaySave",
            &serde_json::json!({
                "repo": "graphify-mcp",
                "phase": "testing",
                "conf": 0.8,
                "next": "run smoke test"
            }),
            &mut plugin,
        )?;
        assert!(save_out.as_str().is_some_and(|s| s.contains("graphify-mcp")));

        let status_out = run_relay_tool("relayStatus", &empty_args, &mut plugin)?;
        assert!(status_out.as_str().is_some_and(|s| s.contains("graphify-mcp")));

        let close_out = run_relay_tool(
            "relayClose",
            &serde_json::json!({ "repo": "graphify-mcp", "next": "done" }),
            &mut plugin,
        )?;
        assert!(close_out.as_str().is_some_and(|s| s.contains("Consistency: OK")));

        // relayStatus after close must not error (baton was on graphify-mcp).
        assert!(run_relay_tool("relayStatus", &empty_args, &mut plugin)?.as_str().is_some());

        std::env::set_current_dir(cwd)?;
        fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
