use crate::types::{Node, Edge, NodeId};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use petgraph::algo::astar;
use anyhow::{Result, anyhow};

pub fn find_shortest_path(
    graph: &DiGraph<Node, Edge>,
    node_map: &HashMap<NodeId, NodeIndex>,
    start_node: &NodeId,
    end_node: &NodeId,
) -> Result<Option<Vec<String>>> {
    let start_idx = node_map.get(start_node)
        .ok_or_else(|| anyhow!("Source node not found: {:?}", start_node))?;
    let end_idx = node_map.get(end_node)
        .ok_or_else(|| anyhow!("Target node not found: {:?}", end_node))?;

    // Dijkstra/A* pathfinding with uniform edge weights (weight = 1)
    let path_opt = astar(
        graph,
        *start_idx,
        |finish| finish == *end_idx,
        |_| 1, // Uniform edge cost
        |_| 0, // Admissible heuristic (0 makes it equivalent to Dijkstra)
    );

    match path_opt {
        Some((_cost, path_indices)) => {
            let path_labels = path_indices
                .into_iter()
                .map(|idx| graph[idx].id.0.clone())
                .collect();
            Ok(Some(path_labels))
        }
        None => Ok(None),
    }
}
