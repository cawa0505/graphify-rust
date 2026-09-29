# 規格細則：領域聚合 (Domain Hub)

## 需求 1：工具列表精簡為 6 大領域入口
- **GIVEN** Client 發送 `tools/list` 請求
- **WHEN** graphify-mcp 產生可用工具清單
- **THEN** 回傳的 tools 數量必須為 6 個，分別為：
  - `graphify_graph`
  - `graphify_relay`
  - `graphify_opendoc`
  - `graphify_review`
  - `graphify_metrics`
  - `graphify_compose`

## 需求 2：action 參數導向轉發
- **GIVEN** Client 發送 `tools/call` 呼叫聚合工具（如 `graphify_relay`）
- **WHEN** 帶有合法的 `action` 參數（如 `action="status"`）
- **THEN** 網關必須正確解析並轉發至相應的實作，回傳結構與原個別工具一致。
- **WHEN** 缺少 `action` 或傳入未知的 `action`
- **THEN** 必須回傳明確的錯誤訊息，列出該領域支援的所有有效 action。

## 需求 3：歷史舊工具相容
- **GIVEN** 既有腳本或歷史 Agent 呼叫舊工具名稱（如 `graphify_graph_summary`）
- **WHEN** 呼叫抵達 `tools/call`
- **THEN** 網關內部自動適配轉發，不得回傳 `MethodNotFound`。
