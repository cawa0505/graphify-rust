# Graphify Core Engine

`graphify-core` contains the structural AST parser, petgraph-based directed graph engine, and First-Class `.toon` serialization codec. It is written in pure synchronous Rust and is fully WASM-compatible.

---

## Static AST Parsers

The core leverage Tree-sitter parsers to extract structural definitions and imports across 7 target languages:
- **Rust** (`tree-sitter-rust`)
- **Python** (`tree-sitter-python`)
- **Go** (`tree-sitter-go`)
- **JavaScript/TypeScript** (`tree-sitter-javascript`)
- **C/C++**
- **PHP**

### Extracted Relationships
1. **`contains`**: Nesting hierarchy (e.g., File contains Module, Module contains Class, Class contains Method).
2. **`calls`**: Function or method invocation paths.
3. **`imports`**: Dependency import trees.

---

## Petgraph Arena Pre-Allocation

To achieve sub-millisecond memory performance, the builder constructs directed graphs using petgraph's `DiGraph::with_capacity` directly.
- Grouping nodes and edges before graph instantiation completely eliminates dynamic heap reallocations.
- Keeps graph construction under 1ms even for multi-thousand node codebases.

---

## First-Class `.toon` Format

To protect against token bloat in LLM context windows, Graphify employs **Token-Oriented Object Notation (.toon)**:
- **Header-Declared Columns**: Avoids duplicating JSON dictionary keys. Metadata and array shapes are defined at the start of tables.
- **Virtual Serialization-Time Hyperedge Aggregation**: Multiple edges sharing matching sources, relations, and confidence levels are aggregated during serialization with target nodes grouped by `|` pipes. This delivers **60%+ file-size and token savings** during LLM ingest.
- **Transparent Recovery**: Deserialization via `from_toon` splits aggregated pipes back into individual flat directed edges in memory, leaving graph traversal algorithms 100% unaffected.

---

## Plugin API (v1)

`graphify-core` exposes the embedded plugin contract via the `plugin` module:

- **`GraphifyPlugin` trait**: `get_id` / `bind` / `get_workspace_key` / `sync_toon`.
- **`WorkspaceContext`**: `workspace_key` (routing foreign key), `workspace_name`, `root_path`, `timestamp` — matching the interface contract in `docs/plugin_system.md` §3.1.

### sync_toon Packet Contract

`sync_toon(Option<Vec<u8>>) -> Vec<u8>` exchanges **.toon documents**, not custom envelopes: the payload is a serialized .toon file, versioned by the `format_version` metadata key. MUST metadata: `format_version` + `workspace_key`. Optional payload sections (`symbol_nodes`, `graph_topology`) align with `docs/plugin_system.md` §3.2. Errors are expressed as an `error` metadata key (no panic, no signature change). Full contract: `openspec/changes/plugin-sync-toon-v1/specs/sync-toon-packet/spec.md`.

The contract is dependency-free (`std` + `serde` only), keeping `graphify-core` free of LLM/HTTP/MCP dependencies. A reference implementation lives in `plugin.rs` tests, proving external crates can implement and drive the trait.

### `.toon` plugin_data Container

`.toon` metadata reserves the `plugin_data` container for plugin-owned enrichment (memory-plugin-integration-v1):

- **Schema**: `metadata.plugin_data.<plugin_id> → serde_json::Value` (plugin-controlled payload). Plugins MUST NOT add arbitrary top-level metadata fields.
- **Round-trip**: `to_toon` / `from_toon` serialize and restore the container verbatim (escaped JSON in the metadata block).
- **Tolerance**: absent `plugin_data` parses to an empty container (legacy `.toon` unchanged); unknown plugin entries are preserved verbatim, never interpreted. Empty containers are omitted on serialization.
- **Domain memory envelope**: plugin records use `graphify_core::plugin_memory::PluginMemoryEnvelope<T>` (`format_version` / `workspace_key` / `plugin_id` / `record_id` / `record_kind` / `created_at` / `source_refs` / `payload`) with plugin-specific payloads (`OpenDocPayload`, `ReviewPayload`, `HandoffPayload`), each independently versioned via `schema_version`. Storage isolation and name derivation live in `graphify-llm::plugin_memory`; the core defines only the data contract.

### Crate Dependency Graph

```
┌──────────────────────────────────────────────────────────┐
│   graphify-plugin-*   (embedded crates, e.g. handoff)    │
│        │  implements graphify_core::GraphifyPlugin       │
│        ▼                                                 │
│   graphify-core ── plugin.rs (trait + WorkspaceContext)  │
│   (zero LLM/HTTP deps, sync, WASM-compatible)            │
└──────────────────────────────────────────────────────────┘
```

Dependency direction: `graphify-plugin-* → graphify-core`. There is no reverse dependency — the core defines the contract, plugins implement it. `graphify-llm` and `graphify-mcp` remain optional layers above; plugins never require them.

### External MCP plugin boundary

Third-party plugins do not add dependencies to `graphify-core`. They run as
independent MCP servers and are hosted by `graphify-mcp` when the unified
gateway mode is enabled. The gateway reads `[plugins.<id>]` declarations,
performs the MCP initialize handshake over stdio, aggregates tools under the
`graphify_plugin_<plugin_id>_<tool_name>` namespace, and forwards calls. After
`graph_reindex` or `graphify_notify_plugins`, it sends a
`notifications/graph_updated` notification containing `kind` and
`workspace_key`.
