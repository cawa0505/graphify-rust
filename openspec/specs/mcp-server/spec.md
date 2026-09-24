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

### Requirement: Tool naming convention

**FROM:** Tools use inconsistent prefixes (`graphify_graph_*`, `graphify_graphify_*`, `graphify_review_*`, `coverageIngest`/`coverageGetContext`/`coverageBlindspots`, `opendocIndex`)

**TO:** Every MCP tool SHALL follow the `graphify_<domain>_<action>` naming convention using snake_case.

The domain segment SHALL be one of: `graph`, `memory`, `workspace`, `coverage`, `opendoc`, `review`, `telemetry`, `relay`, `plugin`.

The action segment SHALL be a verb in snake_case: `query`, `status`, `ingest`, `reindex`, `search`, etc.

Existing tools SHALL be renamed as follows:

| Old name | New name |
|----------|----------|
| `graphify_graph_summary` | `graphify_graph_summary` (unchanged) |
| `graphify_graph_query_node` | `graphify_graph_query_node` (unchanged) |
| `graphify_graph_trace_path` | `graphify_graph_trace_path` (unchanged) |
| `graphify_graph_reindex` | `graphify_graph_reindex` (unchanged) |
| `graphify_graphify_query` | `graphify_graph_query` |
| `graphify_graphify_path` | `graphify_graph_path` |
| `graphify_graphify_notify_plugins` | `graphify_plugin_notify` |
| `graphify_reviewGetContext` | `graphify_review_get_context` |
| `graphify_reviewIngest` | `graphify_review_ingest` |
| `graphify_reviewResolve` | `graphify_review_resolve` |
| `graphify_reviewSearchCrg` | `graphify_review_search_crg` |
| `coverageIngest` | `graphify_coverage_ingest` |
| `coverageGetContext` | `graphify_coverage_get_context` |
| `coverageBlindspots` | `graphify_coverage_blindspots` |
| `opendocIndex` | `graphify_opendoc_index` |
| `opendocGetContext` | `graphify_opendoc_get_context` |
| `opendocAuditDrift` | `graphify_opendoc_audit_drift` |
| `telemetryGetContext` | `graphify_telemetry_get_context` |
| `telemetryIngest` | `graphify_telemetry_ingest` |
| `graphify_relayInit` | `graphify_relay_init` |
| `graphify_relaySave` | `graphify_relay_save` |
| `graphify_relayClose` | `graphify_relay_close` |
| `graphify_relayResume` | `graphify_relay_resume` |
| `graphify_relayStatus` | `graphify_relay_status` |
| `graphify_relaySwitch` | `graphify_relay_switch` |
| `graphify_relayAdd` | `graphify_relay_add` |
| `graphify_memory_query` | `graphify_memory_query` (unchanged) |

The old tool names SHALL be removed. Backward compatibility SHALL NOT be maintained for the old names.

#### Scenario: Agent uses renamed tool

- **WHEN** an agent invokes `graphify_graph_query` with a node ID
- **THEN** the server SHALL return the same result that `graphify_graphify_query` previously returned
- **AND** `graphify_graphify_query` SHALL no longer be available

#### Scenario: Agent uses old name

- **WHEN** an agent invokes `graphify_graphify_query`
- **THEN** the server SHALL return a tool-not-found error

### Requirement: Coverage tool domain

The coverage tools SHALL be moved from the `review` domain to their own `coverage` domain with the `graphify_coverage_*` prefix.

#### Scenario: Coverage tools under new domain

- **WHEN** an agent invokes `graphify_coverage_ingest`
- **THEN** the server SHALL perform the same LCOV ingestion that `coverageIngest` previously performed
- **AND** `coverageIngest` SHALL no longer be available as a tool name

### Requirement: auto-broadcast after index/extract

The server SHALL emit a graph update event automatically after each successful index, extract, or reindex operation, eliminating the need for manual `notify_plugins` calls in normal workflows.

The `notify_plugins` tool SHALL be renamed to `graphify_plugin_notify` and retained as a manual override.

#### Scenario: Reindex auto-broadcasts

- **WHEN** an agent invokes `graphify_graph_reindex` and it succeeds
- **THEN** the server SHALL automatically broadcast a `graph_updated` event to all bound plugins
- **AND** the server SHALL include the auto-broadcast status in the reindex response

### Requirement: workspace_key parameter optional

The `workspace_key` parameter SHALL be optional for tools that require it. When omitted, the server SHALL use the currently active workspace's key.

#### Scenario: Query without workspace_key

- **WHEN** an agent invokes `graphify_memory_query` without `workspace_key`
- **THEN** the server SHALL use the active workspace key
- **AND** return results scoped to the active workspace

### Requirement: Help tool

The server SHALL expose a `graphify_help` tool that returns a categorized list of all available tools with descriptions.

#### Scenario: Help lists all tools

- **WHEN** an agent invokes `graphify_help`
- **THEN** the server SHALL return all registered tools grouped by domain with descriptions

### Requirement: Consistent error protocol

All tools SHALL return a three-state response: success with data, empty result with explanation, or error with descriptive message.

#### Scenario: Memory disabled returns clear message

- **WHEN** an agent invokes `graphify_memory_query` and memory is not configured
- **THEN** the response SHALL explicitly state "memory is not configured/enabled" rather than returning an empty result

## [待討論]

- 是否需要支援多個同時開啟的 codebase 專案切換？
- 是否引入權限機制以限制可 reindex 的目錄範圍？
- `graph_summary` 是否需要提供更細緻的過濾條件（如指定特定的 Module 目錄）？
