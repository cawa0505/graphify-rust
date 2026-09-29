# 變更提案：graphify-mcp 領域聚合 (Domain Hub) 架構

## 1. 背景與現狀問題

目前 `graphify-mcp` 隨著專案演進，共宣告並暴露了 **33 個獨立 MCP Tools**：
- 核心圖譜 (11 個)：`graph_query`, `graph_path`, `graph_summary`, `graph_query_node`, `graph_trace_path`, `graph_reindex`, `skeleton_extract`, `workspace_status`, `memory_query`, `plugin_notify`, `graphify_help`
- Relay 外掛 (7 個)：`relay_init`, `relay_save`, `relay_close`, `relay_switch`, `relay_resume`, `relay_status`, `relay_add`
- OpenDoc 外掛 (3 個)：`opendoc_index`, `opendoc_get_context`, `opendoc_audit_drift`
- Review 外掛 (4 個)：`review_ingest`, `review_get_context`, `review_resolve`, `review_search_crg`
- Telemetry & Coverage (5 個)：`telemetry_ingest`, `telemetry_get_context`, `coverage_ingest`, `coverage_get_context`, `coverage_blindspots`
- Compose 外掛 (3 個)：`compose_read`, `compose_write`, `compose_render`

### 痛點：
1. **Prompt / Context 嚴重膨脹**：每次 LLM 初始化與接收工具列表時，33 個工具的 JSON Schema 佔用超過數千 tokens，極大浪費上下文預算。
2. **LLM 呼叫決策發散**：過於細碎的工具讓 LLM 容易在相似名稱間猶豫（例如 `graph_path` vs `graph_trace_path`）。

---

## 2. 解決方案：領域聚合 (Domain Hub Pattern)

不刪減任何功能，將 33 個工具按領域整合成 **6 大統一入口**，所有細部操作改為 `action` 參數（加對應的參數欄位）：

1. **`graphify_graph`** (圖譜核心操作)：
   - `action`: `query` | `summary` | `reindex` | `path` | `query_node` | `skeleton` | `workspace` | `memory`
2. **`graphify_relay`** (跨 Session 狀態接力)：
   - `action`: `status` | `init` | `save` | `close` | `switch` | `resume` | `add`
3. **`graphify_opendoc`** (規格與 Symbol 綁定/漂移)：
   - `action`: `get_context` | `index` | `audit_drift`
4. **`graphify_review`** (代碼審查與 CRG 關聯)：
   - `action`: `get_context` | `ingest` | `resolve` | `search_crg`
5. **`graphify_metrics`** (覆蓋率與遙測指標)：
   - `action`: `coverage_ingest` | `coverage_get` | `coverage_blindspots` | `telemetry_ingest` | `telemetry_get`
6. **`graphify_compose`** (跨 Workspace 架構組合)：
   - `action`: `read` | `write` | `render`

同時保留 legacy 單獨 tool 呼叫路由（若呼叫舊名則內部轉發），確保不破壞既有腳本相容性。
