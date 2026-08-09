# Plugin Domain Memory Specification

## Purpose

Define isolated, workspace-scoped, versioned memory records for plugin-owned
domain knowledge without allowing plugins to mutate Graphify core memory.

## Requirements

### Requirement: Plugin writes stay in domain memory

Plugins MUST NOT write, update, or delete Graphify core-memory records.

Plugin-owned records MUST use plugin-domain memory associated with the registered
`plugin_id` and `workspace_key`.

#### Scenario: Plugin stores a review record

- **WHEN** the Review plugin persists historical review knowledge
- **THEN** the record is stored in Review domain memory and cannot alter a core
  code-node memory record

#### Scenario: Plugin attempts a core-memory write

- **WHEN** a plugin requests a write to Graphify core memory
- **THEN** the system rejects the operation with an explicit authorization or
  capability error

### Requirement: Domain records use a shared envelope

Every plugin-domain record MUST contain a versioned envelope with at least
`format_version`, `workspace_key`, `plugin_id`, `record_id`, `record_kind`,
`created_at`, and `source_refs`.

Each plugin MUST own and version its payload; fields from one plugin payload MUST
NOT become mandatory fields for another plugin.

#### Scenario: OpenDoc record is persisted

- **WHEN** OpenDoc stores a document chunk association
- **THEN** the record contains the shared envelope and an OpenDoc-specific payload
  containing document/chunk identity, document version, and symbol links

#### Scenario: Plugin payload evolves independently

- **WHEN** Review changes its payload schema
- **THEN** OpenDoc and Handoff records remain valid without acquiring Review fields

### Requirement: Domain memory is physically isolated

Each plugin-domain memory store MUST use an independent system-managed collection
or equivalent isolated namespace and MUST NOT share the Graphify core-memory
collection.

Plugins MUST provide an identifier, not raw collection names or database
credentials; the memory service MUST derive and validate storage names.

#### Scenario: Plugin collections are independent

- **WHEN** OpenDoc and Review domain memory are configured
- **THEN** each has an independently rebuildable and removable storage boundary

### Requirement: Workspace identity partitions records

Every plugin-domain read and write MUST be scoped by `workspace_key`, and records
from one workspace MUST NOT be returned for another workspace query.

#### Scenario: Workspace-scoped retrieval

- **WHEN** Handoff queries its domain memory for a workspace
- **THEN** only records carrying the requested workspace key are eligible
