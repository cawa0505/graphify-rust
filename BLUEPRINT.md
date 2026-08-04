# GraphifyRust 重構核心藍圖 & MCP 增強架構設計

## 一、 核心設計目標 (Core Principles)

* **極致效能與低記憶體**：利用 Rust + tree-sitter 將大型 Repo 的 AST 靜態解析速度提升 50 倍以上，記憶體佔用壓在 10MB 以內。
* **100% 相容 Python 版**：保持相同的 CLI 介面（參數）與產出的 `graph.json` / `graphml` 資料結構，讓既有的上游/下游工具與 Agent 無痛銜接。
* **靈魂組件：Provider Config + Auto Rotate**：保留並強化你原創的高可用性機制。對抗 429 Rate Limit，自動在 Local SLM ↔ 免費遠端 API (Flash) ↔ 備用 API 之間無縫切換。
* **Local-First & Token 避難**：支援 Ollama / llama.cpp 本地小模型（如 Qwen2.5-Coder 1.5B/7B），搭配 GBNF / Structured Output，實現 **0 元 API 成本** 的代碼語意圖譜建構。

---

## 二、 系統架構模組劃分 (Module Architecture)

```
┌────────────────────────────────────────────────────────────────────────┐
│                          graphify-rust CLI                             │
└────────────────────────────────────────────────────────────────────────┘
                                    │
    ┌───────────────────────────────┼───────────────────────────────┐
    ▼                               ▼                               ▼
┌───────────────────────┐ ┌───────────────────┐ ┌─────────────────────────┐
│ 1. Static Extractor   │ │ 2. Graph Engine   │ │ 3. Auto-Rotate LLM Pipeline
│ (tree-sitter / AST)   │ │ (petgraph / Serde)│ │ (Local SLM / Remote API)│
└───────────────────────┘ └───────────────────┘ └─────────────────────────┘
            │                       │                            │
    抽出 80% 確定性結構              建立 Topology & JSON          處理 20% 語意摘要/抽取
            └───────────────────────┴────────────────────────────┘
```

### 1. 靜態提取器 (src/parser/)
* **技術選型**：tree-sitter (支援 Rust, TypeScript, Python, Go 等多語言 parser)。
* **職責**：以物理級的確定性抓取代碼結構（struct, class, fn, impl, import, call）。
* **優點**：不花費任何 LLM Token，幾毫秒內掃完數千個檔案。

### 2. 圖結構引擎 (src/graph/)
* **技術選型**：petgraph ＋ serde_json。
* **職責**：在記憶體中建立有向圖（Directed Graph），維護 Node（檔案/類別/函數）與 Edge（調用/繼承/引用）的關係。
* **相容層**：導出的 JSON Schema 欄位與 Python 版 100% 一致：
```json
{
  "nodes": [{ "id": "fn_process", "type": "function", "label": "process_user" }],
  "edges": [{ "source": "fn_process", "target": "db_query", "relation": "calls" }]
}
```

### 3. 轉發與容錯管道 (src/provider/)
* **技術選型**：reqwest ＋ async-trait ＋ tokio。
* **職責**：當需要針對節點做高階語意摘要時，負責呼叫 LLM，並包含全自動降級/轉發機制：
```toml
# config.toml
[[providers]]
name = "ollama-local"
type = "ollama"
endpoint = "http://localhost:11434"
model = "qwen2.5-coder:1.5b"
priority = 1                      # 預設 0 成本在地端跑

[[providers]]
name = "google-flash"
type = "gemini"
api_key = "ENV_VAR"
model = "gemini-2.5-flash"
priority = 2                      # 遇限流或失敗自動秒切遠端
```

---

## 三、 本地小模型 (Local SLM) 優化策略

為了讓 Qwen2.5-Coder 1.5B/7B 這種小模型在 graphify-rust 裡跑出大模型等級的精準度，我們採用以下戰術：
1. **GBNF Grammar 強制約束**：在調用本地 llama.cpp / Ollama 時，帶入 JSON Schema Grammar，在底層 Token 生成層級直接封鎖非法格式，達到 100% 的 JSON 解析成功率。
2. **極簡 Task 拆分**：不讓 SLM 去做龐大的程式碼分析，Rust 已經把 AST 拆解好，SLM 只需要做「10 字以內的函數功能摘要」或「模糊模組歸類」。

---

## 四、 🛰️ MCP 增強架構設計

