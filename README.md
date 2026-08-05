# GraphifyRust

English | [繁體中文](README_zh-TW.md)

An extremely high-performance, low-latency static code AST semantic graph construction tool written in Rust and powered by Tree-sitter. Featuring the highly compact `.toon` Token-Oriented Object Notation serialization format, a thread-safe multi-key lock-free auto-rotating failover LLM pipeline, local Qdrant long/short-term semantic memory integration, and Model Context Protocol (MCP) server support, it delivers high-precision topological code awareness to AI assistants at minimal cost.

![Graphify TUI Demo](docs/graphify-tui-demo.gif)

---

## 📚 Documentation Index

Detailed system architecture, command specifications, and integration guides have been modularized into separate manuals:

*   **[CLI Manual](docs/cli.md)**: Documenting all commands including `extract`, `query`, `path`, `install-skill`, `tui`, and the new `index` command for one-click semantic vector store ingestion.
*   **[Core Engine](docs/core.md)**: Deep dive into Tree-sitter AST parsing, petgraph pre-allocation optimizations, and the `.toon` serialization logic which saves 60% in token overhead.
*   **[LLM & Memory Pipeline](docs/llm.md)**: Explaining the lock-free thread-safe `AtomicUsize` multi-key rotation, 429 rate limit tolerance, backup provider failover, and local Qdrant vector store implementation.
*   **[MCP Server Spec](docs/mcp.md)**: Detailing how the graph and semantic RAG search capabilities (Summary, Query Node, Path, Reindex) integrate with AI development environments.

---

## ⚡ Performance Benchmark & Parity

We compared the AST extraction and graph building speeds on a multi-language test project containing `Rust`, `Python`, `Go`, and `JavaScript` (110 source files, 422 edges):

| Dimension | Legacy Python version | New Rust version (110 files) | Performance Leap |
| :--- | :---: | :---: | :---: |
| **AST Extraction + Build Time** | ~420 ms | **16 ms** (0.016s) | **26.25x faster** ⚡ |
| **Multi-Core Scaling** | Not supported (single-threaded) | **Supported via Rayon (-j N)** | Scales linearly with threads 🚀 |
| **Memory Allocation Strategy** | Dynamic scaling / copying | **Petgraph Arena Pre-allocation** | Zero heap fragmentation |
| **Serialized File Size (.toon)** | 185 KB (JSON) | **74 KB** (.toon) | **60% token reduction** (boosts prompt speed) |

---

## 📦 Quick Installation & Build

```bash
# Install directly from GitHub
cargo install --git https://github.com/cawa0505/graphify-rust.git --branch main --bin graphify --force

# Or clone and enter directory to build locally
git clone https://github.com/cawa0505/graphify-rust.git
cd graphify-rust
cargo install --path graphify-cli --force
```

---

## 📄 License

Licensed under the **MIT License**. See [LICENSE](LICENSE) for details.
