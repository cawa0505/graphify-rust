# plugin-sync-toon-v1 — Tasks

## 1. 文件同步

- [x] 1.1 更新 `docs/plugin_system.md` §3.2：補上 sync_toon 封包契約（.toon 文件、MUST metadata `format_version`/`workspace_key`、optional `symbol_nodes`/`graph_topology`、semver 政策、`error` metadata 錯誤表達）
- [x] 1.2 更新 `docs/core.md` Plugin API 段：補 sync_toon 封包規格摘要與指向 openspec change 的連結
- [x] 1.3 確認 `graphify-core/src/plugin.rs` 的 trait rustdoc 與 `docs/plugin_system.md` §3.2 對 sync_toon 的描述一致（不寫代碼，僅檢視；如有偏離記錄於 1.4）

## 2. 驗證

- [x] 2.1 `openspec validate plugin-sync-toon-v1` 通過（4 artifacts：proposal / spec / design / tasks）
- [x] 2.2 對照 spec.md 逐條確認文件已涵蓋 5 個 Requirements（封包即 .toon、MUST 欄位、optional 承載、版本政策、零依賴簽名凍結）
- [x] 2.3 與用戶確認後 archive change（`openspec archive`）或保留為 in-progress
