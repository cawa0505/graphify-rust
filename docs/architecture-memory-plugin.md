# Memory Subsystem and Plugin Boundary Architecture

## Overview

This document describes the architecture of Graphify's long-term memory subsystem and its boundary with the plugin system.
It clarifies responsibilities, data flow, and extension points to maintain a clean separation of concerns.

## Memory Subsystem (graphify-llm)

The memory subsystem is responsible for:
- Storing and retrieving semantic representations of code entities (nodes, edges, symbols)
- Managing embeddings via local (Ollama/FastEmbed) or remote providers
- Incremental synchronization of the code graph with a vector database (Qdrant)
- Providing query interfaces for similarity search and context retrieval

Key components:
- `QdrantMemoryStore`: Handles connection to Qdrant, collection management, and CRUD operations for nodes.
- Embedding generation: Converts text snippets (docstrings, symbol names, file content) into vectors.
- Incremental sync: Uses file hashes to determine which nodes need re-embedding and upsertion.
- Query API: `MemorySearcher` trait (`query_restricted` / `query_memory`) for retrieving relevant code context based on vector similarity, scoped by `workspace_key` with bounded limits.

The memory subsystem is **core to Graphify's functionality** and must remain operational even when no plugins are present.

## Plugin System

Plugins are external entities that extend Graphify's capabilities via the MCP protocol or the in-process `GraphifyPlugin` trait.
They can:
- Receive graph update notifications (`notifications/graph_updated`)
- Provide additional tools via `tools/list` and `tools/call`
- Access the current workspace context (including `workspace_key`)
- Perform domain-specific analysis on the code graph

Plugins are **optional extensions** and should not be required for core Graphify operations.

## Boundary Between Memory and Plugins

### Data Flow
1. Core extraction (`graphify-core`) produces the in-memory graph and `.toon` file.
2. The memory subsystem (`graphify-llm`) subscribes to graph changes (via CLI indexing or MCP reindex) and:
   - Generates embeddings for new/changed nodes
   - Upserts nodes into Qdrant
3. Plugins receive `graph_updated` notifications (with `workspace_key` and event kind) and may:
   - Query the memory subsystem for semantic context
   - Perform their own analysis and store results in their own storage
   - Enrich the graph with additional metadata (if designed to do so)

### Access Patterns
- **Plugins reading from memory**: Should occur via formal MCP tools (e.g., a `graphify_memory_search` tool) or, for in-process plugins, via direct API calls to `graphify-llm` (not recommended for third-party plugins to avoid tight coupling).
- **Plugins writing to memory**: Generally discouraged to prevent pollution of the core memory index. Plugins should use their own storage for domain-specific data.
- **Memory subsystem calling plugins**: Only occurs via broadcast of `graph_updated` events; the memory subsystem does not invoke plugin logic directly.

## Native Plugin Memory Use Cases

The three first-party native Rust plugins should use core memory as a bounded
semantic context layer. They must not reimplement the core indexing pipeline.
These are architecture scenarios, not claims that the plugin crates are already
implemented in this repository.

### `graphify-plugin-opendoc`

OpenDoc bridges non-code documents such as `.xlsx`, `.pdf`, and `.docx` with code
symbols:

```text
document chunk → core memory search → candidate symbol → graph/.toon traversal
```

This enables document-to-code traceability, stale-requirement detection, and
semantic links where a document does not mention an explicit symbol name.
Document chunks and external identifiers remain OpenDoc domain data; core memory
returns bounded context rather than exposing Qdrant internals.

### `graphify-plugin-review`

Review combines changed-symbol subgraphs with historical review knowledge:

```text
git diff → impacted subgraph → core memory search → review findings
```

Historical review records are plugin-domain memory, partitioned by `workspace_key`
and attributed to the review plugin. Core memory provides code structure and
semantic context; it does not silently become a store for arbitrary review history.

### `graphify-plugin-handoff`

Handoff stores and restores task context across sessions or agents:

```text
focused subgraph + pinned symbols + task state + memory references → HandoffSnapshot
```

Snapshots may contain references to core-memory results, but must not assume that a
Qdrant point ID is a durable public API. Restoration must tolerate expired or
unavailable memory entries and still restore structural `.toon` context.

### Shared native-plugin contract

