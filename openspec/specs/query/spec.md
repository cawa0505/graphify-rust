# query

## Purpose

Define the behavioral contracts for traversing and querying the extracted dependency graph, enabling efficient local static analysis context retrieval for AI agents and CLI users.

## Requirements

### Requirement: BFS Traversal (Query)

The system MUST support traversing the knowledge graph using a Breadth-First Search (BFS) algorithm starting from a target node up to a specified maximum depth.

#### Scenario: BFS Traversal from a target function

- GIVEN a loaded knowledge graph with nodes `A`, `B`, and `C` where `A -> B -> C` (e.g., A calls B, B calls C)
- WHEN a query is requested for node `A` with max depth 1
- THEN the result SHALL contain nodes `A` and `B` and the edge `A -> B`
- AND the result SHALL NOT contain node `C` or the edge `B -> C`.

### Requirement: Shortest Path Detection

The system MUST support finding the shortest directed path between a source node and a target node.

#### Scenario: Shortest Path between two functions

- GIVEN a loaded knowledge graph with nodes `A -> B -> C` and a direct path `A -> C`
- WHEN the shortest path is requested from `A` to `C`
- THEN the result SHALL return the direct path `[A -> C]` instead of the longer path `[A -> B, B -> C]`.
