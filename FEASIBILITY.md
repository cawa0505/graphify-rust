# GraphifyOpt → Rust 重構可行性評估

## 現狀規模

| 指標 | 數值 |
|------|------|
| Python 總行數 | ~52,277 |
| 核心模組 | extract.py (5408), build.py (1299), llm.py (2730), serve.py (1999), detect.py (1813) |
| 語言 Extractor | 30+ 種（Python/JS/TS/Go/Rust/Java/C/C++/Ruby/C#/Kotlin/Scala/PHP/Swift/Lua/Zig/Bash/JSON/SQL/Terraform/Verilog/Fortran/Elixir/Pascal/Apex/Blade/Razor/DM/Dart/Julia/ObjC） |
| 依賴 | networkx, tree-sitter (24 language bindings), numpy, rapidfuzz |

## 分層可行性

| 層級 | 現狀 | Rust 替代 | 可行性 | 優先級 |
|------|------|-----------|--------|--------|
| **Tree-sitter 解析** | Python tree-sitter bindings (24個 py 包) | `tree-sitter` crate + language grammars | ★★★★★ 最高 ROI | P0 |
| **Graph 建構** | NetworkX (純 Python) | `petgraph` | ★★★★★ | P0 |
| **File 措辭/偵測** | pathlib + glob | `std::fs` + `ignore` crate | ★★★★★ | P0 |
| **MCP Server** | Python mcp lib (stdio) | `rmcp` / `tower-lsp` | ★★★★☆ | P0 |
| **LLM Pipeline** | Python httpx + provider rotation | Rust reqwest + 自寫 rotation | ★★★☆☆ | P1 |
| **Clustering (Leiden)** | graspologic (Python) | `petgraph` + 自寫或 `nexus` | ★★☆☆☆ | P2 |
| **Export (HTML/SVG)** | Python string formatting | Rust serde + tera/minijinja | ★★★☆☆ | P1 |
| **Semantic Cache** | Python file-based | Rust sled/rocksdb 或純 JSON | ★★★★☆ | P1 |

## 建議架構

```
graphify-rust/
├── crates/
│   ├── graphify-core/      # tree-sitter 解析 + petgraph 建構
│   │   ├── src/
│   │   │   ├── extract/    # 每語言一個 module，tree-sitter grammar 直掛
│   │   │   ├── graph.rs    # petgraph 建構 + 去重 + alias 解析
│   │   │   ├── detect.rs   # 檔案掃描 + 過濾
│   │   │   └── cache.rs    # 結構化增量快取
│   │   └── Cargo.toml
│   ├── graphify-llm/       # LLM pipeline + Provider Auto-Rotate
│   │   ├── src/
│   │   │   ├── provider.rs # reqwest + 429 retry + key rotation
│   │   │   ├── semantic.rs # 語意分析 prompt + 解析
│   │   │   └── cache.rs    # semantic cache (結構化)
│   │   └── Cargo.toml
│   └── graphify-mcp/       # MCP Server (JSON-RPC over stdio)
│       ├── src/main.rs
│       └── Cargo.toml
├── Cargo.toml              # workspace root
└── README.md
```

## 關鍵技術決策

1. **tree-sitter Rust binding**: 直接用 `tree-sitter` crate，不再經過 Python bindings。24 個 language grammar 全部掛上去，零 LLM pass。

2. **petgraph 取代 NetworkX**: 記憶體從 ~48MB 降到 <5MB，速度快 10-50x。`petgraph::Graph<NodeWeight, EdgeWeight>` 直接映射現有的 nodes/edges schema。

3. **MCP Server**: 用 `rmcp` (Rust MCP SDK) 或手寫 JSON-RPC over stdio。暴露 `graphify_query`、`graphify_path`、`graphify_export` 三個 tool。

4. **LLM Pipeline**: Phase 1 可以先用 Rust reqwest 實作 provider rotation（你已經驗證過的邏輯），Phase 2 再加語意分析 prompt。

## 風險與建議

| 風險 | 緩解 |
|------|------|
| 30 個 extractor 一次性移植量太大 | **分階段**：先做 top 5 語言（Python/JS/TS/Go/Rust），跑通全流程，再逐一補齊 |
| Leiden clustering 在 Rust 生態不成熟 | Phase 2 用 Python FFI 橋接 graspologic，或用輕量替代（label propagation） |
| 現有 `graph.json` schema 相容性 | 用 `serde` 嚴格映射現有 extraction schema，確保 output 100% 相容 |
| 測試覆蓋 | 現有 `tests/` 的 fixture 可以直接用，Rust 端對齊同一份 fixture |

## 工作量估算

| 階段 | 範圍 | 預估工時 |
|------|------|----------|
| **Phase 1**: 核心（extract + graph + detect） | top 5 語言 + petgraph 建構 + CLI | 3-5 天 |
| **Phase 2**: MCP Server + export | JSON-RPC + graph.json export | 1-2 天 |
| **Phase 3**: LLM pipeline | Provider rotation + semantic cache | 2-3 天 |
| **Phase 4**: 剩餘 25+ extractors | 逐一移植 | 5-8 天 |
| **Phase 5**: 測試 + benchmark | 對齊 Python 版 fixture + 性能對比 | 2-3 天 |

**總計：13-21 天**（一個人全職）

## 結論

**完全可行。** 最大 ROI 在 Phase 1（tree-sitter + petgraph），光是這一步就能達到 48x 記憶體節省和 10x 速度提升。LLM pipeline 可以用 Rust reqwest 重寫 provider rotation，保持已驗證過的 auto-rotate 邏輯。

建議先跑 Phase 1 證明可行性，再決定是否全面投入。
