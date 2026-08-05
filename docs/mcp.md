# Graphify MCP Server

`graphify-mcp` exposes the static graph analysis and vector store features to AI assistants (including Cursor, Cline, OpenCode, and Roo Code) using the standardized Model Context Protocol (MCP) over standard input/output.

---

## Exposed MCP Tools

The server implements 4 semantic tools:

1. **`graphify_graph_summary`**
   Returns high-level graph topology metrics, counting total nodes, relationships, and language-specific distributions.

2. **`graphify_graph_query_node`**
   Queries a specific node by ID or label with custom traversal depth, extracting the surrounding local topology context.

3. **`graphify_graph_trace_path`**
   Calculates and traces the shortest calling/dependency path between any two symbols in the codebase.

4. **`graphify_graph_reindex`**
   Triggers a fast single-file incremental AST extraction, re-indexes the target file, and updates the local `.toon` file in seconds.

---

## Serde JSON Compatibility Layers

To handle legacy schema and backward compatibility with the original Python version:
- **Serde Defaults & Aliases**: Fields like `language`, `source_file`, `source_location`, and `confidence` utilize default values or aliases to gracefully process legacy `graph.json` structures.
- **Worry-Free Upgrades**: The server seamlessly starts up and serves clients without crashing even when loading outdated JSON outputs.
