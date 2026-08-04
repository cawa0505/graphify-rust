# Proposal: Graph Query and Path Traversal (Phase 2)

## Intent
Provide efficient, local, and zero-LLM graph query capability via BFS traversal and shortest path search over the extracted petgraph structure. This allows agents or CLI users to locate callers, dependents, and structural connections in microseconds.

## Scope
- `graphify-core`: Implement BFS traversal (`query`) and shortest path (`path`) algorithms using `petgraph`.
- `graphify-cli`: Connect the subcommands `query` and `path` to the core implementations.
- `graphify-mcp`: Expose the actual tool call implementations for `graphify_query` and `graphify_path` JSON-RPC endpoints.

## Technical Approach
1. **BFS Traversal (`graphify-core/src/graph/query.rs`)**:
   - Given a target node ID and max depth, traverse the graph using BFS.
   - Return a subset of the graph (nodes and edges) within the traversal boundary.
2. **Shortest Path (`graphify-core/src/graph/path.rs`)**:
   - Given a source and target node ID, find the shortest path using `petgraph::algo::dijkstra` or `astar` (or unweighted BFS path).
   - Return the sequence of nodes and edges connecting them.
3. **CLI Integration**:
   - `graphify query <target-node> [--depth N]`
   - `graphify path <source-node> <target-node>`
