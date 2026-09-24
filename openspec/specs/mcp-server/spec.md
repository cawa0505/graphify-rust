# MCP Protocol Server Specification

## Purpose

Define the MCP (Model Context Protocol) Server interface for GraphifyRust, exposing precise tools to minimize token consumption and enable interactive codebase querying.

## Requirements

### Requirement: Graph Summary Tool (graph_summary)
The server SHALL expose a `graph_summary` tool returning a high-level architectural map of the project to allow quick orientation with minimal token cost.

#### Scenario: Requesting project summary
- GIVEN a loaded knowledge graph of a multi-module codebase
- WHEN the client invokes `graph_summary`
- THEN the server SHALL return only the top-level module topology, core structs, and classes, omitting deep method details and local edge lists
- AND the payload size SHALL be optimized to consume approximately 200 tokens

### Requirement: Query Node Tool (graph_query_node)
The server SHALL expose a `graph_query_node` tool that acts as a local probe, returning details of a single node and its immediate neighbors (1-hop).

#### Scenario: Inspecting a specific function
- GIVEN a loaded knowledge graph containing node `fn_process_user`
- WHEN the client invokes `graph_query_node` with `node_id: "fn_process_user"` and `depth: 1`
- THEN the server SHALL return the definition summary, docstrings, and immediate callers/callees (1-hop adjacency list)
- AND the payload size SHALL be optimized to consume approximately 100 tokens

### Requirement: Trace Path Tool (graph_trace_path)
The server SHALL expose a `graph_trace_path` tool returning the shortest call path or dependency chain between two nodes.

#### Scenario: Tracing dependency between struct and database
- GIVEN a loaded knowledge graph containing `UserStruct` and `DatabaseQuery`
- WHEN the client invokes `graph_trace_path` with `from: "UserStruct"` and `to: "DatabaseQuery"`
- THEN the server SHALL compute the shortest path using petgraph's Dijkstra/A* algorithm within 1 millisecond
- AND return the ordered path: `UserStruct -> process_user() -> validate_user() -> DatabaseQuery`
- AND the payload size SHALL be optimized to consume approximately 50 tokens

### Requirement: Graph Reindex Tool (graph_reindex)
The server SHALL expose a `graph_reindex` tool to allow incremental, background updates to the graph whenever a file is modified.

#### Scenario: Incremental update of a modified file
- GIVEN a modified file `src/user.rs`
- WHEN the client invokes `graph_reindex` with `file_path: "src/user.rs"`
- THEN the server SHALL run the tree-sitter AST parser on `src/user.rs` only, update the in-memory petgraph model, and write to `graph.json` in milliseconds
- AND return the number of updated nodes and status


### Requirement: Compose Read Tool (graphify_compose_read)
The server SHALL expose a `graphify_compose_read` tool that reads a specified Assembly Manifest file and returns its workspaces list and relations list as structured content.

#### Scenario: Reading an existing manifest
- GIVEN an Assembly Manifest file at `compose/ecommerce.yaml`
- WHEN the client invokes `graphify_compose_read` with `manifest: "compose/ecommerce.yaml"`
- THEN the server SHALL return the workspaces (id + path) and relations (from + type + to) as structured JSON
- AND the response SHALL fit within a compact token budget (manifest summary, not file dump)

### Requirement: Compose Write Tool (graphify_compose_write)
The server SHALL expose a `graphify_compose_write` tool that creates or updates relations in a specified Assembly Manifest. Before writing, the server SHALL run the same reference validation as `compose validate` (workspace paths resolved, relation endpoints exist, node-granularity references resolved against each workspace's graph). Validation failure SHALL reject the write and leave the manifest file byte-identical.

#### Scenario: Agent writes a valid semantic relation
- GIVEN an agent intends to record that workspace `agentshop` uses workspace `nexusledger`
- WHEN the client invokes `graphify_compose_write` with a new relation whose endpoints both resolve
- THEN the server SHALL append the relation to the manifest
- AND return the post-write relations summary

#### Scenario: Agent writes an invalid relation
- GIVEN the agent submits a relation referencing node `ghost_ws::no_such_fn`
- WHEN the client invokes `graphify_compose_write`
- THEN the server SHALL reject the write with the failing reference string
- AND the manifest file SHALL remain unchanged

### Requirement: Compose Render Tool (graphify_compose_render)
The server SHALL expose a `graphify_compose_render` tool that renders the unified graph of a specified Assembly Manifest to ASCII (default) or SVG, via the same box-of-rain projection used by the CLI. If `npx` or box-of-rain is unavailable, the tool SHALL return an explicit error containing the stderr summary, never fabricated output.

#### Scenario: Agent requests an architecture diagram
- GIVEN a valid manifest with resolvable workspaces
- WHEN the client invokes `graphify_compose_render` with `manifest: "compose/ecommerce.yaml"`
- THEN the server SHALL return the ASCII diagram text with workspace container boxes and labeled cross-workspace arrows
## [待討論]

- 是否需要支援多個同時開啟的 codebase 專案切換？
- 是否引入權限機制以限制可 reindex 的目錄範圍？
- `graph_summary` 是否需要提供更細緻的過濾條件（如指定特定的 Module 目錄）？
