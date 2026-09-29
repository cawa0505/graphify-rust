# 規格細則：領域聚合 (Domain Hub)

## ADDED Requirements

### Requirement: 工具列表精簡為 6 大領域入口

`tools/list` SHALL 只回傳 6 個領域入口工具（`graphify_graph`、`graphify_relay`、`graphify_opendoc`、`graphify_review`、`graphify_metrics`、`graphify_compose`），細粒度操作改以 `action` 參數表達。

#### Scenario: tools/list 數量與名稱

- **GIVEN** Client 發送 `tools/list` 請求
- **WHEN** graphify-mcp 產生可用工具清單
- **THEN** 回傳的 tools 數量必須為 6 個，分別為：
  - `graphify_graph`
  - `graphify_relay`
  - `graphify_opendoc`
  - `graphify_review`
  - `graphify_metrics`
  - `graphify_compose`

### Requirement: action 參數導向轉發

`tools/call` 對聚合工具 SHALL 依 `action` 參數解析並轉發至對應 legacy 實作；缺漏或未知的 `action` SHALL 回傳明確錯誤並列出該領域所有有效 action。

#### Scenario: 合法 action 轉發

- **GIVEN** Client 發送 `tools/call` 呼叫聚合工具（如 `graphify_relay`）
- **WHEN** 帶有合法的 `action` 參數（如 `action="status"`）
- **THEN** 網關必須正確解析並轉發至相應的實作，回傳結構與原個別工具一致。

#### Scenario: 缺漏或未知 action 報錯

- **GIVEN** Client 發送 `tools/call` 呼叫任一聚合工具
- **WHEN** 缺少 `action` 或傳入未知的 `action`
- **THEN** 必須回傳明確的錯誤訊息，列出該領域支援的所有有效 action。

### Requirement: 歷史舊工具相容

legacy 個別工具名稱 SHALL 繼續保持可用（內部自動適配轉發），作為相容層。

#### Scenario: 舊工具名稱照常運作

- **GIVEN** 既有腳本或歷史 Agent 呼叫舊工具名稱（如 `graphify_graph_summary`）
- **WHEN** 呼叫抵達 `tools/call`
- **THEN** 網關內部自動適配轉發，不得回傳 `MethodNotFound`。
