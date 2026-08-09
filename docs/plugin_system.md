# Graphify Plugin & Module System 系統架構規劃書

## 1. 執行摘要 (Executive Summary)

Graphify 係基於 16ms 極速靜態 AST 提取、Petgraph 拓撲圖譜與 .toon Token 壓縮格式建構之硬核代碼圖譜引擎。本規劃書旨在定義 Graphify Plugin / Module System 之微核心架構，將散亂的 AI Agent 輔助工具（如 Code Review、Code Relay Handoff、OpenDocuments 多格式向量檢索）收攏於統一生態。

本架構採用「神經符號雙模系統 (Neuro-Symbolic System)」設計理念：
- **Graphify Core (Symbolic)**：負責 100% 精準的符號結構樹、呼叫鏈與衝擊半徑分析（Ground Truth）。
- **OpenDocuments (Neural)**：負責多格式文檔（.xlsx, .pdf, .docx 等）之向量語意空間（Vector Semantic Space）。
- **Workspace UUID**：作為跨模組、跨引擎的硬性對齊外鍵（Foreign Key），實現零耦合、高隔離的 MCP-to-MCP 協同工作流。

## 2. 系統總體架構 (System Architecture)

採用微核心（Micro-kernel）與 MCP Protocol 雙層解耦架構。Graphify Rust Core 保持極輕量與極速，所有進階功能均以 Plugin / MCP Module 形式外掛。

```
┌───────────────────────────────────────────────┐
│            Graphify TUI & CLI                  │
└───────────────────────┬───────────────────────┘
                        │
┌──────────────────────────────────────────────────────────────────────▼──────────────────────────────────────────────────────┐
│                    Graphify Core Engine (Rust Micro-kernel)                                                                   │
│  - 16ms AST Multi-Language Extractor (Rust, Python, Go, JS, C, PHP.)                                                         │
│  - Petgraph Topology Engine (BFS/DFS Shortest Path, Call Graph)                                                               │
│  - Ultra-compact `.toon` Serializer (-60%+ Token Savings)                                                                     │
│  - Workspace Identity Manager (`workspace_key` Generator & Indexer)                                                          │
└──────────────────────────────────────────────────────────────────────┬──────────────────────────────────────────────────────┘
                        │
      ┌─────────────────────────────────┼─────────────────────────────────┐
      │ (MCP Protocol)                  │ (MCP Protocol)                  │ (MCP Protocol)
      ▼                                 ▼                                 ▼
┌──────────────────────────────┐   ┌──────────────────────────────┐   ┌──────────────────────────────┐
│ `graphify-plugin-review`     │   │ `graphify-plugin-handoff`    │   │ `graphify-plugin-opendoc`    │
│ (Blast Radius Code Review)   │   │ (Context-Preserved Handoff)  │   │ (Multi-Doc Vector Sync)      │
└──────────────────────────────┘   └──────────────────────────────┘   └──────────────┬───────────────┘
                                                                                      │ (MCP Call)
                                                                                      ▼
                                                                       ┌──────────────────────────────┐
                                                                       │   OpenDocuments MCP         │
                                                                       │   (Vector DB + File Parsers)│
                                                                       │   [.xlsx,.pdf,.docx,.md]    │
                                                                       └──────────────────────────────┘
```

## 3. 核心數據合約與對齊機制 (Core Data Contract)

### 3.1 Workspace Alignment (鍵值對齊)

為防止多專案與 Monorepo 環境下的向量噪訊與誤判，Graphify Core 在初始化時生成定型之 `workspace_key`，並於 Graph Metadata 與 Plugin API 通訊中強制帶入。

```typescript
// Common Identity Schema
interface WorkspaceContext {
  workspace_key: string; // e.g., "w-9f8a2b1c-8e7d-4c3b"
  workspace_name: string; // e.g., "graphify-monorepo"
  root_path: string;      // e.g., "/Users/dev/projects/graphify"
  timestamp: number;
}
```

### 3.2 GraphifyPlugin Trait (v1)

已於 `graphify-core` 落地（`plugin.rs`，OpenSpec change `plugin-trait-v1`）。內嵌型插件 crate（如 `graphify-plugin-handoff`）實作此 trait 後由核心驅動：

