## Why

Graphify already contains a Qdrant-backed long-term memory implementation in
`graphify-llm`, but the plugin architecture currently defines graph events and
tool routing without a formal memory boundary. The three planned first-party
native plugins—OpenDoc, Review, and Handoff—need semantic context and domain
memory without coupling themselves to Qdrant internals or corrupting core code
memory.

## What Changes

- Define Graphify core memory as an indexing-owned capability independent from
  optional plugins.
- Define a restricted, storage-agnostic core-memory query API for native plugins.
- Define the restricted `graphify_memory_query` MCP tool for third-party plugins.
- Prohibit plugins from writing Graphify core memory; plugin-specific records use
  isolated domain-memory collections.
- Define a versioned shared envelope with plugin-specific payloads for OpenDoc,
  Review, and Handoff records.
- Define reconstructable memory references for `HandoffSnapshot` instead of
  durable Qdrant point IDs.
- Define `workspace_key` scoping across core memory and plugin-domain memory.
- Define the unavailable-provider behavior: preserve AST/Petgraph/`.toon`
  structural output and report semantic memory as unavailable.
- Reserve `.toon` `plugin_data` for namespaced optional plugin metadata.
- Record the staged responsibility split from `graphify-llm` toward a future
  `graphify-memory` crate without performing the rename in this change.

## Capabilities

### New Capabilities

- `memory-plugin-query`: Restricted native and MCP queries against core memory.
- `plugin-domain-memory`: Isolated, versioned memory records owned by plugins.
- `toon-plugin-data`: Reserved `.toon` container for optional plugin metadata.

### Modified Capabilities

None. The existing plugin-event behavior is an integration constraint for this
change, not a requirement modification to the unarchived `plugin-events-v1`
change.

## Impact

- Affects the public plugin contracts, MCP tool surface, `.toon` metadata
  contract, workspace scoping, and memory service APIs.
- Affects `graphify-llm` responsibility boundaries and may later require a
  **BREAKING** crate rename or extraction to `graphify-memory`; that migration
  is explicitly outside this proposal's implementation phase.
- Native plugin crates will consume the restricted memory API; third-party MCP
  plugins will use the restricted query tool.
- No new dependency is required in `graphify-core`.