All three plugins use `workspace_key` as the Graphify routing and partition key.
They may receive `notifications/graph_updated` and decide whether to refresh their
own domain memory. Core memory synchronization remains owned by the Graphify
indexing pipeline and must not depend on any plugin being loaded.

The following require an approved OpenSpec before implementation:

- `[待討論]` whether any plugin may write core memory;

### Identity and Routing
- The `workspace_key` (a stable hash of the canonical workspace root path) serves as the routing key for:
  - Correlating graph extracts with memory entries
  - Plugin context via `WorkspaceContext::workspace_key`
  - Notification payloads to plugins
- It is **not** intended to be used as a Qdrant collection name or as a plugin workspace identifier without explicit mapping.

## [待討論] Decisions

The following items require explicit resolution before implementation:

1. **Plugin read access to core memory — Decided: restricted APIs**
   - Native plugins query core memory through a restricted, storage-agnostic API.
     It returns bounded semantic context and stable Graphify identifiers without
     exposing Qdrant collections, payload schema, point IDs, credentials, or
     provider configuration.
   - Third-party MCP plugins query through a restricted `graphify_memory_query`
     tool. It enforces `workspace_key` scoping and bounded results, and exposes
     neither Qdrant internals nor write access.

2. **Plugin write access to core memory — Decided: prohibited**
   - Plugins must not insert or update entries in the Graphify core memory index.
   - The core indexing pipeline is the sole writer of code-graph memory.
   - OpenDoc document chunks, Review history, and Handoff snapshots belong to
     plugin-domain memory and must use plugin-owned storage or a future explicitly
     defined domain-memory API.

3. **Handoff memory references — Decided: reconstructable references**
   - `HandoffSnapshot` must not use a Qdrant point ID as its durable reference.
   - It stores `workspace_key`, stable Graphify node IDs, source paths, and the
     bounded query metadata needed to reconstruct memory context.
   - Restoration must tolerate collection migration, embedding-model changes,
     deleted memory entries, and unavailable semantic search. Structural `.toon`
     context remains restorable when memory lookup fails.

4. **Plugin-domain record schemas — Decided: shared envelope, typed payloads**
   - Every plugin-domain record uses a versioned envelope containing at least
     `format_version`, `workspace_key`, `plugin_id`, `record_id`, `record_kind`,
     `created_at`, and `source_refs`.
   - Each plugin owns a typed payload within that envelope:
     - OpenDoc: document and chunk identity, document version, and symbol links.
     - Review: change and review identity, affected symbols, finding, and resolution.
     - Handoff: task state, pinned symbols, structural `.toon` context, and a
       reconstructable memory query.
   - Payload evolution is versioned; one plugin's payload fields must not become
     mandatory for another plugin.

5. **Embedding provider fallback — Decided: preserve structural indexing**
   - When no embedding provider is available, Graphify retains the AST, Petgraph,
     and `.toon` structural output and reports semantic memory as unavailable.
   - It must not create hash vectors or null-vector records as semantic substitutes.
   - Semantic memory synchronization can be retried after a provider becomes
     available; plugin memory queries must return an explicit unavailable status,
     not an empty result that looks like a successful search.

6. **Memory namespace isolation — Decided: one collection per plugin**
   - Each plugin-domain memory uses an independent collection and never shares the
     Graphify core-memory collection.
   - The Graphify memory service derives and validates the collection name from
     `plugin_id`; plugins do not supply raw collection names or database credentials.
   - Collections may evolve, rebuild, or be removed independently so OpenDoc,
     Review, and Handoff payload schemas and embedding models do not constrain one
     another.
   - Connection policy and authorization remain centrally managed even though the
     data collections are isolated.

7. **`.toon` enrichment by plugins — Decided: reserved `plugin_data` container**
   - Plugins may add optional metadata only inside the reserved `plugin_data`
     container; they must not add arbitrary top-level fields.
   - Entries are partitioned by the registered `plugin_id`, for example:

     ```toon
     metadata:
       format_version: "1"
       workspace_key: "..."
       plugin_data:
         opendoc: { ... }
         review: { ... }
         handoff: { ... }
     ```

   - Core consumers must tolerate absent or unknown plugin entries. Each plugin
     owns its payload schema and versioning; plugin data must not change the
     meaning of core nodes or edges.