```rust
pub trait GraphifyPlugin {
    fn get_id(&self) -> &str;
    fn bind(&mut self, ctx: WorkspaceContext);
    fn get_workspace_key(&self) -> &str;
    fn sync_toon(&mut self, opt_toon: Option<Vec<u8>>) -> Vec<u8>;
}
```

語意：
- `get_id` — 插件唯一識別碼（如 `"graphify-plugin-handoff"`）。
- `bind` — 綁定工作區上下文；綁定後 `get_workspace_key` 必須回傳與 `ctx.workspace_key` 相同值。
- `get_workspace_key` — 路由鑑別外鍵；未 bind 時回傳空字串。
- `sync_toon` — `Some(payload)` 為被動同步（消費外部 .toon），`None` 為主動同步（以綁定上下文自產輸出）；回傳處理後 `Vec<u8>`，不得 panic。

契約零依賴：僅 `std` + `serde`，不引入任何 LLM/HTTP/MCP 型別，維持 `graphify-core` 同步純粹性。reference 實作見 `graphify-core/src/plugin.rs` 測試。

#### sync_toon 封包契約（v1）

`sync_toon` 交換的是 **.toon 文件本體**（非自訂 envelope）：payload 即 .toon 序列化，版本承載於 metadata 的 `format_version` 鍵。

- **MUST metadata**：`format_version`（封包契約版本，v1 = `"1.0.0"`）、`workspace_key`（路由鍵）。
- **Optional 承載**：`symbol_nodes`、`graph_topology`，對齊下方 §3.3 Standard Plugin Communication Protocol 的對應視圖；存在與否不得影響封包有效性。
- **版本政策（semver）**：MAJOR 不符 → 解析端可（MAY）拒絕；MINOR 不符 → 可（MAY）忽略未知欄位；PATCH → 必須（MUST）相容。
- **錯誤表達**：無法產出有效輸出時，回傳含 `error` metadata 的 .toon（字串描述），不得 panic、不得改簽名。

完整規格：`openspec/changes/plugin-sync-toon-v1/specs/sync-toon-packet/spec.md`。

### 3.3 Standard Plugin Communication Protocol

Plugin 之間或對外曝露給 AI Agent 的 MCP 工具必須符合以下雙重響應格式：

```json
{
  "workspace_key": "w-9f8a2b1c-8e7d-4c3b",
  "symbol_nodes": [
    {
      "id": "graphify-core/src/lib.rs:module",
      "kind": "module",
      "filepath": "graphify-core/src/lib.rs"
    }
  ],
  "graph_topology": "import:pub use types::{Node, Edge} -> import:pub use extract::extract_file",
    "toon_payload": "compressed_toon_binary_or_text"
}
```

### 3.4 子進程 Plugin 主機（graphify-mcp plugin scanning，v1）

第三方 plugin 以獨立 MCP server 子進程形式存在，由 `graphify-mcp` 掃描並聚合。

- **掃描來源**：`~/.config/graphify/config.toml` 的 `[plugins.<id>]` 段（`command` 必填、`args`/`env`/`cwd` 選用）。缺檔或無該段時為空容器，不阻擋 server 啟動（故障隔離）。
- **進程模型**：啟動時 spawn，JSON-RPC 2.0 over stdio，`Content-Length` framing；`initialize` 握手失敗或逾時的 plugin 標記為 `Failed`，不影響其他 plugin（單一 plugin 失敗隔離）。
- **工具命名**：聚合工具以 `graphify_plugin_<plugin_id>_<tool_name>` 三段前綴避免命名衝突；`tools/call` 依此前綴路由回對應子進程。
- **圖更新通知**：`graph_reindex` 工具成功完成後，向所有 `Ready` plugin 子進程廣播 `notifications/graph_updated`（JSON-RPC notification，無回應預期）。
- **既有工具不變**：內建 `graphify_*` 工具維持原行為，plugin 聚合僅為增量。

### 3.5 Plugin-Domain Memory 邊界（memory-plugin-integration-v1）

