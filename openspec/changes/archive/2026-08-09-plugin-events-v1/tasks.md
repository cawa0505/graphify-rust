## 1. Core: GraphUpdateEvent 型別

- [x] 1.1 在 `graphify-core/src/plugin.rs` 新增 `GraphUpdateKind` enum（`Indexed` / `Extracted` / `Manual`，`#[non_exhaustive]` + `Copy + Clone + Debug + PartialEq`）
- [x] 1.2 新增 `GraphUpdateEvent` struct：`workspace_key: String` + `modified_nodes: Vec<NodeId>` + `event: GraphUpdateKind`

## 2. Core: trait 擴充

- [x] 2.1 `GraphifyPlugin` trait 新增 `on_graph_updated(&mut self, event: &GraphUpdateEvent)`，**預設空實作** `{}`（backward compatible，不破壞既有實作）
- [x] 2.2 為 `EchoHandoffPlugin` reference 實作覆寫 `on_graph_updated` 並記錄收到的事件（供測試驗證）

## 3. Core: 測試

- [x] 3.1 測試：bind 後收到 `on_graph_updated` 事件且欄位正確（workspace_key / modified_nodes / kind）
- [x] 3.2 測試：既有 plugin（未覆寫 hook）bind 後 index 廣播不報錯
- [x] 3.3 測試：`GraphUpdateKind` 三種 kind 可構造與比較

## 4. CLI: index/extract 後廣播

- [x] 4.1 `run_index`（main.rs:508）在成功完成後，對所有已 bind plugin 呼叫 `on_graph_updated`（`GraphUpdateKind::Indexed`，modified_nodes 取自該 run 影響的節點）
- [x] 4.2 extract 流程完成後同樣廣播（`GraphUpdateKind::Extracted`）
- [x] 4.3 單一 plugin hook 失敗不中斷其他 plugin（錯誤記 stderr，命令層視情況回報）

## 5. CLI: plugin run-hooks 子命令

- [x] 5.1 main.rs 增加 `plugin run-hooks` 子命令，廣播 `GraphUpdateKind::Manual` 事件
- [x] 5.2 無 plugin bind 時 exit 0（不發事件）
- [x] 5.3 hook 執行失敗時回報明確錯誤

## 6. 驗證

- [x] 6.1 `cargo fmt --all --check` 通過
- [x] 6.2 `cargo clippy --workspace --all-targets` 零警告
- [x] 6.3 `cargo test --workspace` 全數通過
- [x] 6.4 `openspec validate plugin-events-v1` 通過
