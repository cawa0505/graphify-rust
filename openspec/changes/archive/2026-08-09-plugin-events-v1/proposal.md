## Why

D4 決策已定案（見 docs/plugin-sdk-roadmap.md）：自家 plugin 需要圖更新事件的接收機制。目前 `graphify index` / `extract` 執行後沒有任何事件廣播，plugin 無法得知代碼變更或 AST 重新解析——這阻斷了自家 plugin（review / handoff / opendoc）的事件驅動能力，也是日後第三方 SDK 協議 `onGraphUpdated` 的內建對應。

## What Changes

- 在 `graphify-core` 新增 `GraphUpdateEvent` 型別：`workspace_key` + `modified_nodes: Vec<NodeId>` + `event` kind（`Indexed` / `Extracted` / `Manual`）。
- 擴充 `GraphifyPlugin` trait：新增 `on_graph_updated(&mut self, event: &GraphUpdateEvent)` 方法（**提供預設空實作，不破壞既有實作**）。
- `graphify-cli` 的 `run_index` / extract 執行完成後，對所有已 bind 的 plugin 廣播 `on_graph_updated` 事件。
- 新增 `graphify plugin run-hooks` 子命令：手動觸發 `Manual` 事件（供 scripts/CI 使用）。

## Capabilities

### New Capabilities
- `plugin-events`: 圖更新事件的型別定義、trait 擴充、index/extract 後自動廣播、手動觸發命令（`graphify plugin run-hooks`）。

### Modified Capabilities
<!-- 無既有 spec 行為變更 -->

## Impact

- `graphify-core/src/plugin.rs`：新增 `GraphUpdateEvent`、trait 擴充 `on_graph_updated`（預設空實作）。
- `graphify-cli/src/main.rs`：`run_index` 完成後廣播；新增 `plugin run-hooks` 子命令（`graphify-cli/src/plugin.rs` 或沿用 main.rs 結構）。
- 既有 `GraphifyPlugin` 實作（含測試）不受破壞（預設空實作 backward compatible）。
- 無新依賴。
