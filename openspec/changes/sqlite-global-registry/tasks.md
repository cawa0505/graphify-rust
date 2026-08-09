# sqlite-global-registry — Tasks

## 1. graphify-registry crate 骨架

- [x] 1.1 建立 `graphify-registry` crate（workspace member + Cargo.toml，rusqlite bundled 唯一新依賴）
- [x] 1.2 `db.rs`：`RegistryDb::open(path)` 建立/開啟 SQLite，`PRAGMA user_version = 1`，三表 schema（workspaces / plugin_registrations / handoff_registry），含 CHECK 約束（status 兩態）與 FK CASCADE
- [x] 1.3 schema 測試：首建建表、corrupt db 明確報錯不覆寫、user_version 版控

## 2. workspaces 表

- [x] 2.1 `upsert_workspace(workspace_key, root_path)`：不存在則插入並設 active
- [x] 2.2 `set_active_workspace(workspace_key)`：單一 active（清除其他）
- [x] 2.3 `list_workspaces()` / `get_active_workspace()`
- [x] 2.4 測試：註冊/切換 single-active/清單

## 3. plugin_registrations 表 + rehydration 戳記

- [x] 3.1 `upsert_plugin_registration(plugin_id, workspace_key, collection_name)`：首插 `last_synced_at=0, status='Unavailable'`
- [x] 3.2 `set_status(plugin_id, workspace_key, Ready|Unavailable)` + status 兩態 CHECK 測試（SyncPending 不得入庫）
- [x] 3.3 `get_pending_records_since(last_synced_at)` 語意：由 envelope `created_at > last_synced_at` 選取（介面定義，P4 接實際 query）
- [x] 3.4 `mark_synced(plugin_id, workspace_key, timestamp)`：同一交易內更新戳記
- [x] 3.5 測試：rehydration 判定點、cascade 刪除、兩態約束

## 4. handoff_registry 表 + auto-pruning

- [x] 4.1 `put_snapshot(HandoffSnapshot)`：insert，`expires_at` 預設 = `created_at + 7 days`
- [x] 4.2 `list_snapshots(workspace_key)` / `get_snapshot(snapshot_id)`
- [x] 4.3 prune 單交易：TTL 刪除（`expires_at < now`）+ FIFO（`ORDER BY created_at DESC LIMIT 20`，超出刪最舊）
- [x] 4.4 測試：7 天預設、expired 刪除、20 份上限 FIFO、prune 順序（先 TTL 後 FIFO）、寫入即觸發

## 5. 被動 ping + MemorySyncJob 介面

- [x] 5.1 `probe_provider()`：10ms timeout 健康檢查（包 `QdrantMemoryStore::is_available`，加 timeout 邊界）
- [x] 5.2 `MemorySyncJob` 執行：於 `check_and_resync` 成功 ping 後啟動。從 SQLite 撈 `created_at > last_synced_at` 的 `handoff_registry` 待同步資料，經由 Qdrant REST API batch upsert（復用現有 `upsert_rest`），成功後 mark_synced。**不實作 gRPC Incremental Sync**（P6 併入本 change，gRPC 需求已移除）。
- [x] 5.3 `check_and_resync()`：CLI/TUI 啟動邊界呼叫——ping 失敗 → Warning + 續跑；成功 → job + 洗回 Ready
- [x] 5.4 測試：ping 失敗續跑、成功轉 Ready、失敗保留資料（用 injectable probe 測）

## 6. CLI 接線 + 文件

- [x] 6.1 `graphify workspace list/switch` 子指令（供 TUI Stage 1 用）
- [x] 6.2 XDG 路徑解析（Linux `~/.local/share/graphify/graphify.db`，無 HOME 時 fallback）
- [x] 6.3 文件同步：plugin_system.md（registry 層）、core.md（DB 位置）、architecture-memory-plugin.md（roadmap P2 標完成）
- [x] 6.4 全 workspace：fmt + clippy `-D warnings` + tests 全綠；OpenSpec validate + tasks 標記完成