## Recommendations

- Keep `graphify-llm` unchanged until the responsibility split is specified and verified. Extract or rename the memory portion to `graphify-memory` only after separating the chat/provider pipeline contract.
- Keep the plugin system focused on providing extensible tools and event handling, not on replacing core memory.
- Define a formal MCP tool for memory search (e.g., `graphify_memory_query`) that plugins can use to retrieve semantic context.
- Use the `workspace_key` as the primary mechanism for correlating data across core, memory, and plugins.
- Document all boundary decisions in the OpenSpec specifications to ensure forward compatibility.

## Implementation Roadmap

Tracked in the memory-plugin-integration-v1 OpenSpec change. Phases are sequential;
each phase requires verification before the next begins.

| Phase | Scope | Status |
| ----- | ----- | ------ |
| Phase 0 | Repair pre-existing `graphify-llm` memory.rs compile errors (false workspace isolation, `Unavailable` Clone, caller signatures) | ✅ Done |
| Phase 1 | Core memory query contract: storage-agnostic `MemorySearcher` API with `workspace_key` scoping, bounded limits, explicit unavailable status | ✅ Done |
| Phase 2 | Third-party MCP query integration: `graphify_memory_query` tool through the restricted memory service boundary | ✅ Done |
| Phase 3 | Plugin-domain memory infrastructure: versioned `PluginMemoryEnvelope<T>`, per-plugin collection derivation, read/write boundaries | ✅ Done |
| Phase 4 | `.toon` enrichment: reserved `plugin_data.<plugin_id>` container, core tolerance of absent/unknown entries | ✅ Done |
| Phase 5 | Crate split: extract `graphify-memory` (memory.rs + plugin_memory.rs + memory config); keep `graphify-llm` as the general LLM gateway. In the same change, add the minimal `CoreLlmProvider` trait + `PluginContext` skeleton (see v2.0-alpha supplement). No multi-model routing, no plugin-specialized model clients (Shieldstral etc. stay in Phase 7) | ✅ Done |
| Phase 6 | Verification, docs sync, OpenSpec archive of memory-plugin-integration-v1 | ⏳ Planned |
| Phase 7 | Three native plugins (opendoc / review / handoff) in the GraphifyPlugins repository | ⏳ Planned |

Phase 5 is gated by Phase 3/4: the memory API boundary must stabilize before the
rename. Per the decision recorded above, `graphify-memory` was chosen over
`graphify-vector` (vector narrows long-term memory to a vector-DB wrapper).

### Phase 5 decisions (recorded 2026-08-09)

- **Extract, not rename**: `graphify-memory` = memory.rs + plugin_memory.rs + memory
  config subset; `graphify-llm` survives as the general LLM gateway
  (pipeline.rs / gbnf.rs / providers). Lower churn than an outright rename.
- **Include the gateway skeleton in Phase 5**: `CoreLlmProvider` trait
  (`complete` / `chat`) with `AutoRotatePipeline` as the default implementation,
  plus a `PluginContext { memory, llm, workspace_key }` skeleton. Doing this at
  extraction time is near-zero marginal cost; deferring it to Phase 7 would
  enlarge that phase's blast radius (plugin ecosystem + core context changes
  simultaneously).
- **Minimal interface scope**: no multi-model routing, no plugin-specialized
  model clients in Phase 5. Dedicated models (e.g. Shieldstral-3B in
  graphify-plugin-review, Option B) remain Phase 7 plugin work.
- **[待討論] Memory write**: v2's `GraphifyMemoryEngine` implies plugin write
  capability, which conflicts with the Phase 3 decision (plugins cannot write
  core memory; only the indexing pipeline writes). Phase 5 keeps the read-only
  boundary; a system-managed write API would be a separate OpenSpec change.
- Source: `docs/ref/graphify-v2-alpha-architecture-supplement.md` (verbatim).

## References

- `docs/core.md`: Core engine and `GraphifyPlugin` trait definition.
- `docs/plugin_system.md`: Plugin communication protocol and `workspace_key` usage.
- `openspec/specs/architecture/spec.md`: High-level requirements including local embeddings and Qdrant integration.
- `openspec/specs/extraction-schema/spec.md`: Physical contract for `graph.json` and `.toon`.
