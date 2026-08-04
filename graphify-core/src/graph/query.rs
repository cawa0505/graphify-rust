use crate::types::{Node, Edge, GraphOutput, NodeId};
use petgraph::graph::DiGraph;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashSet, VecDeque, HashMap};
use petgraph::graph::NodeIndex;
use anyhow::{Result, anyhow};

pub fn query_bfs(
    graph: &DiGraph<Node, Edge>,
    node_map: &HashMap<NodeId, NodeIndex>,
    start_node: &NodeId,
    max_depth: usize,
) -> Result<GraphOutput> {
    let start_idx = node_map.get(start_node)
        .ok_or_else(|| anyhow!("Node not found: {:?}", start_node))?;

    let mut visited_nodes = HashSet::new();
    let mut visited_edges = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back((*start_idx, 0));
    visited_nodes.insert(*start_idx);

    while let Some((curr_idx, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        // Get outbound edges
        for edge_ref in graph.edges_directed(curr_idx, Direction::Outgoing) {
            let target_idx = edge_ref.target();

            visited_edges.insert(edge_ref.id());

            if !visited_nodes.contains(&target_idx) {
                visited_nodes.insert(target_idx);
                queue.push_back((target_idx, depth + 1));
            }
        }

        // Get inbound edges
        for edge_ref in graph.edges_directed(curr_idx, Direction::Incoming) {
            let source_idx = edge_ref.source();

            visited_edges.insert(edge_ref.id());

            if !visited_nodes.contains(&source_idx) {
                visited_nodes.insert(source_idx);
                queue.push_back((source_idx, depth + 1));
            }
        }
    }

    let result_nodes: Vec<Node> = visited_nodes
        .into_iter()
        .map(|idx| graph[idx].clone())
        .collect();

    let result_edges: Vec<Edge> = visited_edges
        .into_iter()
        .map(|idx| graph[idx].clone())
        .collect();

    let total_nodes = result_nodes.len();
    let total_edges = result_edges.len();

    Ok(GraphOutput {
        nodes: result_nodes,
        edges: result_edges,
        metadata: crate::types::GraphMetadata {
            version: "1.0".to_string(),
            generated_at: "0".to_string(),
            total_nodes,
            total_edges,
            languages: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
        },
    })
}
