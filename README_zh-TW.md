# GraphifyRust

[English](README.md) | 繁體中文

基於 Rust 與 Tree-sitter 實作的高效能、低延遲靜態程式碼 AST 語意圖譜建構工具。採用精簡 `.toon` Token 序列化格式、多金鑰執行緒安全原子輪轉（Auto-Rotate）容災管線、Qdrant 長短期語意記憶體配置，以及 MCP (Model Context Protocol) 伺服器，專為 LLM 助理提供極低成本、極致精準的程式碼拓撲感知能力。

![Graphify TUI Demo](docs/graphify-tui-demo.gif)

---

## 📚 模組化技術手冊 (Documentation Index)

詳細的系統設計、指令規格與整合說明已拆分至獨立文件：

*   **[命令列手冊 (CLI Manual)](docs/cli.md)**：完整說明 `graphify` 包含的 `extract`, `query`, `path`, `install-skill`, `tui` 以及全新的 `index`（語意記憶體一鍵索引）指令。
*   **[核心解析引擎 (Core Engine)](docs/core.md)**：深入 tree-sitter AST 解析、Petgraph 記憶體預分配優化以及節省 60% Token 空間的 `.toon` 格式原理。
*   **[LLM 容災與記憶體管線 (LLM & Memory)](docs/llm.md)**：詳細解析 Lock-free `AtomicUsize` 金鑰自動輪轉、429 容災、備用降級，以及 Qdrant 向量記憶體儲存庫實作。
*   **[MCP 伺服器規格 (MCP Server)](docs/mcp.md)**：說明如何將圖譜與 RAG 檢索能力（Summary, Query Node, Path, Reindex）完美接入 AI 助理環境。

---

## ⚡ 效能基準測試與對齊 (Performance Benchmark & Parity)

我們對同一個包含 `Rust`, `Python`, `Go` 以及 `JavaScript` 的多語言測試專案（110 個源檔案，422 條邊）進行了物理提取耗時比對：

| 評測維度 | Python 舊版 | Rust 新版 (110 檔案) | 效能提升倍數 |
| :--- | :---: | :---: | :---: |
| **AST 提取 + 建圖時間** | ~420 ms | **16 ms** (0.016s) | **26.25 倍** ⚡ |
| **多核心並行擴充性** | 不支援 (單執行緒) | **支援 Rayon (-j N)** | 隨執行緒數線性增長 🚀 |
| **記憶體分配策略** | 動態擴容拷貝 | **Petgraph Arena 預分配** | 0 垃圾記憶體碎片 |
| **圖譜檔案體積 (.toon)** | 185 KB (JSON) | **74 KB** (.toon) | **節省 60% 體積** (Token 效率提速) |

---

## 📦 快速安裝與編譯 (Installation)

```bash
# 直接從 GitHub 一鍵安裝
cargo install --git https://github.com/cawa0505/graphify-rust.git --branch main graphify-cli --force

# 或複製並進入本地專案目錄
git clone https://github.com/cawa0505/graphify-rust.git
cd graphify-rust
cargo install --path graphify-cli --force
```

---

## 📄 開源授權 (License)

本專案採用 **MIT License** 授權，詳見 [LICENSE](LICENSE) 檔案。
