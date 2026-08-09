# design: plugin-scan-v1

## Context

graphify-mcp 目前是純 MCP server（~495 行），6 個內建工具硬編碼在 `handle_request` 的 match 中，採用換行分隔的 JSON-RPC over stdio（非標準 Content-Length framing）。無 plugin 基礎設施、無進程管理、config 無 `[plugins]` 段。

外部 SDK 協定已定案（docs/plugin-sdk-thirdparty.md）：第三方 plugin = 標準 MCP server 子進程，Stdio + JSON-RPC，符合 MCP 2025-11-25 規範。本 change 實作 Mode 1 Gateway 角色：graphify-mcp 聚合 plugin 工具並轉發呼叫。

## Goals

- graphify-mcp 能從 config 掃描並啟動第三方 MCP plugin 子進程
- 工具聚合（`graphify_plugin_<id>_<tool>` 前綴）與呼叫轉發
- `notifications/graph_updated` 轉發給健康 plugin（對齊 plugin-events-v1）
- 單個 plugin 失敗不影響 server 與其他 plugin

## Non-Goals

- 不實作 plugin 動態卸載/重載（P1 lifecycle 範圍）
- 不實作 MCPPluginAdapter（第一方 v1 trait ↔ 外部 SDK 橋接，無消費者不做）
- 不改 graphify-core / graphify-llm 依賴（#3088）
- 不處理 D6 其餘子問題（打包格式、啟動時機、設定位置沿用 roadmap [待討論]）

## Decisions

### D1: plugin 子進程由誰管理

graphify-mcp 直接管理 plugin 子進程（spawn / init / health 追蹤），不放 graphify-core。

- 理由：外部 SDK 是 graphify-mcp 的角色（Mode 1 Gateway），core 維持零依賴（#3088）；cli 的 `PluginHost`（第一方 v1 trait）與此處的第三方進程管理是兩層，不混用
- 替代：放在 graphify-cli——被拒，graphify-mcp 需要進程存活的同步請求/回應循環，cli 是命令式一次性執行

### D2: framing 層

graphify-mcp 對 plugin 子進程使用標準 MCP Content-Length framing（`Content-Length: <n>\r\n\r\n` + JSON body），與 MCP 2025-11-25 規範一致；graphify-mcp 對外 client 連線維持現況（換行分隔）。

- 理由：第三方 plugin 按 MCP 規範實作（SDK 側已定義），graphify-mcp 必須用標準 framing 才能通訊；對外連線格式是既有行為，不在此 change 改動
- 替代：對外連線也改 Content-Length——被拒，破壞現有 client 相容性，超範圍

### D3: 工具命名空間

`graphify_plugin_<plugin_id>_<original_tool_name>` 三段式前綴。

- 理由：對齊已定案的 SDK 慣例（lib-1 研究：Go mcp-gateway 用 `gateway_` 前綴、prooflayer 用 `plugin_prefix_`）；plugin id 段提供跨 plugin 隔離（同工具名不同 plugin 不衝突），`graphify_plugin_` 固定段區分內建工具
- 替代：僅 `graphify_<id>_<tool>`——被拒，與內建工具前綴語意混淆

### D4: plugin 失敗隔離

每個 plugin 子進程的初始化/呼叫失敗獨立追蹤；失敗 plugin 的工具從 `tools/list` 排除，呼叫時回明確錯誤。

- 理由：對齊 cli `PluginHost::broadcast` 的 catch_unwind 隔離哲學（#3143）；外部進程的失敗模式更多（spawn 失敗、超時、非 JSON 輸出），需要狀態機
- 替代：失敗即終止整個 server——被拒，違反 plugin 獨立性原則

### D5: 圖更新通知來源

graphify-mcp 從何處得知圖更新？現況 graphify-mcp 是 pull-based（工具觸發 graphify 操作），無 index/extract 主流程。

- 設計：`notifications/graph_updated` 的發送點在 graphify-mcp 內的工具（graphify_query / graphify_path 等觸發萃取/索引的工具）完成後廣播，加上 `graphify_notify_plugins` 手動觸發工具；本 change 先提供手動觸發 + 工具完成後廣播兩條路徑
- 理由：graphify-mcp 無常駐 daemon（#3097），事件必然來自工具調用或手動觸發；D4（plugin-events-v1）在 cli 側已實作同語義，此處是 MCP 側對等物
- 替代：graphify-mcp 監聽檔案系統變更——被拒，超範圍（#3097 已定案 pull-based）

## Risks / Trade-offs

- [plugin 子進程洩漏] → 每個 plugin 進程在 server 退出時由 OS 回收；不主動 kill（D6b 啟動時機未定案前不引入複雜 lifecycle）
- [非標準輸出污染 framing] → stderr 重導向到 server 日誌，stdout 僅保留給 JSON-RPC；初始化握手失敗即標記失敗
- [工具名過長] → 三段式前綴可能讓工具名偏長（`graphify_plugin_<id>_<tool>`）；接受，MCP 生態工具名本就可長，唯一性優先

## Migration Plan

- config.toml 新增 `[plugins.<id>]` 段（選用）：`command`（必填）、`args`、`env`、`cwd`（選用）
- 無 plugin 段時 server 行為不變（向後相容）
- 回滾：移除 `[plugins]` 段即回到現況

## Open Questions

- 無（D6 其餘子問題標記在 roadmap 為 [待討論]，不阻擋本 change；真正需要本 change 定案的已在 D1-D5 定案）
