## Purpose

Provide bounded, workspace-scoped semantic access to Graphify core memory for
native plugins and third-party MCP plugins without exposing storage internals.

## ADDED Requirements

### Requirement: Native plugin core-memory query

The system MUST provide native plugins with a restricted, storage-agnostic query
operation against Graphify core memory.

The operation MUST scope every query to an explicit `workspace_key`, enforce a
bounded result limit, and return stable Graphify identifiers plus bounded context.
It MUST NOT expose Qdrant collection names, payload internals, point IDs,
credentials, or embedding-provider configuration.

#### Scenario: Native plugin queries workspace memory

- **WHEN** a bound native plugin submits a valid query with its `workspace_key`
- **THEN** the system returns bounded matching context and stable Graphify identifiers
  from that workspace only

#### Scenario: Query cannot cross workspace boundaries

- **WHEN** a query requests a workspace different from the bound plugin context
- **THEN** the system rejects the query without returning records from either
  unrequested workspace

### Requirement: Third-party MCP memory query

The MCP server MUST expose a restricted `graphify_memory_query` tool for
third-party plugins and clients that need core semantic context.

The tool MUST apply `workspace_key` scoping and bounded result limits, and MUST
return an explicit error when semantic memory is unavailable. The tool MUST NOT
permit writes or expose storage internals.

#### Scenario: MCP plugin performs a bounded query

- **WHEN** a third-party plugin calls `graphify_memory_query` with a valid
  workspace key and query
- **THEN** the server returns bounded matching context for that workspace

#### Scenario: Semantic memory is unavailable

- **WHEN** the configured embedding or semantic-memory provider is unavailable
- **THEN** `graphify_memory_query` returns an explicit unavailable status rather
  than an empty successful result

### Requirement: Core memory remains indexing-owned

Core memory synchronization MUST remain owned by the Graphify indexing pipeline
and MUST NOT depend on any plugin being loaded or receiving a graph-update event.

#### Scenario: Indexing works without plugins

- **WHEN** a workspace is indexed with no plugins configured
- **THEN** structural indexing and core-memory synchronization follow their own
  configured behavior without requiring plugin startup

#### Scenario: Plugin event delivery fails

- **WHEN** a graph-update notification cannot be delivered to a plugin after an
  indexing operation
- **THEN** the core indexing result and core-memory result are not rolled back
  solely because of that plugin failure
