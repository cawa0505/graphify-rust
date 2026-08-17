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
use graphify_core::plugin::{GraphifyPlugin, derive_workspace_key};
use graphify_core::types::{Edge, GraphMetadata, GraphOutput, Node, NodeId};
use graphify_llm::config::PluginsConfig;
use graphify_plugin_handoff::relay::SaveArgs;
use std::fmt::Write as _;
use graphify_plugin_handoff::RelayPlugin;
use graphify_plugin_opendoc::OpendocPlugin;
use graphify_plugin_review::ReviewPlugin;
use graphify_plugin_telemetry::TelemetryPlugin;
use graphify_plugin_test_coverage::CoveragePlugin;
use memory_query::MemoryQueryService;
use petgraph::graph::{DiGraph, NodeIndex};
use plugin_host::host::PluginHost;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};
use types::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, PathParams, QueryNodeParams, QueryParams,
    ReindexParams, TracePathParams,
};

/// Helper to create a standard MCP tool registration JSON object.
/// Reduces boilerplate in the tools/list handler.
fn register_tool(name: &str, desc: &str, schema: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "description": desc,
        "inputSchema": schema,
    })
}

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
    //
    // The circuit breaker seeds quarantined plugins from the global registry
    // db. If the db is unavailable the server cannot start — the XDG data
    // directory must be writable.
    let registry_db = graphify_registry::RegistryDb::open(&graphify_registry::registry_db_path())?;
    let workspace_key = std::env::current_dir()
        .ok()
        .map_or_else(|| "default".to_string(), |p| derive_workspace_key(&p));
    let plugin_host = Rc::new(RefCell::new(match PluginsConfig::load() {
        Ok(config) => PluginHost::scan(&config, &registry_db, &workspace_key),
        Err(e) => {
            eprintln!("graphify-mcp: failed to load plugin config, starting without plugins: {e}");
            PluginHost::scan(&PluginsConfig::default(), &registry_db, &workspace_key)
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
    // Embedded review plugin: code-review-graph bridge (file-based ingest,
    // line→symbol binding in graphify.db); review* tools are self-contained.
    // Slice 2: notify buffer 收集 ImpactAlert，response 寫完後以
    // notifications/review/impact_alert 轉發給 client（T2.3）。
    let review_notify: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let review = Rc::new(RefCell::new(build_review_plugin(Arc::clone(&review_notify))));
    // Embedded telemetry plugin: Draco Telemetry bridge (file-based ingest,
    // hotspot threshold in graphify.db); telemetry* tools are self-contained.
    let telemetry = Rc::new(RefCell::new(build_telemetry_plugin()));
    // Embedded coverage plugin: test coverage bridge (LCOV/JSON ingest,
    // line→symbol binding in graphify.db); coverage* tools are self-contained.
    let coverage = Rc::new(RefCell::new(build_coverage_plugin()));

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
                    Rc::clone(&review),
                    Rc::clone(&telemetry),
                    Rc::clone(&coverage),
                );
                if is_notification {
                    continue;
                }
                let response_json = serde_json::to_string(&response)?;
                writeln!(stdout, "{response_json}")?;
                stdout.flush()?;
                // Slice 2 T2.3：response 後再轉發 ImpactAlert notifications
                // （先回應後通知，避免與 pending-request bookkeeping 交錯）。
                if let Ok(mut buf) = review_notify.lock() {
                    for alert in buf.drain(..) {
                        let notif = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/review/impact_alert",
                            "params": alert,
                        });
                        writeln!(stdout, "{notif}")?;
                    }
                    stdout.flush()?;
                }
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
    let mut p = OpendocPlugin::new()
        .with_registry_path(graphify_registry::registry_db_path());
    // Layer 2：讀 `OD_BASE_URL` env，設定且非空時注入 RestBackend 直連 OD。
    if let Some(url) = std::env::var("OD_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        p = p.with_backend(Box::new(
            graphify_plugin_opendoc::RestBackend::new(&url),
        ));
    }
    p.bind_for_cli(&cwd)
}

fn build_review_plugin(notify: Arc<Mutex<Vec<serde_json::Value>>>) -> ReviewPlugin {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut p = ReviewPlugin::new().with_registry_path(graphify_registry::registry_db_path());
    // v1.1：注入 notify callback（host 控制轉發，Dependency Inversion）。
    // Slice 2 T2.3：plugin emit 的 ImpactAlert 先進 buffer，response 寫完後
    // 由 main loop drain 成 MCP notifications/review/impact_alert 轉發。
    p.set_notify_callback(Some(Box::new(move |payload| {
        if let Ok(mut buf) = notify.lock() {
            buf.push(payload);
        }
    })));
    // Slice 1/2：CRG_MCP_URL 設定時注入即時分析 client（目前保留骨架）。
    p.bind_for_cli(&cwd)
}

