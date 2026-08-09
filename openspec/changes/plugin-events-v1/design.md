## Context

See proposal.md — Why. Current state: `graphify-core::plugin` 已實作 `GraphifyPlugin` trait（`get_id` / `bind` / `get_workspace_key` / `sync_toon`）與 `WorkspaceContext`（含 `workspace_key`）。CLI 的 `run_index`（main.rs:508）是同步 pipeline，完成後無事件機制。`graphify-cli` 以 300-line 上限模組化（main.rs / skill.rs / snapshot.rs / tui.rs）。

## Goals / Non-Goals

**Goals:**
- 讓自家 plugin 能接收圖更新事件（D4 決策落地）
- trait 擴充 backward compatible（既有實作不破壞）
- index/extract 完成後自動廣播 + 手動觸發

**Non-Goals:**
- 第三方 SDK 協議（`plugin-host-mcp` 另開 change，D7a 定案不進 core）
- 常駐 daemon / 檔案監看（D4 已排除 Option A）
- plugin registry（discovery/lifecycle）— 超出本 change
- MCP Adapter（D7b 延後）

## Decisions

**D1: `GraphUpdateEvent` 型別放 `graphify-core::plugin`**
與 trait 同居模組，避免循環依賴；`workspace_key: String` + `modified_nodes: Vec<NodeId>` + `event: GraphUpdateKind`。`GraphUpdateKind` 為 enum（`Indexed` / `Extracted` / `Manual`），可序列化供日後協議對接。
- 替代：放 CLI —— 否決，trait 簽名需要它，放 core 才無循環依賴。

**D2: trait 擴充採預設空實作 `on_graph_updated(&mut self, event: &GraphUpdateEvent) {}`**
既有 plugin 不需改任何程式碼即可編譯；有意願者覆寫。符合 spec「不破壞既有實作」需求。
- 替代：trait 新增必要方法 —— 會破壞 `EchoHandoffPlugin` 與既有測試，否決。

**D3: 廣播點在 `run_index` / extract 完成後、回傳前**
同步呼叫所有已 bind plugin 的 `on_graph_updated`；`bind` 時 plugin 以 `Vec` 存於呼叫端（graphify-cli）。失敗語意：單一 plugin hook 失敗不中斷其他 plugin（記 stderr），回傳 `Result` 給命令層決定是否報錯。
- 替代：非同步 spawn —— CLI 是同步流程，過度設計，否決。

**D4: `plugin run-hooks` 子命令走既有 CLI 結構**
main.rs 的 match 增加 `plugin` 子命令分支，`GraphUpdateKind::Manual` 事件廣播。無 plugin 時 exit 0（spec 要求）。

**D5: `workspace_key` 產生規則（架構決策 A 落地）**
`workspace_key` 是 graphify 對 workspace root 的穩定身份：`canonicalize(root_path)` 後以 std `DefaultHasher`（SipHash）求 64-bit hash 再轉 hex 字串。可重現（同一路徑跨 process/machine 同 key）、零新依賴（#3088）。與 opendoc-mcp workspace UUID 無關——後者是 plugin 層概念，由 opendoc plugin 於 bind 時自行對映。
- 替代：uuid crate 隨機生成 —— 無法重現，跨 CLI/plugin 對不齊，否決。
- 產生 helper 放 `graphify-core::plugin`（與 `WorkspaceContext` 同居，供 CLI 與 plugin 共用同一規則）。

## Risks / Trade-offs

- [trait 擴充長期鎖定型別] → `GraphUpdateKind` 用 enum + `#[non_exhaustive]` 保留未來 kind 擴充空間。
- [modified_nodes 語意因 run 而異] → index 傳有變更的檔案節點、extract 傳該 run 產生的節點；spec 以「該 run 影響的節點」表述，避免過度承諾。

## Migration Plan

- 純增量：新增型別 + trait 預設方法 + CLI 廣播，無既有行為變更，無 rollback 需求。

## Open Questions

- 無（D4 決策已收斂；協議對接屬 `plugin-host-mcp`）。
