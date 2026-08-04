# GraphifyRust

基於 Rust 與 Tree-sitter 實作的高效能、低延遲靜態程式碼 AST 語意圖譜建構工具，內建高可用性多金鑰（Local SLM / Cloud API）輪轉容災機制與 MCP (Model Context Protocol) 伺服器支援。

## 核心特性

- **高效靜態 AST 提取**：採用 `tree-sitter` 解析 Rust、Python、Go、JavaScript、C、C++、PHP 等多種語言。毫秒級掃描檔案，精確提取實體（模組、結構體、函數）與關係（調用、包含、導入）。
- **高維度代碼圖譜建構**：採用記憶體端有向圖（Directed Graph）引擎，輸出標準 `graph.json` 拓撲，所有實體與關係完全對齊 [Python 舊版 (cawa0505/graphify)](https://github.com/cawa0505/graphify) 相容規範。
- **Auto-Rotate 雙層容災**：支援本地小模型（如 Ollama Qwen2.5-Coder）與遠端 API。遭遇 429 Rate Limit 時，利用 Atomic 執行緒安全模除（Modulo）機制即時、零延遲切換 API Key 或執行備用 Provider 降級。
- **XDG-First 配置與平滑遷移**：優先讀取 `~/.config/graphify/config.toml` 或 `GRAPHIFY_CONFIG_PATH`，且自動遷移舊版 Python `~/.graphify/config.json` 的設定值。
- **MCP 互動式查詢伺服器**：內建 MCP Server，提供 `graph_summary`（高維鳥瞰）、`graph_query_node`（精準局部探針）、`graph_trace_path`（調用鏈分析）等工具，大幅減少 AI 助理讀取程式碼的 Token 消耗。

## 專案結構

```text
GraphifyRust/
├── graphify-core/   # AST 靜態解析器、有向圖引擎與圖譜導出
├── graphify-llm/    # 多 Provider 金鑰輪轉管線、429 降級容災
├── graphify-mcp/    # MCP 伺服器實作、極簡探針 Tool 暴露
└── openspec/        # OpenSpec 技術規格說明與變更設計
```

## 快速開始

### 編譯專案
```bash
cargo build --release
```

### 執行 MCP 伺服器
```bash
cargo run --release --bin graphify-mcp
```

