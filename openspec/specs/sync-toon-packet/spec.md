# sync-toon-packet

## Purpose

定義 `GraphifyPlugin::sync_toon` 交換的封包契約：payload 為一組 .toon 文件，metadata 承載版本與路由鍵，讓內嵌插件、未來外部 SDK 與 `MCPPluginAdapter` 共享同一份可版本化、可驗證的封包定義。

## Requirements

### Requirement: sync_toon 封包即 .toon 文件

`sync_toon` 的傳入 payload（`Some(bytes)`）與回傳值皆必須（MUST）為一組符合 Graphify .toon 序列化格式的文件。傳入 `None` 表示主動同步，插件必須（MUST）以綁定上下文自產輸出。插件不得（MUST NOT）回傳非 .toon 格式的位元組作為成功結果；無法產生有效輸出時必須（MUST）回傳 metadata 含 `error` 欄位的 .toon 文件，不得 panic。

#### Scenario: 被動同步消費外部 .toon
- **WHEN** 呼叫 `sync_toon(Some(payload))` 且 `payload` 為有效 .toon 文件
- **THEN** 插件解析並處理該文件，回傳處理後的有效 .toon 文件位元組

#### Scenario: 主動同步自產輸出
- **WHEN** 呼叫 `sync_toon(None)`
- **THEN** 插件以綁定之 `WorkspaceContext` 產生 .toon 輸出並回傳，不得 panic，且輸出 metadata 含正確 `workspace_key`

#### Scenario: 無法產生輸出時回傳錯誤 metadata
- **WHEN** 插件綁定上下文不足以產出有效 .toon（如未 bind 即呼叫主動同步）
- **THEN** 插件回傳 metadata 含 `error` 欄位的 .toon 文件，而非空位元組或 panic

### Requirement: metadata 必須欄位

封包 .toon 文件的 metadata 區段必須（MUST）包含以下欄位：`format_version`（字串，語意化版本，本規格 v1 為 `"1.0.0"`）與 `workspace_key`（字串，路由鑑別鍵，值與 `get_workspace_key()` 回傳一致）。缺少任一 MUST 欄位的封包視為無效。

#### Scenario: 封包帶有版本與路由鍵
- **WHEN** 插件產生 sync_toon 輸出
- **THEN** 輸出 .toon metadata 同時含 `format_version` 與 `workspace_key`，且 `workspace_key` 與 `get_workspace_key()` 一致

#### Scenario: 缺 MUST 欄位視為無效
- **WHEN** 解析端收到缺 `format_version` 或 `workspace_key` 的 .toon 封包
- **THEN** 解析端必須（MUST）以 metadata 含 `error` 的 .toon 回應，不得以 panic 中斷

### Requirement: optional 承載欄位

封包 .toon 文件可（MAY）包含 `symbol_nodes`（符號節點陣列）與 `graph_topology`（拓撲摘要字串）等 optional 承載，對齊 `docs/plugin_system.md` §3.2 Standard Plugin Communication Protocol 的對應視圖。解析端必須（MUST）容忍 optional 欄位缺失——optional 欄位存在與否不得影響封包有效性。

#### Scenario: optional 承載依插件需要呈現
- **WHEN** 插件需要傳遞符號或拓撲資訊
- **THEN** 封包可含 `symbol_nodes` / `graph_topology`；不需要時可省略，封包仍有效

### Requirement: 版本政策

`format_version` 遵循語意化版本（MAJOR.MINOR.PATCH）。MAJOR 相同（本規格 v1 即 `1.x.x`）時，解析端必須（MUST）能安全消費對端 payload；MAJOR 提升表示破壞性變更，解析端可（MAY）拒絕並以 `error` metadata 回應。PATCH 變更僅為修正，不影響相容性。

#### Scenario: 同 MAJOR 版本可互操作
- **WHEN** 對端封包 `format_version` 為 `1.2.0` 而本端支援 `1.x`
- **THEN** 解析端正常處理，不因 minor/patch 差異拒絕

#### Scenario: MAJOR 不符時拒絕
- **WHEN** 對端封包 `format_version` 為 `2.0.0` 而本端僅支援 `1.x`
- **THEN** 解析端以 metadata 含 `error`（說明版本不符）的 .toon 回應

### Requirement: 契約零依賴與簽名凍結

本封包規格不得（MUST NOT）要求 `graphify-core` 新增任何 LLM、HTTP、MCP 依賴或型別。`GraphifyPlugin::sync_toon` 簽名 `fn sync_toon(&mut self, opt_toon: Option<Vec<u8>>) -> Vec<u8>` 維持 plugin-trait-v1 定案契約，不因本規格而改變。

#### Scenario: 簽名與依賴維持凍結
- **WHEN** 檢視 graphify-core 對 sync_toon 的依賴面
- **THEN** 僅 `std` + `serde`，簽名與 plugin-trait-v1 一致，無新增外部依賴