Plugin 的長期記憶分三層，`workspace_key` 為跨層路由鍵（見 `docs/architecture-memory-plugin.md`）：

- **Layer 1 核心記憶**（graphify-llm + Qdrant）：由 indexing pipeline 獨佔寫入，plugin 只能透過受限查詢 API（`MemorySearcher`、`graphify_memory_query` 工具）讀取，無法寫入或取得儲存內部型別（point ID、collection 名、credentials）。
- **Layer 2 Plugin Domain Memory**：每個 plugin 一個獨立 namespace（`graphify_plugin_<plugin_id>`），記錄以版本化 envelope 儲存：

  ```
  PluginMemoryEnvelope<T> {
    format_version, workspace_key, plugin_id,
    record_id, record_kind, created_at, source_refs, payload: T
  }
  ```

  系統從 `plugin_id` 衍生實體名稱並驗證；plugin 不得提供原始 collection 名稱或 credentials（`graphify-memory::plugin_memory::plugin_collection_name`）。`HandoffSnapshot` 用可重建的查詢條件（workspace_key + node IDs + source paths）取代 Qdrant point ID。
- **Layer 3 外部知識**（OpenDoc / GitHub / Linear 等 adapter）：非本架構核心，由各 adapter 管理。

**不可寫核心記憶**：plugin-domain 寫入只允許進 Layer 2 的自身 namespace；核心記憶同步永遠由 indexing pipeline 擁有，不依賴任何 plugin 載入或收到 graph-update 事件。

### 3.6 Global Registry（sqlite-global-registry，v1）

跨 workspace 的註冊與同步狀態由 `graphify-registry` crate 以單一 SQLite 資料庫集中管理（路徑解析見 `docs/core.md`，`GRAPHIFY_REGISTRY_PATH` / `XDG_DATA_HOME` override）：

- **`workspaces`** — workspace 註冊（`workspace_key`、`root_path`、`is_active`、`last_indexed_at`）。CLI 提供 `graphify workspace list/switch/status`（TUI Stage 1 使用）。
- **`plugin_registrations`** — plugin ↔ workspace ↔ Qdrant collection 映射與 `last_synced_at`（一鍵 rehydration 的時間戳來源）、`status`（`Ready` / `Unavailable` 兩態）。
- **`handoff_registry`** — `HandoffSnapshot` 全域索引：`expires_at = created_at + 7 天`（TTL），每 workspace 上限 20 筆（FIFO pruning），寫入時單一 transaction 完成。

同步為被動式：無常駐 daemon（#3097），由 `graphify-registry::resync::check_and_resync` 在觸發點（CLI / TUI）以 10ms ping 檢查 provider 可用性，不可用即回 `Unavailable`。一鍵 rehydration（1.3.1）：`created_at > last_synced_at` 的 envelope 以 `record_id` idempotent upsert 回外部伺服器後更新 `last_synced_at`。

## 4. 三大核心 Plugin 詳細設計 (Plugin Specifications)

### 4.1 Code Review Plugin (`graphify-plugin-review`)

**定位**：拓撲感知代碼審查（Topology-Aware Code Review）。

**問題**：傳統 git diff 僅能進行單檔或行級別語法檢查，無法感知跨模組 Breaking Changes。

**解決方案**：
1. 擷取 git diff 修改之檔案與 Symbol 清單。
2. 呼叫 Graphify Core 執行 BFS Blast Radius Trace（爆炸半徑分析），計算向上與向下受影響之所有 1~N 階呼叫鏈。
3. 將「變更點」與「受影響拓撲子圖 (.toon)」餵給 AI Reviewer 進行全景審查。

```
[ Git Diff ] ──> Extract Modified Symbols ──> Graphify BFS Trace ──> Generate Blast Radius Sub-graph (.toon)
                                                                              │
                                                                              ▼
                                                                    AI Code Reviewer Prompt
```

### 4.2 Code Relay Handoff Plugin (`graphify-plugin-handoff`)

**定位**：結構化任務交接（Context-Preserved Handoff）。

**問題**：開發者或 Agent 交接時僅留下一段自然語言 Prompt，造成接手者上下文缺失或需重新掃描全案。

