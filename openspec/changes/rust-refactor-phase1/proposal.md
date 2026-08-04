# Proposal: Rust 重構 Phase 1

## Intent

建立 GraphifyRust 的 Rust workspace 骨架，實作 Phase 1 核心功能：tree-sitter 多語言 AST 解析 + petgraph 圖建構，輸出符合 extraction-schema 的 graph.json。

GraphifyOpt (Python) 的瓶頸：tree-sitter Python bindings ~48MB 記憶體，Rust 原生 ~1MB；AST 解析速度差距 ~10x；petgraph 比 networkx 快 5-10x。

## Scope

### In Scope

- Workspace 設定（graphify-core, graphify-mcp, graphify-cli）
- Tree-sitter extraction（Python, Rust, Go, JS/TS）
- Graph building（petgraph）
- graph.json 輸出（符合 extraction-schema）
- MCP server skeleton（graphify_query, graphify_path）
- CLI（extract, query subcommands）

### Out of Scope

- LLM semantic extraction（Phase 2）
- Clustering / community detection（Phase 2）
- HTML visualization（Phase 2）
- `--update` incremental mode（Phase 2）
- Wiki export（Phase 2）

## Approach

1. 建立 workspace crate 結構
2. 定義 Node/Edge/GraphOutput types（Serde）
3. 用 tree-sitter 實作多語言 extractor
4. 用 petgraph 建構 graph，輸出 graph.json
5. 實作 CLI + MCP server skeleton
