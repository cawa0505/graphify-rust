## Context

`graphify-llm` currently contains both Qdrant-backed long-term memory and the
LLM chat/provider pipeline. The plugin system already has `workspace_key`, graph
update notifications, MCP subprocess routing, and `.toon` serialization, but
none of those contracts define memory access or plugin-owned records.

See `proposal.md` and the three capability specs for the observable behavior.
This design keeps `graphify-core` free of memory-storage and network dependencies.

## Goals / Non-Goals

**Goals:**

- Expose one bounded core-memory query abstraction to native plugins.
- Expose the equivalent bounded capability through `graphify_memory_query` for
  third-party MCP plugins.
- Keep core-memory writes inside the indexing pipeline.
- Isolate plugin-domain memory by system-managed plugin collection/namespace and
  `workspace_key`.
- Define a stable shared record envelope and reserved `.toon` plugin container.
- Preserve structural indexing when semantic providers are unavailable.

**Non-Goals:**

- Renaming the `graphify-llm` crate in this change.
- Moving the three plugin crates into GraphifyRust.
- Letting plugins write or mutate core-memory records.
- Defining a universal payload for OpenDoc, Review, and Handoff.
- Adding a new dependency to `graphify-core`.

## Decisions

### 1. Memory service boundary

The memory service owns provider selection, embedding generation, storage,
collection naming, and credentials. A native-plugin query adapter exposes only a
workspace-scoped query input and bounded semantic results. The result contract
uses Graphify node/source identifiers and bounded text/context, not Qdrant types.

Third-party plugins use `graphify_memory_query` through graphify-mcp. The MCP
handler validates `workspace_key`, query size, and result limit before delegating
to the same service boundary. It returns a distinct unavailable status when
semantic memory cannot be queried.

Alternative rejected: exposing `QdrantMemoryStore` directly. That would couple
plugins to transport, payload, collection, vector dimension, and credentials.

### 2. Write ownership and isolation

The indexing pipeline remains the only writer of core memory. Plugin records use
one independently managed collection or equivalent namespace per registered
plugin, with `workspace_key` as a mandatory partition field. The service derives
and validates physical names from `plugin_id`; plugins cannot provide raw names
or credentials.

Alternative rejected: a shared collection with a `plugin_id` filter. Separate
collections permit independent schemas, embedding models, rebuilds, and deletion.

### 3. Record envelope and `.toon` enrichment

Plugin-domain records use a versioned envelope:

```text
format_version, workspace_key, plugin_id, record_id, record_kind,
created_at, source_refs, payload
```

The payload is plugin-owned and versioned independently. `.toon` enrichment is
optional and limited to `metadata.plugin_data.<plugin_id>`; unknown entries are
ignored by core consumers. Core nodes and edges remain unchanged.

### 4. Handoff references

Handoff snapshots store reconstructable references: `workspace_key`, stable
Graphify node IDs, source paths, and bounded query metadata. They do not treat
Qdrant point IDs as durable public identifiers. Structural `.toon` context is the
fallback when semantic records are expired, migrated, deleted, or unavailable.

### 5. Provider failure behavior

Structural extraction and `.toon` output proceed independently of embedding
availability. Memory synchronization reports an explicit unavailable state and
does not write hash or null vectors. A later explicit synchronization retries
semantic indexing.

### 6. `graphify-llm` to `graphify-memory` transition

The first implementation introduces the boundary without a crate rename. Before
any rename, inventory and separate chat/provider APIs from memory APIs, then
perform a compatibility-preserving extraction or rename in a separate change.

Alternative rejected: immediate whole-crate rename. It would mix naming cleanup,
API migration, and plugin integration, making rollback and verification harder.

## Risks / Trade-offs

- [Risk] A native adapter may grow into an unrestricted storage façade. → Keep
  result/input types storage-agnostic and test that Qdrant types do not cross the
  public boundary.
- [Risk] Separate plugin collections increase operational objects. → Derive names
  centrally and create collections lazily; provide explicit rebuild/delete APIs.
- [Risk] Semantic results become stale after graph changes. → Use graph-update
  notifications as plugin refresh triggers, while keeping core sync independent.
- [Risk] Handoff references may no longer resolve. → Persist structural `.toon`
  context and treat semantic lookup as optional enrichment.
- [Risk] Existing `graphify-llm` callers are affected by premature extraction.
  → Keep the current crate/API during this change and require a separate migration
  spec for `graphify-memory`.

## Migration Plan

1. Add storage-agnostic memory query and plugin-domain contracts without changing
   existing core indexing behavior.
2. Add core-memory query exposure to native plugin integration and the MCP tool.
3. Add versioned domain records, isolated storage, and `.toon` `plugin_data`.
4. Verify provider-unavailable structural fallback and workspace isolation.
5. In a later change, split or rename memory APIs after the chat/provider inventory
   is complete; retain a compatibility re-export or migration path as required.

Rollback is achieved by disabling the new plugin memory adapters/tools. Core
structural extraction and existing core-memory indexing remain independently
usable.
