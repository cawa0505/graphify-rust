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

- **`GraphifyPlugin` trait**: `get_id` / `bind` / `get_workspace_uuid` / `sync_toon`.
- **`WorkspaceContext`**: `workspace_uuid` (routing foreign key), `workspace_name`, `root_path`, `timestamp` — matching the interface contract in `docs/plugin_system.md` §3.1.

The contract is dependency-free (`std` + `serde` only), keeping `graphify-core` free of LLM/HTTP/MCP dependencies. A reference implementation lives in `plugin.rs` tests, proving external crates can implement and drive the trait.

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
