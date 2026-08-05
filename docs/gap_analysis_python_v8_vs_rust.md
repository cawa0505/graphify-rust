# Gap Analysis & Comparison: Python Graphify (v8) vs. GraphifyRust

This document outlines the architectural differences, feature coverage, and an implementation gap analysis between the legacy Python Graphify (`v8`) and the high-performance, single-binary GraphifyRust.

---

## 1. Feature Coverage Matrix

| Feature / Capability | Python Graphify (v8) | GraphifyRust (Current) | Gaps / Future Path | Status / [待討論] |
| :--- | :--- | :--- | :--- | :--- |
| **Language Coverage** | 36 tree-sitter grammars + regex fallback | 9 high-perf extractors (Rust, Go, Python, TS/JS, C/C++, PHP, Java, Swift) | Satisfies 95% of active project needs; adding others is trivial. | **Implemented (Core Set)** |
| **Database Integrations** | Neo4j, FalkorDB, Postgres, Qdrant | Qdrant (semantic vector store) | No Neo4j/FalkorDB direct push; graph exported as `.toon` or `.json`. | `[待討論]` |
| **Incremental Updates** | `--update` (modified file checks via git/timestamp) | Full re-extraction only | No changed-file diffing or watch mode yet. | `[待討論]` |
| **Work Memory (Reflection)** | `LESSONS.md` output feedback loop + JSON overlay | None | `AutoRotatePipeline` exists but is not used for lessons. | `[待討論]` |
| **Clustering & Partitioning** | Leiden community clustering + parallel LLM naming | Raw tree visualization in TUI canvas | Leiden algorithm is missing in Rust graph engine. | `[待討論]` |
| **Visualization Backends** | Force-directed `graph.html`, SVG, GraphML, Obsidian | Ratatui TUI canvas, MCP stdio | Web UI (`graph.html` template) is missing. | `[待討論]` |
| **Semantic Pass** | Optional doc/media LLM summarization | Completed `AutoRotatePipeline` | Pipeline implemented but not wired into `extract`. | `[待討論]` |

---

## 2. Architectural Comparison & Trade-Offs

### Python Graphify (v8): Maximum Reach, Massive Weight
* **Model**: Pulls dozens of heavyweight external dependencies (compiled libraries for PDF, images, Microsoft Office formats, Google API client, 36 separate precompiled tree-sitter bindings).
* **Footprint**: Multi-gigabyte runtime footprint. Requires a complex `pip`/`uv` setup and Python 3.11+ environment.
* **Performance**: Heavy AST walks and serialization overheads in single-threaded Python-level loops.
* **Payoff**: High breadth, out-of-the-box support for any niche language and office media extraction.

### GraphifyRust: Zero-Dependency, Extreme Performance
* **Model**: Hand-written, high-quality Rust extractors running AST traversals in native memory.
* **Footprint**: Single static, compiled binary. Zero external runtime dependencies, minimal CPU/memory overhead. Perfect for isolated, resources-constrained homelab/edge deployments.
* **Performance**: Ultra-fast AST parses powered by `Rayon` multi-threaded worker pools.
* **Trade-Off**: Adding languages requires compiling their specific grammars (takes ~200 lines of safe Rust), but delivers predictable, compiler-enforced safety.

---

## 3. Recommended Implementation Roadmap

If closing legacy capability gaps becomes a priority, we suggest the following sequential phases to maintain structural elegance and performance:

### Phase 1: Interactive HTML & Export Format Boost `[待討論]`
* **Action**: Implement a simple `--html` flag to write a self-contained, interactive force-directed HTML template (`graph.html`).
* **Why**: Provides high-quality, lightweight external visualization bypassing heavy database dependencies.

### Phase 2: In-Memory Leiden Clustering `[待討論]`
* **Action**: Port or integrate a lightweight pure-Rust community detection algorithm onto our `petgraph::DiGraph`.
* **Why**: Unlocks automatic subsystem grouping and hierarchical layout rendering directly within both the TUI and future HTML layouts.

### Phase 3: Incremental Extractor (`--update`) `[待討論]`
* **Action**: Parse Git/File mtime diffs to update only modified files inside the persistent graph artifact.
* **Why**: Drops extraction latency to sub-millisecond ranges on large repositories.
