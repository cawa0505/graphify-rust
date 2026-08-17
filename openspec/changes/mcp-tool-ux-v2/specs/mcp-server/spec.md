## MODIFIED Requirements

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