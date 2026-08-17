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
    GraphifyPlugin, build_graph, derive_workspace_key, extract_file, find_shortest_path, query_bfs,
};
use graphify_plugin_handoff::relay::SaveArgs;
use graphify_plugin_handoff::RelayPlugin;
use graphify_plugin_opendoc::OpendocPlugin;
use graphify_plugin_test_coverage::CoveragePlugin;
use graphify_registry::db::RegistryDb;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "graphify", version = env!("CARGO_PKG_VERSION"), about = "GraphifyRust CLI")]
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
    /// code-review-graph 橋接（內嵌 graphify-plugin-review）
    Review {
        #[command(subcommand)]
        command: ReviewCommand,
    },
    /// 測試覆蓋率橋接（內嵌 graphify-plugin-test-coverage）
    Coverage {
        #[command(subcommand)]
        command: CoverageCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommand {
    /// Manually trigger graph-update hooks for all bound plugins
    RunHooks,
    /// Run a passive health probe on all bound plugins
    Probe,
    /// Reset a quarantined plugin and re-probe it
    Reset {
        /// Plugin ID to reset (e.g. handoff, opendoc, review)
        plugin_id: String,
    },
    /// List all registered plugins with their current health status
    List,
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
    /// Add a workspace by root path. Key is auto-derived; '.' resolved to full path.
    Add {
        /// Root path of the workspace (e.g. /home/user/project or .)
        root_path: String,
    },
    /// Delete a workspace by key
    Delete {
        /// Workspace key to delete
        workspace_key: String,
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

#[derive(Subcommand, Debug, Clone)]
pub enum ReviewCommand {
    /// 從 CRG `IngestPayload` JSON 檔案匯入 review 點位（line→symbol 升維綁定）
    Ingest {
        /// `IngestPayload` JSON 檔案路徑
        payload: PathBuf,
    },
    /// 呼叫 CRG `detect_changes_tool`，把 top-10 風險節點對映成
    /// review 點位並綁定（`CRG_BASE_URL` 指定 CRG endpoint）
    SearchCrg {
        /// git diff base ref（預設 HEAD~1；指定更早 commit 可涵蓋更多變更）
        #[arg(long)]
        base: Option<String>,
    },
    /// 查詢某 canonical node id 關聯的未解決 review
    GetContext {
        /// canonical node id（如 `src/auth.rs:function:verify_token`，由 ingest 時綁定）
        node: String,
    },
    /// 手動標記一筆 review 為已解決
    Resolve {
        /// review id（`IngestPayload` 中的 `review_id`）
        review_id: String,
        /// 解決原因（記錄用）
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum CoverageCommand {
    /// 從 LCOV 文字匯入覆蓋率資料（stdin 或檔案路徑）
    IngestLcov {
        /// LCOV 檔案路徑（省略時從 stdin 讀取）
        payload: Option<PathBuf>,
    },
    /// 從 cobertura JSON 匯入覆蓋率資料（stdin 或檔案路徑）
    IngestJson {
        /// JSON 檔案路徑（省略時從 stdin 讀取）
        payload: Option<PathBuf>,
    },
    /// 查詢某個 canonical node id 的覆蓋率綁定
    Query {
        /// canonical node id（如 `src/a.rs:function:f`）
        node: String,
    },
    /// 列出所有覆蓋率 < 50% 的盲區節點
    Blindspots,
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
            PluginCommand::Probe => run_plugin_probe()?,
            PluginCommand::Reset { plugin_id } => run_plugin_reset(&plugin_id)?,
            PluginCommand::List => run_plugin_list()?,
        },
        Commands::Workspace { command } => match command {
            WorkspaceCommand::List => run_workspace_list()?,
            WorkspaceCommand::Switch { workspace_key } => run_workspace_switch(&workspace_key)?,
            WorkspaceCommand::Status { workspace_key } => run_workspace_status(workspace_key)?,
            WorkspaceCommand::Add { root_path } => run_workspace_add(&root_path)?,
            WorkspaceCommand::Delete { workspace_key } => run_workspace_delete(&workspace_key)?,
        },
        Commands::Handoff { command } => run_handoff(command)?,
        Commands::Opendoc { command } => run_opendoc(command)?,
        Commands::Review { command } => run_review(command)?,
        Commands::Coverage { command } => run_coverage(command)?,
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
    let graph = match load_graph_output(graph_path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("[graphify] Cannot load graph: {e}");
            // Show workspace picker — list registered workspaces
            let Ok(ws_list) = workspace::list_workspaces() else {
                eprintln!("[graphify] No registered workspaces. Run `graphify index` first.");
                return Ok(());
            };
            if ws_list.is_empty() {
                eprintln!("[graphify] No registered workspaces. Run `graphify index` first.");
                return Ok(());
            }
            println!("[graphify] Select a workspace:");
            for (i, ws) in ws_list.iter().enumerate() {
                let marker = if ws.is_active { " ◉" } else { "  " };
                println!("  {}.{marker} {} ({})", i + 1, ws.root_path, ws.workspace_key);
            }
            print!("[graphify] Enter number (1-{}): ", ws_list.len());
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let parsed = match input.trim().parse::<usize>() {
                Ok(n) => n,
                Err(_) if input.trim().is_empty() => 1,
                _ => {
                    eprintln!("[graphify] Invalid selection.");
                    return Ok(());
                }
            };
            if parsed < 1 || parsed > ws_list.len() {
                eprintln!("[graphify] Invalid selection.");
                return Ok(());
            }
            let ws = &ws_list[parsed - 1];
            // Try graph at workspace root
            let derived = std::path::Path::new(&ws.root_path).join("graphify-out/graph.toon");
            load_graph_output(&derived).unwrap_or_else(|_| {
                graphify_core::GraphOutput {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    metadata: graphify_core::GraphMetadata::default(),
                }
            })
        }
    };
    tui::run_tui(graph)?;
    Ok(())
}

/// Manually trigger a `Manual` graph-update event for all bound plugins.
fn run_hooks() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let db = RegistryDb::open(&graphify_registry::registry_db_path())
        .context("opening registry")?;
    let mut host = plugin_host::PluginHost::new();
    register_embedded_plugins(&mut host, &cwd, &db);
    host.broadcast(&GraphUpdateEvent::new(
        derive_workspace_key(&cwd),
        Vec::new(),
        GraphUpdateKind::Manual,
    ));
    println!(
        "[graphify] Broadcast manual graph-update event to {} plugin(s).",
        host.len()
    );
    Ok(())
}

/// Run a passive health probe on all bound plugins and print results.
fn run_plugin_probe() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let workspace_key = derive_workspace_key(&cwd);
    let db = RegistryDb::open(&graphify_registry::registry_db_path())
        .context("opening registry for probe")?;
    let mut host = plugin_host::PluginHost::new();
    register_embedded_plugins(&mut host, &cwd, &db);
    let results = host.probe_all(&db, &workspace_key);
    for (id, status) in &results {
        println!("{id}\t{status}");
    }
    Ok(())
}

/// Reset a quarantined plugin and re-probe it.
fn run_plugin_reset(plugin_id: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let workspace_key = derive_workspace_key(&cwd);
    let db = RegistryDb::open(&graphify_registry::registry_db_path())
        .context("opening registry for reset")?;
    let mut host = plugin_host::PluginHost::new();
    register_embedded_plugins(&mut host, &cwd, &db);
    match host.reset_quarantine(&db, &workspace_key, plugin_id) {
        Some(status) => {
            println!("{plugin_id}\t{status}");
            Ok(())
        }
        None => anyhow::bail!("plugin '{plugin_id}' not found"),
    }
}

/// List all registered plugins with their current health status.
fn run_plugin_list() -> Result<()> {
    let db = RegistryDb::open(&graphify_registry::registry_db_path())
        .context("opening registry for list")?;
    let cwd = std::env::current_dir()?;
    let workspace_key = derive_workspace_key(&cwd);
    let rows = db.list_registrations(&workspace_key)?;
    if rows.is_empty() {
        println!("[graphify] No plugins registered for this workspace.");
        return Ok(());
    }
    for row in &rows {
        println!("{}\t{}", row.plugin_id, row.status);
    }
    Ok(())
}

/// Register all embedded plugins into the host.
///
/// ponytail: kept in one place so `run_hooks`, `probe`, `reset` share the
/// same registration set. Add new plugins here.
fn register_embedded_plugins(
    host: &mut plugin_host::PluginHost,
    cwd: &Path,
    _db: &RegistryDb,
) {
    // handoff
    let mut handoff = RelayPlugin::new().with_registry_path(graphify_registry::registry_db_path());
    handoff.bind_for_cli(cwd);
    host.register(Box::new(handoff));

    // opendoc
    let opendoc = OpendocPlugin::new()
        .with_registry_path(graphify_registry::registry_db_path())
        .bind_for_cli(cwd);
    host.register(Box::new(opendoc));

    // review
    let review = graphify_plugin_review::ReviewPlugin::new()
        .with_registry_path(graphify_registry::registry_db_path())
        .bind_for_cli(cwd);
    host.register(Box::new(review));
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

/// Add a workspace by root path. Key auto-derived; path deduped.
fn run_workspace_add(root_path: &str) -> Result<()> {
    if workspace::add_workspace(root_path)? {
        println!("[graphify] Workspace added at '{root_path}'");
    } else {
        println!("[graphify] Workspace already registered: '{root_path}'");
    }
    Ok(())
}

/// Delete a workspace by key.
fn run_workspace_delete(workspace_key: &str) -> Result<()> {
    workspace::delete_workspace(workspace_key)?;
    println!("[graphify] Workspace deleted: '{workspace_key}'");
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

/// `graphify review` — code-review-graph 橋接（內嵌 graphify-plugin-review）。
///
/// 比照 `handoff`/`opendoc` 整合模式：cwd 合成 `WorkspaceContext`、全域
/// graphify.db 路徑注入。
fn build_review_for_cli() -> Result<graphify_plugin_review::ReviewPlugin> {
    let cwd = std::env::current_dir()?;
    let plugin = graphify_plugin_review::ReviewPlugin::new()
        .with_registry_path(graphify_registry::registry_db_path());
    Ok(plugin.bind_for_cli(&cwd))
}

/// 載入 `graphify-out/graph.toon` 餵給 plugin（line→symbol 解析 + Slice 1 drift
/// detection 都需要最新 graph）。無快取時回傳 None，不視為錯誤。
fn feed_graph_and_drift(plugin: &mut graphify_plugin_review::ReviewPlugin, cwd: &Path) {
    let toon_path = cwd.join("graphify-out/graph.toon");
    if !toon_path.exists() {
        eprintln!(
            "[review] note: 無快取 graphify-out/graph.toon；先跑 `graphify graph` \
             才能做 line→symbol 綁定"
        );
        return;
    }
    match load_graph_output(&toon_path) {
        Ok(g) => {
            let s = graphify_core::to_toon(&g);
            plugin.sync_toon(Some(s.into_bytes()));
            // Slice 1：presence-diff — 已綁定 review 的節點若不在最新 graph，
            // 視為已漂移/刪除而自動銷案。kind 用 Manual：diff 只看 node 集合，
            // 不需要 modified_nodes 清單。
            let event = graphify_core::GraphUpdateEvent::new(
                plugin.get_workspace_key(),
                Vec::new(),
                graphify_core::GraphUpdateKind::Manual,
            );
            plugin.on_graph_updated(&event);
        }
        Err(e) => eprintln!(
            "[review] warning: cached graph unreadable ({e}); \
             ingest 將所有行視為 orphan"
        ),
    }
}

/// `graphify review` — code-review-graph 橋接（內嵌 graphify-plugin-review）。
fn run_review(command: ReviewCommand) -> Result<()> {
    let mut plugin = build_review_for_cli()?;
    let key = plugin.get_workspace_key().to_string();
    let cwd = std::env::current_dir().context("cwd")?;
    match command {
        ReviewCommand::Ingest { payload } => {
            feed_graph_and_drift(&mut plugin, &cwd);
            let (bound, orphan) = plugin
                .review_ingest_file(&payload)
                .map_err(|e| anyhow!("review ingest: {e}"))?;
            println!(
                "[review] {}: {bound} bound, {orphan} orphan lines",
                payload.display()
            );
        }
        ReviewCommand::SearchCrg { base } => {
            feed_graph_and_drift(&mut plugin, &cwd);
            let (node_ids, orphan) = plugin
                .review_search_crg(base.as_deref())
                .map_err(|e| anyhow!("review search-crg: {e}"))?;
            println!(
                "[review] search-crg: {} bound, {orphan} orphan",
                node_ids.len()
            );
            for n in &node_ids {
                println!("  - {n}");
            }
        }
        ReviewCommand::GetContext { node } => {
            feed_graph_and_drift(&mut plugin, &cwd);
            let (node_id, rows) = plugin
                .review_get_context(&key, &node, false)
                .map_err(|e| anyhow!("review get_context: {e}"))?;
            if rows.is_empty() {
                println!("[review] {node_id}: no unresolved reviews");
            } else {
                for r in &rows {
                    println!(
                        "{}\t{}\t{}\t{}",
                        r.id, r.severity, r.category, r.comment
                    );
                }
            }
        }
        ReviewCommand::Resolve { review_id, reason } => {
            feed_graph_and_drift(&mut plugin, &cwd);
            let reason = reason.unwrap_or_default();
            let updated = plugin
                .review_resolve(&key, &review_id, "manual", &reason)
                .map_err(|e| anyhow!("review resolve: {e}"))?;
            if updated {
                if reason.is_empty() {
                    println!("[review] {review_id}: resolved");
                } else {
                    println!("[review] {review_id}: resolved ({reason})");
                }
            } else {
                println!("[review] {review_id}: not found");
            }
        }
    }
    Ok(())
}

/// `graphify coverage` — 測試覆蓋率橋接（內嵌 graphify-plugin-test-coverage）。
///
/// 比照 `review` 整合模式：cwd 合成 `WorkspaceContext`、全域 graphify.db
/// 路徑注入。
fn build_coverage_for_cli() -> Result<CoveragePlugin> {
    let cwd = std::env::current_dir()?;
    let plugin = CoveragePlugin::new()
        .with_registry_path(graphify_registry::registry_db_path());
    Ok(plugin.bind_for_cli(&cwd))
}

/// 餵 graph 給 coverage plugin（line→symbol 解析需要 AST range）。
fn feed_coverage_graph(plugin: &mut CoveragePlugin, cwd: &Path) {
    let toon_path = cwd.join("graphify-out/graph.toon");
    if !toon_path.exists() {
        eprintln!(
            "[coverage] note: 無快取 graphify-out/graph.toon；先跑 `graphify graph` \
             才能做 line→symbol 綁定"
        );
        return;
    }
    match load_graph_output(&toon_path) {
        Ok(g) => {
            let s = graphify_core::to_toon(&g);
            plugin.sync_toon(Some(s.into_bytes()));
        }
        Err(e) => eprintln!(
            "[coverage] warning: cached graph unreadable ({e}); \
             ingest 將所有行視為 file-level"
        ),
    }
}

fn read_input(payload: Option<&PathBuf>) -> Result<String> {
    if let Some(p) = payload {
        std::fs::read_to_string(p).map_err(|e| anyhow!("read {}: {e}", p.display()))
    } else {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    }
}

/// `graphify coverage` — 執行測試覆蓋率命令。
fn run_coverage(command: CoverageCommand) -> Result<()> {
    let mut plugin = build_coverage_for_cli()?;
    let key = plugin.get_workspace_key().to_string();
    let cwd = std::env::current_dir().context("cwd")?;
    match command {
        CoverageCommand::IngestLcov { payload } => {
            feed_coverage_graph(&mut plugin, &cwd);
            let data = read_input(payload.as_ref())?;
            let summary = plugin
                .coverage_ingest_lcov(&data)
                .map_err(|e| anyhow!("coverage ingest-lcov: {e}"))?;
            println!(
                "[coverage] ingest-lcov: {} bound nodes, {} total lines, \
                 {} covered, {} blindspots",
                summary.bound_nodes, summary.total_lines, summary.covered_lines, summary.blindspots,
            );
        }
        CoverageCommand::IngestJson { payload } => {
            feed_coverage_graph(&mut plugin, &cwd);
            let data = read_input(payload.as_ref())?;
            let summary = plugin
                .coverage_ingest_json(&data)
                .map_err(|e| anyhow!("coverage ingest-json: {e}"))?;
            println!(
                "[coverage] ingest-json: {} bound nodes, {} total lines, \
                 {} covered, {} blindspots",
                summary.bound_nodes, summary.total_lines, summary.covered_lines, summary.blindspots,
            );
        }
        CoverageCommand::Query { node } => {
            let db = plugin.db().map_err(|e| anyhow!("coverage db: {e}"))?;
            match db.query_by_node(&key, &node)? {
                Some(b) => println!(
                    "[coverage] {}: {}/{} lines ({:.1}%)",
                    b.canonical_node_id,
                    b.covered_lines,
                    b.total_lines,
                    b.line_rate * 100.0,
                ),
                None => println!("[coverage] {node}: no coverage data"),
            }
        }
        CoverageCommand::Blindspots => {
            let db = plugin.db().map_err(|e| anyhow!("coverage db: {e}"))?;
            let spots = db.query_blindspots(&key)?;
            if spots.is_empty() {
                println!("[coverage] no blindspots (all nodes >= 50% coverage)");
            } else {
                println!("[coverage] {} blindspot(s):", spots.len());
                for b in &spots {
                    println!(
                        "  {}\t{}/{} ({:.1}%)",
                        b.canonical_node_id,
                        b.covered_lines,
                        b.total_lines,
                        b.line_rate * 100.0,
                    );
                }
            }
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
