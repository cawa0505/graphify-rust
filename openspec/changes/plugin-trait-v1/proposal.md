# Change: plugin-trait-v1

## Why

`docs/plugin_system.md` 規劃了微核心 Plugin 架構（review / handoff / opendoc 三大插件），以 `workspace_key` 作為 opendoc-mcp → graphify → plugins 間的路由鑑別外鍵。目前 GraphifyCore 沒有任何 Plugin 抽象，無法承載 `graphify-plugin-handoff` 等內嵌型 crate。v1 需先落地最小 Plugin Trait，作為所有內嵌插件的契約起點。

## What Changes

- 在 `graphify-core` 定義 `GraphifyPlugin` trait（v1）：
  - `get_id(&self) -> &str`：插件唯一識別碼。
  - `bind(&mut self, ctx: WorkspaceContext)`：綁定工作區上下文（含 `workspace_key`）。
  - `get_workspace_key(&self) -> &str`：回傳綁定後的工作區 UUID（路由鑑別用）。
  - `sync_toon(&mut self, opt_toon: Option<Vec<u8>>) -> Vec<u8>`：同步 .toon 載荷並回傳處理後輸出。
- 新增 `WorkspaceContext` 結構（`workspace_key`、`workspace_name`、`root_path`、`timestamp`），與 `docs/plugin_system.md` §3.1 契約一致。
- 提供一個可用的 reference 實作範例（example 或 test），證明 trait 可被外部 crate 實作。
- 在 `docs/` 同步 Trait 規格與 crate 依賴圖（plugin 如何依附 graphify-core，且不引入 LLM/MCP 依賴）。

## Capabilities

### New Capabilities

- `plugin-api`: GraphifyPlugin trait、WorkspaceContext 與綁定/同步契約，作為內嵌插件（handoff 等）的穩定接口。此規格不引入任何 MCP 協定，保持 graphify-core 零 LLM/HTTP 依賴。

### Modified Capabilities

<!-- 無既有規格受影響：graphify-core 維持同步、零依賴，不改變 extraction-schema 或其他既有規格 -->

## Impact

- 代碼：`graphify-core/src/` 新增 `plugin.rs`（含 trait 與 `WorkspaceContext`），`lib.rs` 匯出；約 <100 行。
- 無新依賴：保持 std + serde 即可，符合 graphify-core「零 LLM/HTTP」原則。
- 文件：`docs/plugin_system.md` 更新 Trait 規格段；`docs/core.md` 更新依賴圖。
- 向後相容：純新增，不修改既有 Node/Edge/GraphOutput 結構。