/// Build the embedded telemetry plugin: same global-registry pattern as
/// review (`telemetry_bindings` 在 graphify.db）。`DRACO_BASE_URL` 設定時由
/// `draco_client` 主動輪詢 Draco MCP（`source="draco-mcp"`）；否則走檔案型
/// ingest（`source="file"`）。
fn build_telemetry_plugin() -> TelemetryPlugin {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let p = TelemetryPlugin::new().with_registry_path(graphify_registry::registry_db_path());
    p.bind_for_cli(&cwd)
}

/// Build the embedded coverage plugin: same global-registry pattern as
/// telemetry (`coverage_bindings` 在 graphify.db）。LCOV/JSON 文字 ingest，
/// 走 line→symbol 解析綁定到 canonical node id。
fn build_coverage_plugin() -> CoveragePlugin {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let p = CoveragePlugin::new().with_registry_path(graphify_registry::registry_db_path());
    p.bind_for_cli(&cwd)
}

#[allow(clippy::too_many_arguments)] // dispatch hub: 每個 plugin 一個 Rc，新增 plugin 即加一參數
fn handle_request(
    request: JsonRpcRequest,
    state_lock: Arc<RwLock<GraphState>>,
    plugin_host: Rc<RefCell<PluginHost>>,
    memory_query: Rc<RefCell<MemoryQueryService>>,
    relay: Rc<RefCell<RelayPlugin>>,
    opendoc: Rc<RefCell<OpendocPlugin>>,
    review: Rc<RefCell<ReviewPlugin>>,
    telemetry: Rc<RefCell<TelemetryPlugin>>,
    coverage: Rc<RefCell<CoveragePlugin>>,
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
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
            error: None,
        },
        "tools/list" => {
            let mut tools = serde_json::json!([
                register_tool("graphify_help", "List all available tools with descriptions", serde_json::json!({
                    "type": "object",
                    "properties": {}
                })),
                register_tool("graphify_graph_query", "BFS traversal of the knowledge graph (legacy compatibility)", serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" }
                    },
                    "required": ["question"]
                })),
                {
                    "name": "graphify_graph_path",
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
                    "name": "graphify_graph_summary",
                    "description": "Get high-level topology summary",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "graphify_graph_query_node",
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
                    "name": "graphify_graph_trace_path",
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
                    "name": "graphify_graph_reindex",
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
                    "name": "graphify_plugin_notify",
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
                            "workspace_key": { "type": "string", "description": "Workspace key (auto-detected from current directory when omitted)" },
                            "query": { "type": "string" },
                            "limit": { "type": "integer", "default": 10 }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "graphify_relay_init",
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
                    "name": "graphify_relay_save",
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
                    "name": "graphify_relay_close",
                    "description": "Auto-save state and run the closing ritual: consistency check, spec diff, next_step.md, atomic commit, and a best-effort HandoffSnapshot into the registry. Accepts all relay_save params for one-shot close.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "repo": { "type": "string", "description": "Repo name (default: cwd basename)" },
                            "next": { "type": "string", "description": "Next session starter text" },
                            "role": { "type": "string", "description": "Role (e.g. backend/frontend/infra)" },
                            "phase": { "type": "string", "description": "Active phase" },
                            "volatile": { "type": "string", "description": "Volatile state summary" },
                            "conf": { "type": "number", "description": "Confidence score (1-5)" },
                            "debt": { "type": "string", "description": "Comma-separated debt tags" },
                            "kind": { "type": "string", "description": "Template kind (backend/frontend/infra)" }
                        },
                        "required": []
                    }
                },
                {
                    "name": "graphify_relay_switch",
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
                    "name": "graphify_relay_resume",
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
                    "name": "graphify_relay_status",
                    "description": "Show relay summary: repos, active baton, spec drift, last update",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "graphify_relay_add",
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
                    "name": "graphify_opendoc_index",
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
                    "name": "graphify_opendoc_get_context",
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
                    "name": "graphify_opendoc_audit_drift",
                    "description": "Audit doc-side drift: for each indexed spec↔symbol link, re-read the source doc, re-parse the block, and compare its signature (sha1) against the indexed one. Returns per-link status: UpToDate / DocChanged / DocMissing",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "required": []
                    }
                },
                {
                    "name": "graphify_review_ingest",
                    "description": "Import a CRG IngestPayload JSON file into the review_bindings registry: each review point is line→symbol resolved against the cached GraphOutput (Slice 0 file-based import, no CRG dependency)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "payload": { "type": "string", "description": "Path to the IngestPayload JSON file" }
                        },
                        "required": ["payload"]
                    }
                },
                {
                    "name": "graphify_review_get_context",
                    "description": "Query unresolved reviews bound to a canonical node id (e.g. `src/auth.rs:function:verify_token`)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "node": { "type": "string", "description": "canonical node id (assigned at ingest time)" }
                        },
                        "required": ["node"]
                    }
                },
                {
                    "name": "graphify_review_resolve",
                    "description": "Mark a review as resolved by its review id",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "review_id": { "type": "string" },
                            "reason": { "type": "string" }
                        },
                        "required": ["review_id"]
                    }
                },
                {
                    "name": "graphify_review_search_crg",
                    "description": "Call CRG detect_changes_tool (CRG_BASE_URL) and bind its top-risk changed functions as review points (line→symbol via cached GraphOutput). Optional `base` git ref (default HEAD~1) widens the diff window.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "base": { "type": "string", "description": "git diff base ref (default HEAD~1)" }
                        },
                        "required": []
                    }
                },
                {
                    "name": "graphify_telemetry_ingest",
                    "description": "Import telemetry metrics into the telemetry_bindings registry: each metric is line→symbol (or symbol→node, for Draco) resolved against the cached GraphOutput and flagged is_hotspot when p99 > 500ms or alloc > 5MB (dynamic thresholds, env TELEMETRY_HOTSPOT_P99_MS / TELEMETRY_HOTSPOT_ALLOC_BYTES). source=\"file\" 讀本地 IngestPayload JSON；source=\"draco-mcp\" 主動輪詢 Draco fetch_top_hotspots()（Top 10）",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "source": { "type": "string", "description": "\"file\" 或 \"draco-mcp\"（即時輪詢）" },
                            "path_or_draco_params": { "type": "string", "description": "source=\"file\" 時為 IngestPayload JSON 檔路徑；source=\"draco-mcp\" 時可省略" }
                        },
                        "required": ["source"]
                    }
                },
                {
                    "name": "graphify_telemetry_get_context",
                    "description": "Query telemetry bindings for a canonical node id (e.g. `src/db/query.rs:function:query_users`): p99 latency / alloc / call rate / hotspot flag; include_impact_radius 於 Slice 2 展開 Upstream callers BFS",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "node": { "type": "string", "description": "canonical node id" },
                            "include_impact_radius": { "type": "boolean", "description": "Slice 2: 含 Upstream callers 衝擊半徑" }
                        },
                        "required": ["node"]
                    }
                },
                {
                    "name": "graphify_coverage_ingest",
                    "description": "測試覆蓋率資料匯入：LCOV 文字或 cobertura JSON。line→symbol 綁定後存入 coverage_bindings（graphify.db）；每次 ingest 以快照取代舊資料。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "format": { "type": "string", "enum": ["lcov", "json"], "description": "輸入格式" },
                            "data": { "type": "string", "description": "LCOV 或 cobertura JSON 文字內容" }
                        },
                        "required": ["format", "data"]
                    }
                },
                {
                    "name": "graphify_coverage_get_context",
                    "description": "查詢某個 canonical node id 的覆蓋率綁定（covered_lines / total_lines / line_rate）",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "node": { "type": "string", "description": "canonical node id（如 `src/a.rs:function:f`）" }
                        },
                        "required": ["node"]
                    }
                },
                {
                    "name": "graphify_coverage_blindspots",
                    "description": "列出所有覆蓋率 < 50% 的盲區節點",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
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

            // Built-in help tool: returns a formatted listing of all tools.
            if tool_name == "graphify_help" {
                let host = plugin_host.borrow();
                let plugin_tools = host.list_tools();
                drop(host);
                let mut builtin = vec![
                    ("graphify_help", "List all available tools with descriptions"),
                    ("graphify_graph_query", "BFS traversal of the knowledge graph (legacy compatibility)"),
                    ("graphify_graph_path", "Find shortest path between two nodes (legacy compatibility)"),
                    ("graphify_graph_summary", "Get high-level topology summary"),
                    ("graphify_graph_query_node", "Query nodes by ID with depth"),
                    ("graphify_graph_trace_path", "Find shortest path between two nodes"),
                    ("graphify_graph_reindex", "Reindex a file into the graph"),
                    ("graphify_plugin_notify", "Manually broadcast a graph_updated notification"),
                    ("graphify_memory_query", "Semantic memory query over the knowledge graph"),
                    ("graphify_relay_init", "Initialize relay.json for cross-session handoff"),
                    ("graphify_relay_save", "Save session state to relay.json"),
                    ("graphify_relay_close", "Close relay session"),
                    ("graphify_relay_switch", "Switch to another registered repo"),
                    ("graphify_relay_resume", "Resume a relay session"),
                    ("graphify_relay_status", "Show relay summary"),
                    ("graphify_relay_add", "Ingest TODO/handoff doc into relay.json"),
                    ("graphify_opendoc_index", "Index spec blocks in workspace"),
                    ("graphify_opendoc_get_context", "Get spec blocks for a code symbol"),
                    ("graphify_opendoc_audit_drift", "Audit doc-side drift"),
                    ("graphify_review_ingest", "Import CRG review payload"),
                    ("graphify_review_get_context", "Query unresolved reviews"),
                    ("graphify_review_resolve", "Mark a review as resolved"),
                    ("graphify_review_search_crg", "Search CRG for changed functions"),
                    ("graphify_telemetry_ingest", "Import telemetry metrics"),
                    ("graphify_telemetry_get_context", "Query telemetry bindings"),
                    ("graphify_coverage_ingest", "Import test coverage data"),
                    ("graphify_coverage_get_context", "Query coverage for a node"),
                    ("graphify_coverage_blindspots", "List nodes with <50% coverage"),
                ];
                builtin.sort_by(|a, b| a.0.cmp(b.0));
                let mut text = String::from("## Graphify MCP Tools\n\n");
                for (name, desc) in &builtin {
                    let _ = writeln!(text, "- **`{name}`**: {desc}");
                }
                // Plugin tools from the host (returns Vec<Value>)
                if !plugin_tools.is_empty() {
                    text.push_str("\n### Plugin Tools\n\n");
                    for t in &plugin_tools {
                        let n = t.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let d = t.get("description").and_then(|v| v.as_str()).unwrap_or("");
                        let _ = writeln!(text, "- **`{n}`**: {d}");
                    }
                }
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(serde_json::json!({
                        "content": [{ "type": "text", "text": text }]
                    })),
                    error: None,
                };
            }

            // This is a built-in gateway tool, not a namespaced plugin tool.
            if tool_name == "graphify_plugin_notify" {
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
                drop(host);
                // 內嵌 review plugin：同樣餵入新圖並觸發 drift 自動銷案
                // （broadcast 只涵蓋 host 外部程序，內嵌 Rc 需手動）。
                if let Ok(state) = state_lock.read() {
                    let toon_str = graphify_core::to_toon(&state.graph_data);
                    drop(state);
                    let mut r = review.borrow_mut();
                    r.sync_toon(Some(toon_str.into_bytes()));
                    let event = graphify_core::GraphUpdateEvent::new(
                        &workspace_key,
                        Vec::new(),
                        graphify_core::GraphUpdateKind::Manual,
                    );
                    r.on_graph_updated(&event);
                }
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
                "graphify_relay_init" | "graphify_relay_save" | "graphify_relay_close" | "graphify_relay_switch" | "graphify_relay_resume"
                    | "graphify_relay_status" | "graphify_relay_add"
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
            if matches!(tool_name, "graphify_opendoc_index" | "graphify_opendoc_get_context" | "graphify_opendoc_audit_drift") {
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

            // Embedded review tools: code-review-graph bridge — file-based
            // ingest resolves line→symbol against the cached GraphOutput
            // (sync_toon must have run first); queries return bindings in
            // graphify.db. Slice 0 is self-contained (no CRG HTTP dependency).
            //
            // Host responsibility (reviewIngest)：將已索引的 GraphOutput
            // 經由 sync_toon 傳入 plugin 記憶體快取，再做 line→symbol
            // 解析；保證 line 升維至 canonical NodeId 時有圖譜可對齊。
            if matches!(
                tool_name,
                "graphify_review_ingest" | "graphify_review_get_context" | "graphify_review_resolve" | "graphify_review_search_crg"
            ) {
                // 所有 review 工具都需要 graph 快取做 line→symbol 升維；
                // 餵 graph 後再進 dispatch（reviewIngest 之外的工具也需圖譜）。
                let toon_str = {
                    let state = match state_lock.read() {
                        Ok(s) => s,
                        Err(_) => {
                            return JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id,
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32603,
                                    message: "graph state lock poisoned".to_string(),
                                }),
                            };
                        }
                    };
                    graphify_core::to_toon(&state.graph_data)
                };
                review.borrow_mut().sync_toon(Some(toon_str.into_bytes()));
                let mut review = review.borrow_mut();
                return match run_review_tool(tool_name, &tool_arguments, &mut review) {
                    Ok(val) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
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
                            message: format!("Review tool error: {e}"),
                        }),
                    },
                };
            }

            // Embedded telemetry tools: Draco Telemetry bridge — same
            // host-responsibility pattern as reviewIngest：telemetryIngest
            // 前先經由 sync_toon 餵入已索引的 GraphOutput，line→symbol
            // 升維時才有圖譜可對齊。source="file" 走路徑；source="draco-mcp"
            // 走 Draco 輪詢（在 run_telemetry_tool 分派）。
            if matches!(tool_name, "graphify_telemetry_ingest" | "graphify_telemetry_get_context") {
                if tool_name == "graphify_telemetry_ingest" {
                    let state = match state_lock.read() {
                        Ok(s) => s,
                        Err(_) => {
                            return JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id,
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32603,
                                    message: "graph state lock poisoned".to_string(),
                                }),
                            };
                        }
                    };
                    let toon_str = graphify_core::to_toon(&state.graph_data);
                    drop(state);
                    telemetry
                        .borrow_mut()
                        .sync_toon(Some(toon_str.into_bytes()));
                }
                let telemetry = telemetry.borrow();
                return match run_telemetry_tool(tool_name, &tool_arguments, &telemetry) {
                    Ok(val) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
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
                            message: format!("Telemetry tool error: {e}"),
                        }),
                    },
                };
            }

            // Embedded coverage tools: test coverage bridge — LCOV/JSON
            // ingest resolves line→symbol against the cached GraphOutput;
            // coverageIngest 前先餵 graph 讓 line→symbol 升維有圖譜可對齊。
            if matches!(
                tool_name,
                "graphify_coverage_ingest" | "graphify_coverage_get_context" | "graphify_coverage_blindspots"
            ) {
                if tool_name == "graphify_coverage_ingest" {
                    let state = match state_lock.read() {
                        Ok(s) => s,
                        Err(_) => {
                            return JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id,
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32603,
                                    message: "graph state lock poisoned".to_string(),
                                }),
                            };
                        }
                    };
                    let toon_str = graphify_core::to_toon(&state.graph_data);
                    drop(state);
                    coverage
                        .borrow_mut()
                        .sync_toon(Some(toon_str.into_bytes()));
                }
                let coverage = coverage.borrow();
                return match run_coverage_tool(tool_name, &tool_arguments, &coverage) {
                    Ok(val) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
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
                            message: format!("Coverage tool error: {e}"),
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

            match handle_tool_call(tool_name, tool_arguments, state_lock.clone()) {
                Ok(val) => {
                    // After a successful reindex the graph changed: notify
                    // every ready plugin subprocess (design D5). The payload
                    // carries the workspace_key routing key so plugins can
                    // correlate the update with the workspace they were bound
                    // to; failures are isolated inside the host.
                    if tool_name == "graphify_graph_reindex" {
                        let workspace_key = graphify_core::derive_workspace_key(
                            std::env::current_dir().unwrap_or_default(),
                        );
                        // 內嵌 review plugin 不是 host 外部程序：直接在
                        // reindex 成功後餵入新圖並觸發 drift 自動銷案。
                        if let Ok(state) = state_lock.read() {
                            let toon_str = graphify_core::to_toon(&state.graph_data);
                            drop(state);
                            let mut r = review.borrow_mut();
                            r.sync_toon(Some(toon_str.into_bytes()));
                            let event = graphify_core::GraphUpdateEvent::new(
                                &workspace_key,
                                Vec::new(),
                                graphify_core::GraphUpdateKind::Indexed,
                            );
                            r.on_graph_updated(&event);
                        }
                        let mut host = plugin_host.borrow_mut();
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
        "graphify_relay_init" => {
            let project = get_str("project_context")
                .ok_or_else(|| anyhow!("Missing 'project_context'"))?;
            relay.relay_init(project, get_str("kind"))?
        }
        "graphify_relay_save" => relay.relay_save(SaveArgs {
            repo: get_str("repo"),
            role: get_str("role"),
            active_phase: get_str("phase"),
            volatile_state: get_str("volatile"),
            confidence: args.get("conf").and_then(serde_json::Value::as_f64),
            next_session_starter: get_str("next"),
            debt_tag: get_str("debt"),
            kind: get_str("kind"),
        })?,
        "graphify_relay_close" => {
            let repo = get_str("repo");
            // Auto-save before close if any state params are provided
            let role = get_str("role");
            let phase = get_str("phase");
            let volatile = get_str("volatile");
            let confidence = args.get("conf").and_then(serde_json::Value::as_f64);
            let debt = get_str("debt");
            let kind = get_str("kind");
            let next = get_str("next");
            if role.is_some()
                || phase.is_some()
                || volatile.is_some()
                || confidence.is_some()
                || debt.is_some()
                || kind.is_some()
            {
                relay.relay_save(SaveArgs {
                    repo,
                    role,
                    active_phase: phase,
                    volatile_state: volatile,
                    confidence,
                    next_session_starter: next,
                    debt_tag: debt,
                    kind,
                })?;
            }
            relay.relay_close(repo, next)?
        }
        "graphify_relay_switch" => {
            let repo = get_str("repo").ok_or_else(|| anyhow!("Missing 'repo'"))?;
            relay.relay_switch(repo, get_str("kind"))?
        }
        "graphify_relay_resume" => relay.relay_resume(get_str("repo"), get_str("kind"))?,
        "graphify_relay_status" => relay.relay_status()?,
        "graphify_relay_add" => {
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
        "graphify_opendoc_index" => {
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
        "graphify_opendoc_get_context" => {
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
        "graphify_opendoc_audit_drift" => {
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

/// Dispatch one `review*` tool to the embedded review plugin. The host must
/// have fed the current `GraphOutput` into the plugin via `sync_toon` before
/// calling ingest (line→symbol resolution requires a cached graph).
fn run_review_tool(
    name: &str,
    args: &serde_json::Value,
    review: &mut ReviewPlugin,
) -> Result<String> {
    use std::fmt::Write as _;
    let get_str = |key: &str| args.get(key).and_then(|v| v.as_str());
    let wk = review.get_workspace_key().to_string();
    match name {
        "graphify_review_ingest" => {
            let path = get_str("payload").ok_or_else(|| anyhow!("Missing 'payload'"))?;
            let (bound, orphan) = review
                .review_ingest_file(Path::new(path))
                .map_err(|e| anyhow!("review ingest_file: {e}"))?;
            Ok(format!("[review] {path}: {bound} bound, {orphan} orphan lines"))
        }
        "graphify_review_get_context" => {
            let node = get_str("node").ok_or_else(|| anyhow!("Missing 'node'"))?;
            let (_node_id, rows) = review
                .review_get_context(&wk, node, false)
                .map_err(|e| anyhow!("review get_context: {e}"))?;
            if rows.is_empty() {
                Ok(format!("[review] {node}: no unresolved reviews"))
            } else {
                let mut out =
                    format!("[review] {node} — {} unresolved review(s):\n", rows.len());
                for r in &rows {
                    let _ = writeln!(
                        out,
                        "  {}\t{}\t{}\t{}",
                        r.id, r.severity, r.category, r.comment
                    );
                }
                Ok(out)
            }
        }
        "graphify_review_resolve" => {
            let rid = get_str("review_id").ok_or_else(|| anyhow!("Missing 'review_id'"))?;
            let reason = get_str("reason").unwrap_or_default();
            let updated = review
                .review_resolve(&wk, rid, "manual", reason)
                .map_err(|e| anyhow!("review resolve: {e}"))?;
            if updated {
                Ok(format!("[review] {rid}: resolved"))
            } else {
                Ok(format!("[review] {rid}: not found"))
            }
        }
        "graphify_review_search_crg" => {
            let base = get_str("base");
            let (node_ids, orphan) = review
                .review_search_crg(base)
                .map_err(|e| anyhow!("review search_crg: {e}"))?;
            let mut out = format!(
                "[review] search-crg: {} bound, {orphan} orphan",
                node_ids.len()
            );
            for id in &node_ids {
                out = format!("{out}\n  - {id}");
            }
            Ok(out)
        }
        _ => anyhow::bail!("Unsupported review tool: {name}"),
    }
}

/// Dispatch one `telemetry*` tool to the embedded telemetry plugin.
/// Returns a text result; errors are mapped to JSON-RPC error responses.
fn run_telemetry_tool(
    name: &str,
    args: &serde_json::Value,
    telemetry: &TelemetryPlugin,
) -> Result<String> {
    use std::fmt::Write as _;
    let get_str = |key: &str| args.get(key).and_then(|v| v.as_str());
    let wk = telemetry.get_workspace_key().to_string();
    match name {
        "graphify_telemetry_ingest" => {
            let source = get_str("source").ok_or_else(|| anyhow!("Missing 'source'"))?;
            match source {
                "file" => {
                    let path = get_str("path_or_draco_params")
                        .ok_or_else(|| anyhow!("Missing 'path_or_draco_params'"))?;
                    let report = telemetry
                        .telemetry_ingest_file(Path::new(path))
                        .map_err(|e| anyhow!("telemetry ingest_file: {e}"))?;
                    Ok(format!(
                        "[telemetry] {path}: {} metrics, {} bound, {} orphan, {} hotspot(s)",
                        report.total, report.bound, report.orphan, report.hotspots
                    ))
                }
                // Slice 1：一鍵同步 Draco Top 10 熱點（server-side 聚合，
                // 走同一條 ingest 管線）。
                "draco-mcp" => {
                    let report = telemetry
                        .telemetry_ingest_draco(Some(10))
                        .map_err(|e| anyhow!("telemetry ingest_draco: {e}"))?;
                    Ok(format!(
                        "[telemetry] draco-mcp: {} metrics, {} bound, {} orphan, {} hotspot(s)",
                        report.total, report.bound, report.orphan, report.hotspots
                    ))
                }
                other => {
                    anyhow::bail!("source={other} not supported — use \"file\" or \"draco-mcp\"");
                }
            }
        }
        "graphify_telemetry_get_context" => {
            let node = get_str("node").ok_or_else(|| anyhow!("Missing 'node'"))?;
            let include_radius = args
                .get("include_impact_radius")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let (_node_id, rows) = telemetry
                .telemetry_get_context(&wk, node, include_radius)
                .map_err(|e| anyhow!("telemetry get_context: {e}"))?;
            if rows.is_empty() {
                Ok(format!("[telemetry] {node}: no telemetry bindings"))
            } else {
                let mut out = format!("[telemetry] {node} — {} binding(s):\n", rows.len());
                for b in &rows {
                    let hotspot = if b.is_hotspot { " 🔥" } else { "" };
                        // ponytail: display-only MB conversion; i64→f64 精確度損失可忽略
                        #[allow(clippy::cast_precision_loss)]
                        let alloc_mb = b.alloc_bytes as f64 / 1_048_576.0;
                        let _ = writeln!(
                            out,
                            "  {}\tp99: {:.1}ms\talloc: {:.1}MB\tcalls/min: {}{}",
                            b.id,
                            b.p99_ms,
                            alloc_mb,
                            b.call_count,
                            hotspot
                        );
                }
                Ok(out)
            }
        }
        _ => anyhow::bail!("Unsupported telemetry tool: {name}"),
    }
}

fn run_coverage_tool(
    name: &str,
    args: &serde_json::Value,
    coverage: &CoveragePlugin,
) -> Result<String> {
    let get_str = |key: &str| args.get(key).and_then(|v| v.as_str());
    let wk = coverage.get_workspace_key().to_string();
    match name {
        "graphify_coverage_ingest" => {
            let format = get_str("format").ok_or_else(|| anyhow!("Missing 'format'"))?;
            let data = get_str("data").ok_or_else(|| anyhow!("Missing 'data'"))?;
            let summary = match format {
                "lcov" => coverage
                    .coverage_ingest_lcov(data)
                    .map_err(|e| anyhow!("coverage ingest-lcov: {e}"))?,
                "json" => coverage
                    .coverage_ingest_json(data)
                    .map_err(|e| anyhow!("coverage ingest-json: {e}"))?,
                other => anyhow::bail!("format={other} not supported — use \"lcov\" or \"json\""),
            };
            Ok(format!(
                "[coverage] ingest-{format}: {} bound nodes, {} total lines, {} covered, {} blindspots",
                summary.bound_nodes, summary.total_lines, summary.covered_lines, summary.blindspots,
            ))
        }
        "graphify_coverage_get_context" => {
            let node = get_str("node").ok_or_else(|| anyhow!("Missing 'node'"))?;
            let db = coverage
                .db()
                .map_err(|e| anyhow!("coverage db: {e}"))?;
            match db.query_by_node(&wk, node)? {
                Some(b) => {
                    let pct = b.line_rate * 100.0;
                    Ok(format!(
                        "[coverage] {node}: {}/{} lines ({:.1}%)",
                        b.covered_lines, b.total_lines, pct
                    ))
                }
                None => Ok(format!("[coverage] {node}: no coverage data")),
            }
        }
        "graphify_coverage_blindspots" => {
            let db = coverage
                .db()
                .map_err(|e| anyhow!("coverage db: {e}"))?;
            let spots = db.query_blindspots(&wk)?;
            if spots.is_empty() {
                Ok("[coverage] no blindspots (all nodes >= 50% coverage)".to_string())
            } else {
                let mut out = format!("[coverage] {} blindspot(s):\n", spots.len());
                for b in &spots {
                    let pct = b.line_rate * 100.0;
                    let _ = writeln!(out, "  {}\t{}/{} ({:.1}%)",
                        b.canonical_node_id, b.covered_lines, b.total_lines, pct
                    );
                }
                Ok(out)
            }
        }
        _ => anyhow::bail!("Unsupported coverage tool: {name}"),
    }
}

fn handle_tool_call(
    name: &str,
    args: serde_json::Value,
    state_lock: Arc<RwLock<GraphState>>,
) -> Result<serde_json::Value> {
    match name {
        "graphify_graph_query" => {
            let params: QueryParams = serde_json::from_value(args)?;
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            let node_id = NodeId(params.question.clone());
            let node_id_resolved = find_node_by_id_or_label(&r_state.graph_data, &node_id)
                .ok_or_else(|| anyhow!("Node not found: {}", params.question))?;

            let res = query_bfs(&r_state.graph, &r_state.node_map, &node_id_resolved, 2)?;
            Ok(serde_json::to_value(res)?)
        }
        "graphify_graph_path" => {
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
        "graphify_graph_summary" => {
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
        "graphify_graph_query_node" => {
            let params: QueryNodeParams = serde_json::from_value(args)?;
            let r_state = state_lock.read().map_err(|_| anyhow!("RwLock poisoned"))?;
            let node_id = NodeId(params.node_id.clone());
            let node_id_resolved = find_node_by_id_or_label(&r_state.graph_data, &node_id)
                .ok_or_else(|| anyhow!("Node not found: {}", params.node_id))?;

            let depth = params.depth.unwrap_or(1);
            let res = query_bfs(&r_state.graph, &r_state.node_map, &node_id_resolved, depth)?;
            Ok(serde_json::to_value(res)?)
        }
        "graphify_graph_trace_path" => {
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
        "graphify_graph_reindex" => {
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
        let suffix: String = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let dir = std::env::temp_dir().join(format!("graphify-mcp-test-{suffix}"));
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
        let suffix: String = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let dir = std::env::temp_dir().join(format!("graphify-mcp-relay-test-{suffix}"));
        fs::create_dir_all(&dir)?;
        let cwd = std::env::current_dir()?;
        std::env::set_current_dir(&dir)?;

        let mut plugin = RelayPlugin::new().with_registry_path(dir.join("graphify-test.db"));
        plugin.bind_for_cli(&dir);
        let empty_args = serde_json::json!({});

        let init_out = run_relay_tool(
            "graphify_relay_init",
            &serde_json::json!({ "project_context": "test project" }),
            &mut plugin,
        )?;
        assert!(init_out.as_str().is_some_and(|s| s.contains("Initialized relay at")));

        let save_out = run_relay_tool(
            "graphify_relay_save",
            &serde_json::json!({
                "repo": "graphify-mcp",
                "phase": "testing",
                "conf": 0.8,
                "next": "run smoke test"
            }),
            &mut plugin,
        )?;
        assert!(save_out.as_str().is_some_and(|s| s.contains("graphify-mcp")));

        let status_out = run_relay_tool("graphify_relay_status", &empty_args, &mut plugin)?;
        assert!(status_out.as_str().is_some_and(|s| s.contains("graphify-mcp")));

        let close_out = run_relay_tool(
            "graphify_relay_close",
            &serde_json::json!({ "repo": "graphify-mcp", "next": "done" }),
            &mut plugin,
        )?;
        assert!(close_out.as_str().is_some_and(|s| s.contains("Consistency: OK")));

        // relayStatus after close must not error (baton was on graphify-mcp).
        assert!(run_relay_tool("graphify_relay_status", &empty_args, &mut plugin)?.as_str().is_some());

        std::env::set_current_dir(cwd)?;
        fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_relay_close_auto_saves_before_close() -> Result<()> {
        // relay_close with save params must auto-save state before closing,
        // so callers can skip the explicit relay_save call.
        let suffix: String = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let dir = std::env::temp_dir().join(format!("graphify-mcp-relay-autosave-test-{suffix}"));
        fs::create_dir_all(&dir)?;
        let cwd = std::env::current_dir()?;
        std::env::set_current_dir(&dir)?;

        let mut plugin = RelayPlugin::new().with_registry_path(dir.join("graphify-test.db"));
        plugin.bind_for_cli(&dir);
        let empty_args = serde_json::json!({});

        let init_out = run_relay_tool(
            "graphify_relay_init",
            &serde_json::json!({ "project_context": "auto-save test" }),
            &mut plugin,
        )?;
        assert!(init_out.as_str().is_some_and(|s| s.contains("Initialized relay at")));

        // Close with all save params in one shot — no relay_save called first
        let close_out = run_relay_tool(
            "graphify_relay_close",
            &serde_json::json!({
                "repo": "test-repo",
                "role": "backend",
                "phase": "dev",
                "volatile": "正在做 auto-save",
                "conf": 4.5,
                "debt": "需補測試,重構 extractor",
                "next": "繼續 auto-save 測試",
                "kind": "backend"
            }),
            &mut plugin,
        )?;
        let out_text = close_out.as_str().unwrap_or("");
        assert!(out_text.contains("Closing ritual for \"test-repo\"."), "{out_text}");
        assert!(out_text.contains("Consistency: OK"), "{out_text}");

        // Verify state was saved: status should show the repo with saved params
        let status_out = run_relay_tool("graphify_relay_status", &empty_args, &mut plugin)?;
        let status_text = status_out.as_str().unwrap_or("");
        assert!(status_text.contains("test-repo"), "{status_text}");
        assert!(status_text.contains("dev"), "{status_text}");

        std::env::set_current_dir(cwd)?;
        fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
