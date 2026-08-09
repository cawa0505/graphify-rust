## 1. Core memory query contract

- [x] 1.1 Define storage-agnostic core-memory query input, bounded result output, and explicit unavailable status.
- [x] 1.2 Add native-plugin access through a restricted memory service boundary without exposing Qdrant types, point IDs, credentials, or provider configuration.
- [x] 1.3 Add tests for workspace scoping, result limits, unavailable providers, and rejection of cross-workspace queries.

## 2. Third-party MCP query integration

- [x] 2.1 Add the `graphify_memory_query` MCP tool definition with workspace key, query, and bounded limit inputs.
- [x] 2.2 Route the tool through the same restricted memory service boundary and return explicit unavailable errors.
- [x] 2.3 Add tests proving bounded results, workspace isolation, no write capability, and storage internals are not exposed.

## 3. Plugin-domain memory

- [x] 3.1 Define the versioned domain-record envelope with `format_version`, `workspace_key`, `plugin_id`, `record_id`, `record_kind`, `created_at`, `source_refs`, and payload.
- [x] 3.2 Add system-managed per-plugin collection or namespace derivation and validation; reject plugin-supplied raw collection names and credentials.
- [x] 3.3 Implement domain-memory read/write boundaries that cannot mutate core-memory records.
- [x] 3.4 Add OpenDoc, Review, and Handoff payload schemas with independent versioning.
- [x] 3.5 Add tests for plugin isolation, workspace partitioning, schema evolution, and core-write rejection.

## 4. Handoff and `.toon` contracts

- [x] 4.1 Define reconstructable HandoffSnapshot memory references using workspace key, Graphify node IDs, source paths, and bounded query metadata rather than Qdrant point IDs.
- [x] 4.2 Preserve structural `.toon` context when referenced semantic memory is unavailable or expired.
- [x] 4.3 Add the reserved `metadata.plugin_data.<plugin_id>` container and reject arbitrary plugin top-level fields.
- [x] 4.4 Add serialization and compatibility tests for absent, unknown, and versioned plugin data.

## 5. Provider failure and event integration

- [x] 5.1 Ensure structural AST/Petgraph/`.toon` indexing remains usable when embedding providers are unavailable.
- [x] 5.2 Report semantic memory status as unavailable and avoid hash vectors, null vectors, or false successful empty results.
- [x] 5.3 Keep graph-update notifications as optional plugin refresh triggers without making core-memory synchronization depend on plugin delivery.
- [x] 5.4 Add retry/status tests for provider recovery and isolated plugin notification failure.

## 6. Documentation and future crate boundary

- [x] 6.1 Synchronize the approved memory/plugin boundary into `docs/architecture-memory-plugin.md`, `docs/plugin_system.md`, and `docs/core.md`.
- [x] 6.2 Document the current `graphify-llm` responsibility split and defer `graphify-memory` extraction or rename to a separate migration change.
- [x] 6.3 Record verification evidence and validate the OpenSpec change.
