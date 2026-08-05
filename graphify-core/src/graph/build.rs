use crate::types::{Node, Edge, NodeId};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use anyhow::Result;

pub fn build_graph(nodes: &[Node], edges: &[Edge]) -> Result<(DiGraph<Node, Edge>, HashMap<NodeId, NodeIndex>)> {
    let mut graph = DiGraph::<Node, Edge>::with_capacity(nodes.len(), edges.len());
    let mut node_map = HashMap::new();

    // 1. Add all nodes
    for node in nodes {
        let idx = graph.add_node(node.clone());
        node_map.insert(node.id.clone(), idx);
    }

    // 2. Add all edges
    for edge in edges {
        if let (Some(&source_idx), Some(&target_idx)) = (node_map.get(&edge.source), node_map.get(&edge.target)) {
            graph.add_edge(source_idx, target_idx, edge.clone());
        }
    }

    Ok((graph, node_map))
}
