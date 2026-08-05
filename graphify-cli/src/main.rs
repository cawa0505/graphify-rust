// ponytail: allow missing errors doc as these are binary CLI entry points
#![allow(clippy::missing_errors_doc)]
// ponytail: allow collapsible_if for nested directory filtering checks
#![allow(clippy::collapsible_if)]

pub mod skill;
pub mod tui;
pub mod ui;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use graphify_core::{
    build_graph, extract_file, find_shortest_path, query_bfs, GraphMetadata, GraphOutput,
    Node, NodeId, ExtractionResult,
};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Extract { path, output, concurrency } => run_extract(&path, &output, concurrency)?,
        Commands::Query { target, depth, graph } => run_query(&target, depth, &graph)?,
        Commands::Path { source, target, graph } => run_path(&source, &target, &graph)?,
        Commands::InstallSkill { global, dir } => skill::install_skill(global, dir)?,
        Commands::Tui { graph } => run_tui(&graph)?,
        Commands::Index { path, config, output, force } => run_index(&path, config.as_deref(), output.as_deref(), force)?,
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

fn load_graph_output(path: &Path) -> Result<GraphOutput> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read graph file: {}", path.display()))?;
    if path.extension().and_then(|e| e.to_str()) == Some("toon") {
        graphify_core::from_toon(&content)
    } else {
        let output: GraphOutput = serde_json::from_str(&content)?;
        Ok(output)
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
    let target_node_id = find_node(&graph_out.nodes, target)
        .ok_or_else(|| anyhow!("Node not found: {target}"))?;

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

fn run_index(path: &Path, config_path: Option<&Path>, output_path: Option<&Path>, force: bool) -> Result<()> {
    // 1. 載入 LLM & Memory 設定
    let config = if let Some(cfg_p) = config_path {
        graphify_llm::config::LLMConfig::load_from_file(cfg_p.to_str().unwrap_or(""))?
    } else {
        graphify_llm::config::LLMConfig::load_from_file("")?
    };

    // 2. 獲取 GraphOutput
    let graph_out = if path.is_file() && (path.extension().and_then(|e| e.to_str()) == Some("toon") || path.extension().and_then(|e| e.to_str()) == Some("json")) {
        load_graph_output(path)?
    } else {
        // 如果是目錄，我們需要先進行 Extract 提取
        let out_p = output_path.map_or_else(|| PathBuf::from("graphify-out/graph.toon"), Path::to_path_buf);
        println!("Extracting codebase graph first before indexing...");
        run_extract(path, &out_p, None)?;
        load_graph_output(&out_p)?
    };

    if graph_out.nodes.is_empty() {
        println!("No nodes found to index!");
        return Ok(());
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // 3. 如果 Force 則先刪除 Qdrant 集合
    if force {
        let qdrant_config = &config.memory.long_term.qdrant;
        let delete_url = format!(
            "{}/collections/{}",
            qdrant_config.url.trim_end_matches('/'),
            qdrant_config.collection
        );
        let client = reqwest::Client::new();
        let mut req = client.delete(&delete_url);
        if let Some(ref key) = qdrant_config.api_key {
            req = req.header("api-key", key);
        }
        let _ = rt.block_on(async { req.send().await })?;
        println!("Deleted existing Qdrant collection '{}' for force-recreation.", qdrant_config.collection);
    }

    // 4. 建立 Qdrant 實體與非同步執行
    println!("Connecting to Ollama and Qdrant to generate embeddings and index {} nodes...", graph_out.nodes.len());
    let store = graphify_llm::memory::QdrantMemoryStore::new(config.clone());

    rt.block_on(async {
        store.ensure_collection().await?;
        println!("Uploading nodes to Qdrant collection '{}'...", config.memory.long_term.qdrant.collection);
        store.upsert_nodes(&graph_out.nodes).await?;
        Ok::<(), anyhow::Error>(())
    })?;

    println!("Successfully indexed codebase graph into Qdrant store!");
    Ok(())
}
