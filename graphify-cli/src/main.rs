// ponytail: allow missing errors doc as these are binary CLI entry points
#![allow(clippy::missing_errors_doc)]
// ponytail: allow collapsible_if for nested directory filtering checks
#![allow(clippy::collapsible_if)]

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use graphify_core::{
    build_graph, extract_file, find_shortest_path, query_bfs, GraphMetadata, GraphOutput,
    Node, NodeId, Edge,
};
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
        /// Output path for the generated graph JSON
        #[arg(short, long, default_value = "graphify-out/graph.json")]
        output: PathBuf,
    },
    /// Query a node in the graph using BFS traversal
    Query {
        /// Target Node ID or label
        target: String,
        /// Maximum traversal depth
        #[arg(short, long, default_value_t = 1)]
        depth: usize,
        /// Path to the graph JSON file
        #[arg(short, long, default_value = "graphify-out/graph.json")]
        graph: PathBuf,
    },
    /// Find the shortest path between two nodes in the graph
    Path {
        /// Source Node ID or label
        source: String,
        /// Target Node ID or label
        target: String,
        /// Path to the graph JSON file
        #[arg(short, long, default_value = "graphify-out/graph.json")]
        graph: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Extract { path, output } => run_extract(&path, &output)?,
        Commands::Query { target, depth, graph } => run_query(&target, depth, &graph)?,
        Commands::Path { source, target, graph } => run_path(&source, &target, &graph)?,
    }
    Ok(())
}

fn run_extract(input_path: &Path, output_path: &Path) -> Result<()> {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut languages = std::collections::HashSet::new();

    if input_path.is_file() {
        if let Some(lang) = get_lang(input_path) {
            languages.insert(lang);
        }
        let res = extract_file(input_path)?;
        nodes.extend(res.nodes);
        edges.extend(res.edges);
    } else {
        collect_dir(input_path, &mut nodes, &mut edges, &mut languages)?;
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

    let json_str = serde_json::to_string_pretty(&graph_out)?;
    std::fs::write(output_path, json_str)?;
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

fn collect_dir(
    dir: &Path,
    nodes: &mut Vec<Node>,
    edges: &mut Vec<Edge>,
    languages: &mut std::collections::HashSet<String>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
            }
            collect_dir(&path, nodes, edges, languages)?;
        } else if let Some(lang) = get_lang(&path) {
            if let Ok(res) = extract_file(&path) {
                nodes.extend(res.nodes);
                edges.extend(res.edges);
                languages.insert(lang);
            }
        }
    }
    Ok(())
}

fn load_graph_output(path: &Path) -> Result<GraphOutput> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read graph file: {}", path.display()))?;
    let output: GraphOutput = serde_json::from_str(&content)?;
    Ok(output)
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
