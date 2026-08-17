## Purpose

Let MCP tools that require workspace routing infer the current workspace automatically, so AI agents can omit the `workspace_key` parameter in the common case.

## ADDED Requirements

### Requirement: Current workspace inference

The server SHALL maintain an active workspace context. When a tool that requires a `workspace_key` parameter is invoked without it, the server SHALL use the active workspace's key as the default.

#### Scenario: Memory query without workspace_key

- **WHEN** an agent invokes `memory_query` with `workspace_key` omitted
- **THEN** the server SHALL use the currently active workspace key
- **AND** the query SHALL return results scoped to that workspace

#### Scenario: Explicit workspace_key overrides active

- **WHEN** an agent passes an explicit `workspace_key` parameter
- **THEN** the server SHALL use the provided key regardless of the active workspace

### Requirement: Active workspace feedback

The server SHALL provide a way for agents to inspect the currently active workspace, its key, and its root path.

#### Scenario: Agent checks active workspace

- **WHEN** an agent invokes a workspace status tool (e.g., `graphify_workspace_status`)
- **THEN** the server SHALL return the active workspace's key, root path, and registration status