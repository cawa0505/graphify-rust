// ponytail: allow missing errors doc as these are binary CLI entry points
#![allow(clippy::missing_errors_doc)]
// ponytail: allow collapsible_if for nested directory filtering checks
#![allow(clippy::collapsible_if)]

pub mod plugin_host;
pub mod rehydrate;
pub mod skill;
pub mod snapshot;
pub mod tui;
pub mod ui;
pub mod workspace;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use graphify_core::{
    ExtractionResult, GraphMetadata, GraphOutput, GraphUpdateEvent, GraphUpdateKind, Node, NodeId,
    build_graph, derive_workspace_key, extract_file, find_shortest_path, query_bfs,
};
use graphify_plugin_handoff::relay::SaveArgs;
use graphify_plugin_handoff::RelayPlugin;
use graphify_plugin_opendoc::OpendocPlugin;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "graphify", version = "0.1.0", about = "GraphifyRust CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Extract a codebase dependency graph statically using tree-sitter
    Extract {
        /// File or directory to extract
        path: PathBuf,
        /// Output path for the generated graph
        #[arg(short, long, default_value = "graphify-out/graph.toon")]
        output: PathBuf,
        /// Concurrency limit (number of CPU threads) for parallel AST parsing
        #[arg(short = 'j', long)]
        concurrency: Option<usize>,
    },
    /// Query a node in the graph using BFS traversal
    Query {
        /// Target Node ID or label
        target: String,
        /// Maximum traversal depth
        #[arg(short, long, default_value_t = 1)]
        depth: usize,
        /// Path to the graph file
        #[arg(short, long, default_value = "graphify-out/graph.toon")]
        graph: PathBuf,
    },
    /// Find the shortest path between two nodes in the graph
    Path {
        /// Source Node ID or label
        source: String,
        /// Target Node ID or label
        target: String,
        /// Path to the graph file
        #[arg(short, long, default_value = "graphify-out/graph.toon")]
        graph: PathBuf,
    },
    /// Install the Graphify Skill directive for AI Assistants
    InstallSkill {
        /// Install to global level (~/.config/opencode/skills and ~/.cursorrules)
        #[arg(short, long)]
        global: bool,

        /// Custom target directory for installation
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
    /// Interactive TUI-based codebase graph inspector
    Tui {
        /// Path to the graph file
        #[arg(short, long, default_value = "graphify-out/graph.toon")]
        graph: PathBuf,
    },
    /// Index a codebase or a serialized graph file into the Qdrant vector store
    Index {
        /// File or directory to extract and index, or path to a serialized .toon/.json file
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Optional path to the configuration file (default is XDG ~/.config/graphify/config.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Output path if indexing directly from a newly parsed codebase (otherwise defaults to graphify-out/graph.toon)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Force recreation of the Qdrant collection if it already exists
        #[arg(short, long)]
        force: bool,
    },
    /// Manage bound plugins and trigger graph-update events
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    /// Manage graphify workspaces (list, switch, status)
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Relay 狀態接力工具（內嵌 graphify-plugin-handoff）
    Handoff {
        #[command(subcommand)]
        command: HandoffCommand,
    },
    /// 文件↔程式碼追蹤與 drift 偵測（內嵌 graphify-plugin-opendoc）
    Opendoc {
        #[command(subcommand)]
        command: OpendocCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommand {
    /// Manually trigger graph-update hooks for all bound plugins
    RunHooks,
}

#[derive(Subcommand, Debug, Clone)]
pub enum WorkspaceCommand {
    /// List all registered workspaces
    List,
    /// Switch the active workspace
    Switch {
        /// Workspace key to activate
        workspace_key: String,
    },
    /// Show status of a workspace
    Status {
        /// Workspace key (defaults to active)
        workspace_key: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum HandoffCommand {
    /// 建立 relay.json（Code Relay 起點）
    Init {
        /// 專案 context 描述
        project: String,
        /// template kind: backend | frontend | infra
        #[arg(short, long)]
        kind: Option<String>,
    },
    /// 儲存目前工作狀態到 relay.json
    Save {
        /// 子 repo 名稱（預設：目前目錄 basename）
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        phase: Option<String>,
        #[arg(long)]
        volatile: Option<String>,
        /// 信心度 0..5
        #[arg(long)]
        conf: Option<f64>,
        #[arg(long)]
        next: Option<String>,
        #[arg(long)]
        debt: Option<String>,
        #[arg(long)]
        kind: Option<String>,
    },
    /// 執行 closing ritual（consistency check + spec sync + commit + snapshot）
    Close {
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        next: Option<String>,
    },
    /// 切換 active baton 到指定 repo
    Switch {
        /// 子 repo 名稱
        repo: String,
        #[arg(long)]
        kind: Option<String>,
    },
    /// 渲染 RESUME.md 交接文件
    Resume {
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        kind: Option<String>,
    },
    /// 顯示 relay 摘要（repos / active baton / 更新時間）
    Status,
    /// 匯入舊專案的 TODO/handoff 文件
    Add {
        /// 舊文件路徑
        file: String,
        #[arg(long)]
        repo: Option<String>,
    },
    /// 安裝/移除雙軌 agent skill（MCP + CLI 備援）
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum SkillCommand {
    /// 安裝 SKILL.md 到本地 agent 生態（managed copy，可重複執行）
    Install {
        /// 目標 agent：opencode,claude,cursor,cline（逗號分隔；預設：偵測到的全部）
        #[arg(long)]
        agent: Option<String>,
        /// 安裝範圍：user（$HOME）| project（cwd）；預設兩者皆裝
        #[arg(long)]
        scope: Option<String>,
    },
    /// 移除已安裝的 skill（只刪帶 managed 標記的檔案）
    Uninstall {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        scope: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum OpendocCommand {
    /// 全量索引當前 workspace 下所有 `.md` 檔的 spec↔symbol 連結（無參數時）
    /// 或僅索引顯式指定的 doc 路徑（root 相對路徑，逗號分隔可多個）
    Index {
        #[arg(long, value_delimiter = ',')]
        doc_paths: Vec<String>,
    },
    /// doc → code：「改了 docs/auth.md，哪些 symbols 受影響？」
    TraceDoc {
        /// doc 路徑（相對於 workspace root）
        doc_path: String,
    },
    /// code → doc：「`crate::auth::verify_token` 由哪份 spec 描述？」
    TraceCode {
        /// 完整 qualified symbol，如 `crate::auth::verify_token`
        symbol: String,
    },
    /// doc-side drift 偵測：每個 indexed link 重新讀檔，比對 block signature
    AuditDrift,
    /// doc→code drift：spec 宣告的 symbol 在 graph 中找不到（需供給 known symbols）
    AuditMissing {
        /// 逗號分隔的已實作 symbol 清單（從 graphify-core 圖譜匯出）
        #[arg(long, value_delimiter = ',')]
        symbols: String,
    },
    /// 設定 `workspace_key → od_workspace_id` 對映（Layer 2 用；Layer 1 可不設）
    SetMapping {
        /// `OpenDocuments` `workspace_id` 字串（非 Graphify 的 hash key）
        od_workspace_id: String,
    },
    /// 顯示已設定的 `od_workspace_id`
    GetMapping,
    /// 安裝/移除雙軌 agent skill（MCP + CLI 備援）
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Extract {
            path,
            output,
            concurrency,
        } => run_extract(&path, &output, concurrency)?,
        Commands::Query {
            target,
            depth,
            graph,
        } => run_query(&target, depth, &graph)?,
        Commands::Path {
            source,
            target,
            graph,
        } => run_path(&source, &target, &graph)?,
        Commands::InstallSkill { global, dir } => skill::install_skill(global, dir)?,
        Commands::Tui { graph } => run_tui(&graph)?,
        Commands::Index {
            path,
            config,
            output,
            force,
        } => run_index(&path, config.as_deref(), output.as_deref(), force)?,
        Commands::Plugin { command } => match command {
            PluginCommand::RunHooks => run_hooks()?,
        },
        Commands::Workspace { command } => match command {
            WorkspaceCommand::List => run_workspace_list()?,
            WorkspaceCommand::Switch { workspace_key } => run_workspace_switch(&workspace_key)?,
            WorkspaceCommand::Status { workspace_key } => run_workspace_status(workspace_key)?,
        },
        Commands::Handoff { command } => run_handoff(command)?,
        Commands::Opendoc { command } => run_opendoc(command)?,
    }
    Ok(())
}

fn run_extract(input_path: &Path, output_path: &Path, concurrency: Option<usize>) -> Result<()> {
    let mut file_paths = Vec::new();
    if input_path.is_file() {
        if get_lang(input_path).is_some() {
            file_paths.push(input_path.to_path_buf());
        }
    } else {
        collect_files(input_path, &mut file_paths)?;
    }

    // Configure Rayon if specified
    let final_concurrency = concurrency.or_else(|| {
        graphify_llm::config::LLMConfig::load_from_file("")
            .ok()
            .and_then(|cfg| cfg.extraction.concurrency)
    });

    if let Some(n) = final_concurrency {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global();
    }

    let results: Vec<(String, ExtractionResult)> = file_paths
        .par_iter()
        .filter_map(|path| {
            let lang = get_lang(path)?;
            let res = extract_file(path).ok()?;
            Some((lang, res))
        })
        .collect();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut languages = std::collections::HashSet::new();

    for (lang, res) in results {
        nodes.extend(res.nodes);
        edges.extend(res.edges);
        languages.insert(lang);
    }

    let metadata = GraphMetadata {
        version: "1.0.0".to_string(),
        generated_at: "0".to_string(),
        total_nodes: nodes.len(),
        total_edges: edges.len(),
        languages: languages.into_iter().collect(),
        input_tokens: 0,
        output_tokens: 0,
        ..Default::default()
    };

    let graph_out = GraphOutput {
        nodes,
        edges,
        metadata,
    };

    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
        if parent != Path::new("") && parent != Path::new(".") {
            let gitignore_path = parent.join(".gitignore");
            if !gitignore_path.exists() {
                std::fs::write(&gitignore_path, "*\n")?;
            }
        }
    }

    if output_path.extension().and_then(|e| e.to_str()) == Some("toon") {
        let toon_str = graphify_core::to_toon(&graph_out);
        std::fs::write(output_path, toon_str)?;
    } else {
        let json_str = serde_json::to_string_pretty(&graph_out)?;
        std::fs::write(output_path, json_str)?;
    }
    println!(
        "Successfully extracted graph to {} (nodes: {}, edges: {})",
        output_path.display(),
        graph_out.metadata.total_nodes,
        graph_out.metadata.total_edges
    );

    // Broadcast an `Extracted` graph-update event to all bound plugins.
    let mut host = plugin_host::PluginHost::new();
    host.broadcast(&GraphUpdateEvent::new(
        derive_workspace_key(input_path),
        graph_out.nodes.iter().map(|n| n.id.clone()).collect(),
        GraphUpdateKind::Extracted,
    ));
    Ok(())
}

fn get_lang(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| match ext.to_lowercase().as_str() {
            "py" => "python".to_string(),
            "rs" => "rust".to_string(),
            "go" => "go".to_string(),
            "js" | "jsx" | "mjs" => "javascript".to_string(),
            "ts" | "tsx" | "mts" => "typescript".to_string(),
            "c" | "h" => "c".to_string(),
            "cpp" | "cc" | "cxx" | "hpp" | "h++" | "hh" => "cpp".to_string(),
            "php" => "php".to_string(),
            _ => ext.to_lowercase(),
        })
}

fn collect_files(dir: &Path, file_paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
            }
            collect_files(&path, file_paths)?;
        } else if get_lang(&path).is_some() {
            file_paths.push(path);
        }
    }
    Ok(())
}

// ponytail: allow too_many_lines since parsing legacy JSON with manual field conversion is naturally long
#[allow(clippy::too_many_lines)]
fn load_graph_output(path: &Path) -> Result<GraphOutput> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read graph file: {}", path.display()))?;
    if path.extension().and_then(|e| e.to_str()) == Some("toon") {
        graphify_core::from_toon(&content)
    } else {
        // Support backward compatibility for old .json graph format or current JSON
        match serde_json::from_str::<GraphOutput>(&content) {
            Ok(output) => Ok(output),
            Err(e) => {
                // If standard parsing fails, try parsing custom legacy / partial json or fallback
                eprintln!(
                    "[graphify] Warning: Failed to parse JSON as current schema ({e}). Attempting legacy transformation..."
                );
                // Try reading as unstructured json to see if we can migrate it
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(nodes_arr) = val.get("nodes").and_then(serde_json::Value::as_array)
                    {
                        let mut nodes = Vec::new();
                        for n_val in nodes_arr {
                            let id = n_val
                                .get("id")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let label = n_val
                                .get("label")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string();

                            let file_type =
                                match n_val.get("file_type").and_then(serde_json::Value::as_str) {
                                    Some("code") => graphify_core::FileType::Code,
                                    Some("document") => graphify_core::FileType::Document,
                                    Some("paper") => graphify_core::FileType::Paper,
                                    Some("image") => graphify_core::FileType::Image,
                                    Some("rationale") => graphify_core::FileType::Rationale,
                                    Some("concept") => graphify_core::FileType::Concept,
                                    _ => {
                                        // Map legacy "kind" or fallback
                                        let kind = n_val
                                            .get("kind")
                                            .and_then(serde_json::Value::as_str)
                                            .unwrap_or("");
                                        if kind == "file" || kind == "module" {
                                            graphify_core::FileType::Document
                                        } else {
                                            graphify_core::FileType::Code
                                        }
                                    }
                                };

                            let kind = n_val
                                .get("kind")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("unknown")
                                .to_string();
                            let language = n_val
                                .get("language")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("unknown")
                                .to_string();
                            let source_file = n_val
                                .get("source_file")
                                .or_else(|| n_val.get("file_path"))
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string();

                            let start_line_val = n_val
                                .get("start_line")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0);
                            let start_line = usize::try_from(start_line_val).unwrap_or(0);

                            let end_line_val = n_val
                                .get("end_line")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0);
                            let end_line = usize::try_from(end_line_val).unwrap_or(0);

                            let doc_comment = n_val
                                .get("doc_comment")
                                .and_then(serde_json::Value::as_str)
                                .map(std::string::ToString::to_string);
                            let description = n_val
                                .get("description")
                                .and_then(serde_json::Value::as_str)
                                .map(std::string::ToString::to_string);
                            let metadata = n_val
                                .get("metadata")
                                .and_then(serde_json::Value::as_object)
                                .cloned();

                            nodes.push(Node {
                                id: graphify_core::NodeId(id),
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

                        let mut edges = Vec::new();
                        if let Some(edges_arr) =
                            val.get("edges").and_then(serde_json::Value::as_array)
                        {
                            for e_val in edges_arr {
                                let source = e_val
                                    .get("source")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                let target = e_val
                                    .get("target")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                let relation = e_val
                                    .get("relation")
                                    .or_else(|| e_val.get("kind")) // old schema used "kind" for edges too
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("calls")
                                    .to_string();
                                let source_file = e_val
                                    .get("source_file")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                let confidence = e_val
                                    .get("confidence")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("EXTRACTED")
                                    .to_string();
                                let source_location = e_val
                                    .get("source_location")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                let description = e_val
                                    .get("description")
                                    .and_then(serde_json::Value::as_str)
                                    .map(std::string::ToString::to_string);

                                edges.push(graphify_core::Edge {
                                    source: graphify_core::NodeId(source),
                                    target: graphify_core::NodeId(target),
                                    relation,
                                    source_file,
                                    confidence,
                                    source_location,
                                    description,
                                });
                            }
                        }

                        let version = val
                            .get("metadata")
                            .and_then(|m| m.get("version"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("1.0.0")
                            .to_string();
                        let generated_at = val
                            .get("metadata")
                            .and_then(|m| m.get("generated_at"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("0")
                            .to_string();
                        let total_nodes = nodes.len();
                        let total_edges = edges.len();
                        let languages = val
                            .get("metadata")
                            .and_then(|m| m.get("languages"))
                            .and_then(serde_json::Value::as_array)
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| {
                                        v.as_str().map(std::string::ToString::to_string)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        return Ok(GraphOutput {
                            nodes,
                            edges,
                            metadata: graphify_core::GraphMetadata {
                                version,
                                generated_at,
                                total_nodes,
                                total_edges,
                                languages,
                                input_tokens: 0,
                                output_tokens: 0,
                                ..Default::default()
                            },
                        });
                    }
                }
                Err(anyhow!("Failed to deserialize or migrate JSON: {e}"))
            }
        }
    }
}

fn find_node(nodes: &[Node], input: &str) -> Option<NodeId> {
    if let Some(node) = nodes.iter().find(|n| n.id.0 == input) {
        return Some(node.id.clone());
    }
    let input_lower = input.to_lowercase();
    nodes
        .iter()
        .find(|n| n.label.to_lowercase() == input_lower || n.id.0.to_lowercase() == input_lower)
        .map(|n| n.id.clone())
}

fn run_query(target: &str, depth: usize, graph_path: &Path) -> Result<()> {
    let graph_out = load_graph_output(graph_path)?;
    let target_node_id =
        find_node(&graph_out.nodes, target).ok_or_else(|| anyhow!("Node not found: {target}"))?;

    let (graph, node_map) = build_graph(&graph_out.nodes, &graph_out.edges)?;
    let result = query_bfs(&graph, &node_map, &target_node_id, depth)?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn run_path(source: &str, target: &str, graph_path: &Path) -> Result<()> {
    let graph_out = load_graph_output(graph_path)?;
    let src_node_id = find_node(&graph_out.nodes, source)
        .ok_or_else(|| anyhow!("Source node not found: {source}"))?;
    let tgt_node_id = find_node(&graph_out.nodes, target)
        .ok_or_else(|| anyhow!("Target node not found: {target}"))?;

    let (graph, node_map) = build_graph(&graph_out.nodes, &graph_out.edges)?;
    let path_opt = find_shortest_path(&graph, &node_map, &src_node_id, &tgt_node_id)?;

    match path_opt {
        Some(path) => {
            println!("{}", serde_json::to_string_pretty(&path)?);
        }
        None => {
            println!("[]");
        }
    }
    Ok(())
}

fn run_tui(graph_path: &Path) -> Result<()> {
    let graph = load_graph_output(graph_path)?;
    tui::run_tui(graph)?;
    Ok(())
}

/// Manually trigger a `Manual` graph-update event for all bound plugins.
fn run_hooks() -> Result<()> {
    let mut host = plugin_host::PluginHost::new();
    host.broadcast(&GraphUpdateEvent::new(
        derive_workspace_key(std::env::current_dir()?),
        Vec::new(),
        GraphUpdateKind::Manual,
    ));
    println!(
        "[graphify] Broadcast manual graph-update event to {} plugin(s).",
        host.len()
    );
    Ok(())
}

/// List all registered workspaces from the global registry.
fn run_workspace_list() -> Result<()> {
    let rows = workspace::list_workspaces()?;
    if rows.is_empty() {
        println!("[graphify] No registered workspaces.");
        return Ok(());
    }
    for row in rows {
        let marker = if row.is_active { "*" } else { " " };
        println!("{marker} {} {}", row.workspace_key, row.root_path);
    }
    Ok(())
}

/// Switch the active workspace in the global registry.
fn run_workspace_switch(workspace_key: &str) -> Result<()> {
    workspace::switch_workspace(workspace_key)?;
    println!("[graphify] Active workspace: {workspace_key}");
    Ok(())
}

/// Show the active workspace, or the status of a named one.
fn run_workspace_status(workspace_key: Option<String>) -> Result<()> {
    if let Some(key) = workspace_key {
        if let Some(row) = workspace::workspace_status(&key)? {
            println!("{} {}", row.workspace_key, row.root_path);
        } else {
            println!("[graphify] Workspace not registered: {key}");
        }
    } else if let Some(row) = workspace::active_workspace()? {
        println!("{} {}", row.workspace_key, row.root_path);
    } else {
        println!("[graphify] No active workspace.");
    }
    Ok(())
}

/// 透過內嵌的 handoff plugin 執行 relay* 工具。
///
/// 以目前目錄合成 `WorkspaceContext` 綁定 plugin，並將全域 graphify.db 路徑
/// （graphify-registry XDG 解析）注入 `.with_registry_path` — relayClose 的
/// `HandoffSnapshot` 同步即寫入同一資料庫。
fn run_handoff(command: HandoffCommand) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut plugin =
        RelayPlugin::new().with_registry_path(graphify_registry::registry_db_path());
    plugin.bind_for_cli(&cwd);
    let out = match command {
        HandoffCommand::Init { project, kind } => plugin.relay_init(&project, kind.as_deref())?,
        HandoffCommand::Save {
            repo,
            role,
            phase,
            volatile,
            conf,
            next,
            debt,
            kind,
        } => plugin.relay_save(SaveArgs {
            repo: repo.as_deref(),
            role: role.as_deref(),
            active_phase: phase.as_deref(),
            volatile_state: volatile.as_deref(),
            confidence: conf,
            next_session_starter: next.as_deref(),
            debt_tag: debt.as_deref(),
            kind: kind.as_deref(),
        })?,
        HandoffCommand::Close { repo, next } => {
            plugin.relay_close(repo.as_deref(), next.as_deref())?
        }
        HandoffCommand::Switch { repo, kind } => plugin.relay_switch(&repo, kind.as_deref())?,
        HandoffCommand::Resume { repo, kind } => {
            plugin.relay_resume(repo.as_deref(), kind.as_deref())?
        }
        HandoffCommand::Status => plugin.relay_status()?,
        HandoffCommand::Add { file, repo } => {
            plugin.relay_add(Path::new(&file), repo.as_deref())?
        }
        HandoffCommand::Skill { command } => run_skill_command(command)?,
    };
    println!("{out}");
    Ok(())
}

/// `graphify opendoc` — 文件↔程式碼追蹤與 drift 偵測（內嵌 graphify-plugin-opendoc）。
///
/// 比照 `handoff` 整合模式：cwd 合成 `WorkspaceContext`、全域 graphify.db
/// 路徑注入。Layer 2 backend：讀 `OD_BASE_URL` env（如 `http://127.0.0.1:8080`）。
fn od_base_url() -> Option<String> {
    std::env::var("OD_BASE_URL").ok().filter(|s| !s.trim().is_empty())
}

/// 建構 OpendocPlugin（注入 registry + Layer 2 backend）並 bind 到 cwd。
fn build_opendoc_for_cli() -> Result<graphify_plugin_opendoc::OpendocPlugin> {
    let cwd = std::env::current_dir()?;
    let mut plugin = OpendocPlugin::new()
        .with_registry_path(graphify_registry::registry_db_path());
    if let Some(url) = od_base_url() {
        plugin = plugin.with_backend(Box::new(
            graphify_plugin_opendoc::RestBackend::new(&url),
        ));
    }
    Ok(plugin.bind_for_cli(&cwd))
}

/// `graphify opendoc` — 文件↔程式碼追蹤與 drift 偵測（內嵌 graphify-plugin-opendoc）。
fn run_opendoc(command: OpendocCommand) -> Result<()> {
    let plugin = build_opendoc_for_cli()?;
    match command {
        OpendocCommand::Index { doc_paths } => {
            let (n, msg) = if doc_paths.is_empty() {
                let n = plugin
                    .index_all_docs()
                    .map_err(|e| anyhow!("opendoc index_all_docs: {e}"))?;
                (n, "indexed all docs under workspace root")
            } else {
                let n = plugin
                    .index_doc_paths(&doc_paths)
                    .map_err(|e| anyhow!("opendoc index_doc_paths: {e}"))?;
                (n, "indexed explicit doc paths")
            };
            println!("[opendoc] {msg}: {n} link rows");
        }
        OpendocCommand::TraceDoc { doc_path } => {
            let rows = plugin
                .trace_doc_to_code(&doc_path)
                .map_err(|e| anyhow!("opendoc trace_doc_to_code: {e}"))?;
            if rows.is_empty() {
                println!("[opendoc] {doc_path}: no indexed specs");
            } else {
                for r in &rows {
                    println!("{}\t{}\t{}", r.spec_id, r.symbol, r.doc_path);
                }
            }
        }
        OpendocCommand::TraceCode { symbol } => {
            let rows = plugin
                .fetch_code_to_doc_context(&symbol)
                .map_err(|e| anyhow!("opendoc fetch_code_to_doc_context: {e}"))?;
            if rows.is_empty() {
                println!("[opendoc] {symbol}: no spec coverage");
            } else {
                for r in &rows {
                    println!("{}\t{}\t{}", r.spec_id, r.symbol, r.doc_path);
                }
            }
        }
        OpendocCommand::AuditDrift => {
            let items = plugin
                .audit_drift()
                .map_err(|e| anyhow!("opendoc audit_drift: {e}"))?;
            if items.is_empty() {
                println!("[opendoc] no indexed links to audit");
            } else {
                for item in &items {
                    println!(
                        "{}\t{}\t{}\t{:?}",
                        item.spec_id, item.symbol, item.doc_path, item.status
                    );
                }
            }
        }
        OpendocCommand::AuditMissing { symbols } => {
            let known: Vec<String> = symbols
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            let items = plugin
                .audit_code_missing(&known)
                .map_err(|e| anyhow!("opendoc audit_code_missing: {e}"))?;
            if items.is_empty() {
                println!("[opendoc] no doc-declared symbols missing from the graph");
            } else {
                for item in &items {
                    println!(
                        "{}\t{}\t{}\t{:?}",
                        item.spec_id, item.symbol, item.doc_path, item.status
                    );
                }
            }
        }
        OpendocCommand::SetMapping { od_workspace_id } => {
            plugin
                .set_workspace_mapping(&od_workspace_id)
                .map_err(|e| anyhow!("opendoc set_workspace_mapping: {e}"))?;
            println!("[opendoc] workspace mapping set → {od_workspace_id}");
        }
        OpendocCommand::GetMapping => {
            let mapping = plugin
                .get_workspace_mapping()
                .map_err(|e| anyhow!("opendoc get_workspace_mapping: {e}"))?;
            match mapping {
                Some(id) => println!("[opendoc] od_workspace_id = {id}"),
                None => println!("[opendoc] no workspace mapping set"),
            }
        }
        OpendocCommand::Skill { command } => {
            let out = opendoc_run_skill_command(command)?;
            println!("{out}");
        }
    }
    Ok(())
}

/// `graphify handoff skill` — 安裝/移除雙軌 SKILL.md 到本地 agent 生態。
fn run_skill_command(command: SkillCommand) -> Result<String> {
    use graphify_plugin_handoff::skill_install::{self, Agent, Scope};

    let resolve_agents = |explicit: &Option<String>| -> Result<Vec<Agent>> {
        if let Some(list) = explicit {
            list.split(',')
                .map(|s| Agent::parse(s.trim()).ok_or_else(|| anyhow!("unknown agent: {s}")))
                .collect()
        } else {
            let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("$HOME is not set"))?;
            let cwd = std::env::current_dir()?;
            let found = skill_install::detect_agents(Path::new(&home), &cwd);
            if found.is_empty() {
                return Err(anyhow!(
                    "no known agent config found — pass --agent opencode|claude|cursor|cline"
                ));
            }
            Ok(found)
        }
    };

    let resolve_scopes = |explicit: &Option<String>| -> Result<Vec<Scope>> {
        if let Some(s) = explicit {
            let scope = Scope::parse(s).ok_or_else(|| anyhow!("unknown scope: {s}"))?;
            Ok(vec![scope])
        } else {
            Ok(vec![Scope::User, Scope::Project])
        }
    };

    let (agents_opt, scopes_opt, is_install) = match command {
        SkillCommand::Install { agent, scope } => (agent, scope, true),
        SkillCommand::Uninstall { agent, scope } => (agent, scope, false),
    };

    let agents = resolve_agents(&agents_opt)?;
    let mut lines = Vec::new();
    for scope in resolve_scopes(&scopes_opt)? {
        if is_install {
            let report = skill_install::install(&agents, scope)?;
            for path in &report.installed {
                lines.push(format!("installed: {}", path.display()));
            }
            for (path, why) in &report.skipped {
                lines.push(format!("skipped: {} — {why}", path.display()));
            }
        } else {
            let report = skill_install::uninstall(&agents, scope)?;
            for path in &report.removed {
                lines.push(format!("removed: {}", path.display()));
            }
            for (path, why) in &report.skipped {
                lines.push(format!("skipped: {} — {why}", path.display()));
            }
        }
    }
    Ok(lines.join("\n"))
}

/// `graphify opendoc skill` — 安裝/移除雙軌 SKILL.md（opendoc 版，輸出帶
/// `[opendoc]` 前綴）。
fn opendoc_run_skill_command(command: SkillCommand) -> Result<String> {
    use graphify_plugin_opendoc::skill_install::{self, Agent, Scope};

    let resolve_agents = |explicit: &Option<String>| -> Result<Vec<Agent>> {
        if let Some(list) = explicit {
            list.split(',')
                .map(|s| Agent::parse(s.trim()).ok_or_else(|| anyhow!("unknown agent: {s}")))
                .collect()
        } else {
            let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("$HOME is not set"))?;
            let cwd = std::env::current_dir()?;
            let found = skill_install::detect_agents(Path::new(&home), &cwd);
            if found.is_empty() {
                return Err(anyhow!(
                    "no known agent config found — pass --agent opencode|claude|cursor|cline"
                ));
            }
            Ok(found)
        }
    };

    let resolve_scopes = |explicit: &Option<String>| -> Result<Vec<Scope>> {
        if let Some(s) = explicit {
            let scope = Scope::parse(s).ok_or_else(|| anyhow!("unknown scope: {s}"))?;
            Ok(vec![scope])
        } else {
            Ok(vec![Scope::User, Scope::Project])
        }
    };

    let (agents_opt, scopes_opt, is_install) = match command {
        SkillCommand::Install { agent, scope } => (agent, scope, true),
        SkillCommand::Uninstall { agent, scope } => (agent, scope, false),
    };

    let agents = resolve_agents(&agents_opt)?;
    let mut lines = Vec::new();
    for scope in resolve_scopes(&scopes_opt)? {
        if is_install {
            let report = skill_install::install(&agents, scope)?;
            for path in &report.installed {
                lines.push(format!("[opendoc] installed: {}", path.display()));
            }
            for (path, why) in &report.skipped {
                lines.push(format!("[opendoc] skipped: {} — {why}", path.display()));
            }
        } else {
            let report = skill_install::uninstall(&agents, scope)?;
            for path in &report.removed {
                lines.push(format!("[opendoc] removed: {}", path.display()));
            }
            for (path, why) in &report.skipped {
                lines.push(format!("[opendoc] skipped: {} — {why}", path.display()));
            }
        }
    }
    Ok(lines.join("\n"))
}

fn run_index(
    path: &Path,
    config_path: Option<&Path>,
    output_path: Option<&Path>,
    force: bool,
) -> Result<()> {
    // 1. 載入 LLM & Memory 設定
    let config = if let Some(cfg_p) = config_path {
        graphify_llm::config::LLMConfig::load_from_file(cfg_p.to_str().unwrap_or(""))?
    } else {
        graphify_llm::config::LLMConfig::load_from_file("")?
    };

    // Incremental-sync state: Some(changed) means "only sync these files", None means full upsert.
    let mut changed_files: Option<HashSet<String>> = None;
    // Snapshot written only after a successful upload so a failed run doesn't
    // poison the incremental diff (a stale snapshot would skip a rebuild).
    let mut snapshot_state: Option<(PathBuf, BTreeMap<String, String>)> = None;

    // 2. 獲取 GraphOutput
    let graph_out = if path.is_file() {
        let ext = path.extension().and_then(|e| e.to_str());
        if ext == Some("toon") || ext == Some("json") {
            load_graph_output(path)?
        } else {
            return Err(anyhow!(
                "Unsupported file format for indexing: {}. Supported: .toon, .json",
                path.display()
            ));
        }
    } else {
        // 如果是目錄，優先尋找已經提取好的 graphify-out/graph.toon
        let default_toon = PathBuf::from("graphify-out/graph.toon");
        let default_json = PathBuf::from("graphify-out/graph.json");

        let out_p = output_path.map_or_else(|| default_toon.clone(), Path::to_path_buf);
        let snapshot_path = out_p
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".graphify-snapshot.json");

        let current_hashes = snapshot::compute_file_hashes(path)?;
        let old_hashes = snapshot::load_snapshot(&snapshot_path);
        let changed = snapshot::diff_hashes(&old_hashes, &current_hashes);

        let graph = if !force && !old_hashes.is_empty() {
            // Incremental: a snapshot exists, so we only re-extract and sync when files changed.
            if changed.is_empty() {
                println!(
                    "[graphify] No source changes detected since last index ({} file(s) unchanged), skipping.",
                    current_hashes.len()
                );
                return Ok(());
            }
            println!(
                "[graphify] {} file(s) changed, re-extracting graph for incremental sync...",
                changed.len()
            );
            run_extract(path, &out_p, None)?;
            changed_files = Some(changed);
            load_graph_output(&out_p)?
        } else if out_p.exists() {
            load_graph_output(&out_p)?
        } else if out_p == default_toon && default_json.exists() {
            println!(
                "[graphify] Found legacy JSON graph at {}, migrating and loading...",
                default_json.display()
            );
            load_graph_output(&default_json)?
        } else {
            // 如果不存在，且使用者沒有下 -f (force) 參數
            if !force {
                println!(
                    "[graphify] No existing extracted graph file found ({}).",
                    out_p.display()
                );
                println!(
                    "[graphify] To parse your codebase and index from scratch, run with --force / -f:"
                );
                println!("  graphify index {} --force", path.display());
                return Ok(());
            }
            // 否則，在 force 的情況下，我們自動進行 Extract 提取
            println!("Extracting codebase graph first before indexing...");
            run_extract(path, &out_p, None)?;
            load_graph_output(&out_p)?
        };

        // Persist the fresh snapshot for the next incremental run (also after force re-index).
        snapshot_state = Some((snapshot_path, current_hashes));
        graph
    };

    if graph_out.nodes.is_empty() {
        println!("No nodes found to index!");
        return Ok(());
    }

    sync_to_qdrant(
        &config,
        &graph_out,
        changed_files.as_ref(),
        force,
        &derive_workspace_key(path),
    )?;

    if let Some((snapshot_path, hashes)) = &snapshot_state {
        snapshot::save_snapshot(snapshot_path, hashes)?;
        println!(
            "[graphify] Snapshot saved for incremental indexing: {}",
            snapshot_path.display()
        );
    }

    println!("Successfully indexed codebase graph into Qdrant store!");

    // Startup boundary: rehydrate plugin-memory WAL deltas to the external
    // Qdrant server if it is reachable (P4 task 5.2). Never blocks indexing.
    rehydrate_plugin_memory(&config, &derive_workspace_key(path))?;

    // Broadcast an `Indexed` graph-update event to all bound plugins.
    broadcast_indexed(path, &graph_out);
    Ok(())
}

/// Startup-boundary rehydration (RFC-0004 §1.3.1, P4 task 5.2): probe the
/// external Qdrant server once with a bounded ping; if healthy, push pending
/// plugin-memory WAL deltas to the server collection and flip registrations
/// `Ready`. A probe failure is non-fatal — plugin memory stays local and the
/// CLI continues.
fn rehydrate_plugin_memory(
    config: &graphify_llm::config::LLMConfig,
    workspace_key: &str,
) -> Result<()> {
    use graphify_registry::resync::{ProviderProbe, ResyncOutcome, check_and_resync};

    /// Probes the external Qdrant server `/healthz` with the same bounded
    /// 10ms semantics as `init_with_fallback`'s `server_healthy`.
    struct QdrantServerProbe {
        enabled: bool,
        url: String,
        rt: tokio::runtime::Runtime,
    }
    impl ProviderProbe for QdrantServerProbe {
        fn is_available(&self) -> bool {
            self.enabled && self.rt.block_on(graphify_memory::server_healthy(&self.url))
        }
    }

    let probe = QdrantServerProbe {
        enabled: config.memory.long_term.enabled,
        url: config.memory.long_term.qdrant.url.clone(),
        rt: tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    };
    if !probe.is_available() {
        println!(
            "[graphify] Qdrant server unreachable; plugin memory stays local (rehydration deferred)."
        );
        return Ok(());
    }

    let db = workspace::open_registry()?;
    let local_store = Arc::new(graphify_memory::plugin_memory::PluginDomainMemory::new(
        graphify_memory::plugin_memory::PluginDomainMemory::default_dir(),
    ));
    let job = rehydrate::RehydrateJob::new(local_store, &config.memory.long_term.qdrant.url)?;
    match check_and_resync(&db, &probe, &job, workspace_key)? {
        ResyncOutcome::Synced => println!(
            "[graphify] Rehydrated plugin memory deltas to Qdrant server (registrations Ready)."
        ),
        ResyncOutcome::ProviderUnavailable => println!(
            "[graphify] Rehydration job failed; plugin memory stays local (retry next run)."
        ),
    }
    Ok(())
}

fn sync_to_qdrant(
    config: &graphify_llm::config::LLMConfig,
    graph_out: &GraphOutput,
    changed_files: Option<&HashSet<String>>,
    force: bool,
    workspace_key: &str,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    println!(
        "Connecting to Ollama and Qdrant to generate embeddings and index {} nodes...",
        graph_out.nodes.len()
    );
    let store = rt.block_on(async {
        graphify_memory::QdrantMemoryStore::init_with_fallback(
            config.memory.long_term.clone(),
            config.extraction.concurrency,
        )
        .await
    })?;
    rt.block_on(async {
        if force {
            let _ = store.delete_collection().await;
            println!(
                "Deleted existing Qdrant collection '{}' for force-recreation.",
                config.memory.long_term.qdrant.collection
            );
        }
        store.ensure_collection().await?;
        if let Some(changed) = changed_files {
            println!(
                "Incrementally syncing {} changed file(s) into Qdrant collection '{}'...",
                changed.len(),
                config.memory.long_term.qdrant.collection
            );
            store
                .sync_nodes(&graph_out.nodes, workspace_key, changed)
                .await?;
        } else {
            println!(
                "Uploading nodes to Qdrant collection '{}'...",
                config.memory.long_term.qdrant.collection
            );
            store.upsert_nodes(&graph_out.nodes, workspace_key).await?;
        }
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

/// Broadcast an `Indexed` graph-update event to all bound plugins.
fn broadcast_indexed(path: &Path, graph_out: &GraphOutput) {
    let mut host = plugin_host::PluginHost::new();
    host.broadcast(&GraphUpdateEvent::new(
        derive_workspace_key(path),
        graph_out.nodes.iter().map(|n| n.id.clone()).collect(),
        GraphUpdateKind::Indexed,
    ));
}
