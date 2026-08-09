# plugin-scanning

## Purpose

定義 graphify-mcp 掃描、載入、聚合第三方 MCP plugin 的行為契約，使第三方 plugin 能以標準 MCP 協定（Stdio + JSON-RPC over Content-Length framing）接入 graphify 生態，並以命名空間前綴避免工具名稱衝突。

## ADDED Requirements

### Requirement: 掃描宣告的 plugin 設定

graphify-mcp 在啟動時從設定檔讀取 `[plugins.<id>]` 段的 plugin 宣告。每段必須（MUST）包含 `command` 欄位（plugin 可執行檔路徑或名稱），可（MAY）包含 `args`、`env`、`cwd`。無任何 plugin 宣告時，graphify-mcp 行為與未啟用 plugin 掃描的現況完全一致（內建 6 工具照常提供）。

#### Scenario: 讀取 plugin 宣告

WHEN graphify-mcp 啟動且設定檔包含 `[plugins.mytool]` 段
THEN graphify-mcp 建立對應的 plugin 子進程，並以 `command` + `args` 作為啟動參數

#### Scenario: 無 plugin 宣告

WHEN graphify-mcp 啟動且設定檔無任何 `[plugins.*]` 段
THEN graphify-mcp 不啟動任何子進程，且 `tools/list` 僅回傳內建工具

#### Scenario: 無效的 plugin 設定

WHEN `[plugins.<id>]` 段缺少 `command` 欄位
THEN graphify-mcp 記錄錯誤並略過該 plugin，不中斷 server 啟動

### Requirement: plugin 子進程通訊

graphify-mcp 與 plugin 子進程透過 stdin/stdout 以 JSON-RPC 2.0 訊息通訊，訊息必須（MUST）以 MCP 規範的 Content-Length framing（`Content-Length: <n>\r\n\r\n` + JSON body）封裝。plugin 子進程的 stderr 不得（MUST NOT）污染 JSON-RPC 資料流。

#### Scenario: 初始化握手

WHEN plugin 子進程啟動完成
THEN graphify-mcp 發送 `initialize` 請求，並在收到成功回應後發送 `notifications/initialized` 通知

#### Scenario: 初始化失敗

WHEN plugin 子進程對 `initialize` 回傳錯誤或超過等待時間無回應
THEN graphify-mcp 將該 plugin 標記為失敗狀態，其工具不出現在 `tools/list`，且不影響其他 plugin

### Requirement: 工具聚合與命名空間前綴

graphify-mcp 在 `tools/list` 回應中合併內建工具與所有健康 plugin 的工具。來自 plugin 的工具名稱必須（MUST）以 `graphify_plugin_` 前綴加上 plugin id 與原始工具名組合成唯一名稱，格式為 `graphify_plugin_<plugin_id>_<original_tool_name>`，防止不同 plugin 之間及與內建工具的命名衝突。plugin 之間的命名空間相互獨立。

#### Scenario: 聚合 plugin 工具清單

WHEN plugin `mytool` 宣告工具 `search` 且 server 收到 `tools/list` 請求
THEN 回應包含內建工具以及名為 `graphify_plugin_mytool_search` 的工具

#### Scenario: 工具名稱衝突

WHEN 兩個 plugin `alpha` 與 `beta` 各自宣告工具 `search`
THEN `tools/list` 同時包含 `graphify_plugin_alpha_search` 與 `graphify_plugin_beta_search`，兩者互不干擾

### Requirement: 工具呼叫轉發

graphify-mcp 收到 `tools/call` 請求時，若工具名稱符合 `graphify_plugin_<plugin_id>_<tool>` 格式，必須（MUST）將呼叫轉發給對應 plugin 的 `tools/call`，並將 plugin 的完整回應原樣回傳給 client。若對應 plugin 不可用（失敗/未載入），必須（MUST）回傳錯誤，不得靜默丟棄。

#### Scenario: 轉發工具呼叫

WHEN client 呼叫工具 `graphify_plugin_mytool_search` 帶參數 `{"q": "foo"}`
THEN graphify-mcp 向 plugin `mytool` 發送 `tools/call` 請求（工具名 `search`，參數 `{"q": "foo"}`），並回傳 plugin 的回應

#### Scenario: plugin 不可用

WHEN client 呼叫 `graphify_plugin_mytool_search` 但 plugin `mytool` 處於失敗狀態
THEN graphify-mcp 回傳錯誤訊息，指明 plugin 不可用

### Requirement: 圖更新事件通知

graphify 完成索引或萃取（產生圖更新）後，graphify-mcp 必須（MUST）向健康 plugin 發送 `notifications/graph_updated` 通知，承載 `workspace_key` 與更新事件類型。此通知對齊 plugin-events-v1 的 `on_graph_updated` 語義，是外部 plugin 接收圖更新事件的唯一通道。

#### Scenario: 索引完成後通知

WHEN graphify 完成索引且存在健康 plugin
THEN graphify-mcp 向每個健康 plugin 發送 `notifications/graph_updated`，承載 `workspace_key` 與 `kind`（indexed/extracted/manual）

#### Scenario: 無健康 plugin

WHEN graphify 完成索引且無任何健康 plugin
THEN graphify-mcp 不發送通知，且不影響索引流程