不要讓 Agent 去讀整張圖，而是讓 Agent 把 graphify-rust 當成一個可以「互動式查詢」的結構化資料庫。我們在 graphify-rust 內建 MCP Server，直接暴露以下 4 個專為 **「極致省 Token」** 打造的精準 Tool：

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           graphify-rust MCP                             │
└─────────────────────────────────────────────────────────────────────────┘
        │                         │                       │
        ▼                         ▼                       ▼
┌───────────────────┐   ┌───────────────────┐   ┌───────────────────┐
│ graph_summary     │   │ graph_query_node  │   │ graph_trace_path  │
│ (高維度地圖)       │   │ (節點細節與相鄰)   │   │ (依賴與調用鏈)     │
└───────────────────┘   ┌───────────────────┐   ┌───────────────────┐
```

### 1. 🗺️ graph_summary（高維度鳥瞰地圖）
* **輸入**：無（或可選 `module_path`）
* **輸出**：全專案的頂層架構與模組拓撲（僅包含主要 Module、核心 Struct/Class 的抽象，不包含具體代碼與細節 Edge）。
* **Token 消耗**：~200 Tokens
* **AI 體驗**：Agent 剛進專案時呼叫一次，一眼看懂整個專案的模組劃分，決定下一步要查哪裡。

### 2. 🔍 graph_query_node（節點精準局部探針）
* **輸入**：`{ "node_id": "fn_process_user", "depth": 1 }`
* **輸出**：只傳回該節點的定義摘要，以及與它直接相連（depth: 1）的 1-hop 節點與關係（例如：誰呼叫了它、它呼叫了誰、它是哪個 Struct 的 impl）。
* **Token 消耗**：~100 Tokens
* **AI 體驗**：Agent 不需要讀整個檔案，只需要像查字典一樣，精準調出這個函數的周邊生態。

### 3. 🛣️ graph_trace_path（影響力與調用鏈追蹤）
* **輸入**：`{ "from": "UserStruct", "to": "DatabaseQuery" }`
* **輸出**：傳回兩點之間的最短路徑（Shortest Path）或調用鏈（Call Chain），利用 petgraph 的 Dijkstra 演算法在 1 毫秒內算完：
  `UserStruct -> process_user() -> validate_user() -> DatabaseQuery`
* **Token 消耗**：~50 Tokens
* **AI 體驗**：當 Agent 要做重構時，呼叫這個 Tool 秒懂改動一個型別會波及哪些鏈條（Impact Analysis），徹底封死 AI 瞎猜脈絡的可能！

### 4. 🔄 graph_reindex（背景增量更新）
* **輸入**：`{ "file_path": "src/user.rs" }`
* **輸出**：`{ "status": "updated", "changed_nodes": 3 }`
* **AI 體驗**：當 Code Agent 修改完程式碼後，呼叫一次增量更新，Rust 端只花幾毫秒重新解析該檔案的 tree-sitter AST 並更新記憶體圖譜，維持 Graph 的實時新鮮度。

---

## 五、 🛠️ MCP 實作技術選型

1. **極簡 JSON-RPC 協議層**：利用 tokio 監聽 stdin/stdout。因為 Rust 的 `serde_json` 處理 JSON-RPC 速度極快且型別嚴格，不需要帶入任何龐大的 Node.js 運行時。
2. **與 petgraph 記憶體圖譜綁定**：當 MCP 啟動時，graphify-rust 會在背景把 `graph.json` 載入成 `petgraph::Graph` 記憶體物件。所有的 MCP Tool 查詢（如最短路徑、鄰接節點）都是純記憶體運算（Zero Disk I/O），響應速度是微秒（$\mu s$）級別！

---

## 六、 📋 包含 MCP 的完整 Phase 路線圖

| 階段 | 模組名稱 | 核心內容與價值 |
| --- | --- | --- |
| **Phase 1** | **Core Data & Graph** | 定義相容 Python 的 JSON Struct，用 petgraph 構建記憶體圖譜。 |
| **Phase 2** | **Tree-sitter Parser** | 實現 Rust/TS/Py 的物理級 AST 解析，毫秒級完成 80% 靜態建圖。 |
| **Phase 3** | **Auto-Rotate Pipeline** | 掛載你的靈魂組件：API Key/Provider 自動降級備援機制（Local SLM ↔ Remote）。 |
| **Phase 4** | **MCP Protocol Server** | **🌟 新增！** 實現 stdio JSON-RPC，提供 `summary` / `query_node` / `trace_path` 三大精準工具。 |
| **Phase 5** | **CLI & E2E Validation** | 整合 CLI 參數，驗證與 OpenCode / Claude Code / draco 的 MCP 無縫協同。 |
