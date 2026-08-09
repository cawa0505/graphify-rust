# plugin-sync-toon-v1 — sync_toon 封包規格

## Why

`GraphifyPlugin::sync_toon(&mut self, opt_toon: Option<Vec<u8>>) -> Vec<u8>` 目前交換裸位元組，無版本、無欄位契約，插件間無法判斷對端資料格式。凍結封包契約後，`graphify-plugin-handoff` 等插件可依穩定規格建構，也為未來 `MCPPluginAdapter` 橋接外部插件提供同一份封包定義。

## What Changes

- 定義 `sync_toon` 封包 = 一組 .toon 文件（`Some(payload)` / 回傳值皆為 .toon 序列化）。
- 定義 MUST / optional 欄位表：metadata 必須含 `format_version` 與 `workspace_key`；`symbol_nodes`、`graph_topology` 為 optional 視插件需要。
- 定義 semver 政策：`format_version` 為語意化版本，major 不變時舊插件可安全消費新 payload。
- 文件層級：更新 `docs/plugin_system.md` §3.2 與 `docs/core.md`，讓封包契約與既有「Standard Plugin Communication Protocol」視圖一致。
- **無代碼變更**：純規格凍結（trait 簽名不動，維持 plugin-trait-v1 已定案契約）。

## Capabilities

- **New Capabilities**: `sync-toon-packet` — sync_toon 封包結構、欄位契約與版本政策。

## Impact

- 受影響文件：`docs/plugin_system.md`、`docs/core.md`。
- 受影響代碼：無（純規格；trait 簽名維持 `Option<Vec<u8>> -> Vec<u8>`）。
- 依賴：零新增（graphify-core 維持 std + serde）。
- 未來依賴方：`graphify-plugin-handoff`、`graphify-plugin-review`、`MCPPluginAdapter` 皆以本封包為共同契約。
