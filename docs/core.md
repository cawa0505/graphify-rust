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
