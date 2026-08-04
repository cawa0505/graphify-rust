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

## [待討論]

- 是否需要支援多個同時開啟的 codebase 專案切換？
- 是否引入權限機制以限制可 reindex 的目錄範圍？
- `graph_summary` 是否需要提供更細緻的過濾條件（如指定特定的 Module 目錄）？
