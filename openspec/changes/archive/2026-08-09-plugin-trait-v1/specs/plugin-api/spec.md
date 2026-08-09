## Purpose

定義 GraphifyCore 的插件契約（GraphifyPlugin trait v1），讓 `graphify-plugin-handoff` 等內嵌型 crate 能以統一的綁定、路由鑑別與 .toon 同步流程掛載到 Graphify 核心，同時維持 graphify-core 零 LLM/HTTP 依賴。

## ADDED Requirements

### Requirement: GraphifyPlugin trait 契約

GraphifyCore 必須（MUST）提供 `GraphifyPlugin` trait，供內嵌型插件 crate 實作。trait 必須包含四個方法：`get_id` 回傳插件唯一識別碼（`&str`）、`bind` 接收 `WorkspaceContext` 並綁定工作區上下文、`get_workspace_key` 回傳綁定後的工作區 UUID、`sync_toon` 接收可選的 .toon 載荷並回傳處理後的位元組輸出。trait 定義不得（MUST NOT）依賴任何 LLM、HTTP 或 MCP 相關型別，保持 graphify-core 的同步純粹性。

#### Scenario: 插件可取得唯一識別碼
- **WHEN** 呼叫實作了 `GraphifyPlugin` 的插件實例之 `get_id()`
- **THEN** 回傳該插件的唯一識別字串（如 `"graphify-plugin-handoff"`）

#### Scenario: 插件綁定工作區上下文
- **WHEN** 呼叫 `bind(ctx)` 傳入含 `workspace_key` 的 `WorkspaceContext`
- **THEN** 插件內部記錄該上下文，且後續 `get_workspace_key()` 回傳與 `ctx.workspace_key` 相同的值

#### Scenario: 插件同步 .toon 載荷
- **WHEN** 呼叫 `sync_toon(Some(toon_bytes))` 或 `sync_toon(None)`
- **THEN** 回傳 `Vec<u8>` 形式的處理後輸出；傳入 `None` 時插件必須能以既有綁定上下文產生輸出，不得 panic

### Requirement: WorkspaceContext 資料契約

GraphifyCore 必須（MUST）提供 `WorkspaceContext` 結構，包含 `workspace_key`（路由鑑別外鍵）、`workspace_name`、`root_path` 與 `timestamp` 欄位，與 `docs/plugin_system.md` §3.1 的介面契約一致。`workspace_key` 是 opendoc-mcp → graphify → plugins 之間路由鑑別的硬性對齊鍵。

#### Scenario: WorkspaceContext 承載路由鑑別鍵
- **WHEN** 建立 `WorkspaceContext` 並填入 `workspace_key`
- **THEN** 該 UUID 可透過 `bind` 傳入插件，並經 `get_workspace_key` 一致回傳，供跨模組路由比對

### Requirement: 可實作性驗證

GraphifyCore 必須（MUST）提供至少一個 `GraphifyPlugin` 的 reference 實作範例（測試或 example），證明外部 crate 可以實作此 trait 並通過綁定與同步流程，且不依賴圖表輸出以外的任何外部服務。

#### Scenario: reference 實作可被實例化與驅動
- **WHEN** 以測試或 example 實作 `GraphifyPlugin`，依次呼叫 `bind`、`get_workspace_key` 與 `sync_toon`
- **THEN** 全部方法正常回傳預期結果，測試通過
