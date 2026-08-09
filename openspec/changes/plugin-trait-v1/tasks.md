## 1. Core Trait Implementation (graphify-core)

- [ ] 1.1 新增 `graphify-core/src/plugin.rs`：定義 `WorkspaceContext`（`workspace_key` / `workspace_name` / `root_path` / `timestamp: i64`）與 `GraphifyPlugin` trait（`get_id` / `bind` / `get_workspace_key` / `sync_toon`），全部附 rustdoc 說明
- [ ] 1.2 在 `graphify-core/src/lib.rs` 匯出 `plugin` 模組與 `GraphifyPlugin`、`WorkspaceContext`
- [ ] 1.3 在 `plugin.rs` 加入 `#[cfg(test)]` reference 實作：實作 `GraphifyPlugin`，測試 `bind` → `get_workspace_key` 一致回傳、`sync_toon(Some(..))` 與 `sync_toon(None)` 均回傳預期輸出且不 panic；測試回傳 `Result` 並以 `?` 傳播

## 2. Docs Sync

- [ ] 2.1 更新 `docs/plugin_system.md`：補上 GraphifyPlugin trait v1 規格段（4 方法簽名、`WorkspaceContext` 契約、workspace_key 路由語意）
- [ ] 2.2 更新 `docs/core.md`：加入 crate 依賴圖（graphify-plugin-* 內嵌 crate → graphify-core，零 LLM/HTTP 依賴）

## 3. Verification

- [ ] 3.1 `cargo fmt` 通過
- [ ] 3.2 `cargo clippy --workspace --all-targets` 零警告（100% 乾淨）
- [ ] 3.3 `cargo test -p graphify-core` 全數通過（含新增 reference 測試）
- [ ] 3.4 `openspec validate` 通過，確認 change 合規
