# mcp-server 變更 — Delta Spec

## MODIFIED Requirements

（本檔僅新增 requirements；既有 `graph_summary` / `graph_query_node` / `graph_trace_path` / `graph_reindex` 四個工具的 requirements 不變。）

## ADDED Requirements

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