**解決方案**：
1. 自動追蹤本次開發 Session 中被讀取、修改或新增的關鍵 AST Nodes（Active Nodes）。
2. 提取以此群 Nodes 為中心之核心 Sub-graph (子圖)，序列化為極輕量之 .toon 格式。
3. 將交接紀錄與 .toon 拓撲存入 `.opencode/handoff.toon`，下位 Agent 載入即可 1 秒讀取全景結構。

### 4.3 OpenDoc Vector Plugin (`graphify-plugin-opendoc`)

**定位**：異構文檔與代碼樹之跨域檢索（Hybrid Neuro-Symbolic Search）。

**問題**：企業專案包含大量 .xlsx (試算表)、.pdf (規格書)、.docx (需求單)，無天然 AST 結構。

**解決方案**：
1. OpenDocuments MCP 專注處理非結構化文檔解析、Chunking 與 Embedding，並於 Vector DB 強制加上 `workspace_key` 標籤。
2. Graphify OpenDoc Plugin 作為橋樑，向 OpenDocuments 發起帶有 `workspace_key` 的語意檢索。
3. 從 OpenDocuments 檢索回傳之文檔片段中提取 `linked_symbol`，再由 Graphify 發射 16ms 靜態 Trace，精準補齊代碼實作鏈。

## 5. 跨模組協同工作流範例 (Workflow Walkthrough)

當使用者提出複雜需求：「根據產品規格書 (Excel) 裡的 Token 成本計算公式，檢查目前 Rust 實作是否有重構風險並進行交接。」

```
[ User / AI Agent Request ]
              │
              ▼
┌─────────────────────────────────────────────────────────┐
│ 1. Call `graphify-plugin-opendoc`                       │
│    - Pass: query = "Token 成本計算公式"                  │
│    - Pass: workspace_key = "w-9f8a2b1c"                │
└──────────────────────────┬──────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│ 2. OpenDocuments Vector MCP                             │
│    - Filter: workspace_key == "w-9f8a2b1c"             │
│    - Search: financial_plan.xlsx (Sheet1, Row 12)       │
│    - Return: Concept matched, Symbol: "MemoryConfig"    │
└──────────────────────────┬──────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│ 3. Call `graphify-plugin-review`                        │
│    - Source Symbol: "MemoryConfig"                      │
│    - Execute: Graphify Core 16ms BFS Impact Trace       │
│    - Output: Impact chain (.toon) -> 12 calling nodes   │
└──────────────────────────┬──────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│ 4. Call `graphify-plugin-handoff`                       │
│    - Export: Package active symbols + OpenDoc context   │
│    - Save: .opencode/handoff.toon for next Agent        │
└─────────────────────────────────────────────────────────┘
```

## 6. 開發路線圖 (Implementation Roadmap)

| 階段 | 時程 (週) | 核心 deliverable |
|---|---|---|
| Phase 1: Core Interface | Week 1 - 2 | 於 Graphify Core 實作 `workspace_key` 產生器，並定義 GraphifyPlugin Rust/MCP Trait。 |
| Phase 2: Review & Handoff | Week 3 - 4 | 開發 `graphify-plugin-review` 與 `graphify-plugin-handoff`，支援 .toon 子圖導出。 |
| Phase 3: OpenDoc Bridge | Week 5 - 6 | 建立 OpenDocuments MCP 協定對接，實現以 `workspace_key` 為基礎之多格式 (.xlsx, .pdf) 向量檢索橋樑。 |
| Phase 4: TUI Integration | Week 7 - 8 | 於 Ratatui TUI 主介面整合 Plugin 狀態檢視、BFS Modal 與 Handoff 快照開關。 |

## 7. 結論

本規劃書確立了以 Graphify Core 為精準結構基石、OpenDocuments 為語意向量擴充的**解耦架構**。透過 `workspace_key` 的硬性隔離與 MCP 協定串聯，Graphify 不再只是一個單機 CLI/TUI 工具，而是成為支援全生命周期 Agentic Workflow（檢索、審查、交接、規格同步）的核心基礎設施（AI Infrastructure）。
