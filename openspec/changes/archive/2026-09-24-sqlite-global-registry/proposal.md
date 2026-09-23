# sqlite-global-registry

## Why

RFC-0004 §1.1 定義的 SQLite Global Registry（`graphify.db`）是 TUI workspace switcher（P3）與 Qdrant Local fallback rehydration（P4）的共同前置依賴，但目前零實作。三個 plugin 的 `PluginMemoryEnvelope` 資料散落各 storage，缺少統一的 workspace / plugin 註冊追蹤，導致 rehydration 判定點（`last_synced_at`）與 handoff 快照生命週期（TTL / 上限）無從實現。

## What Changes

- 新增 `sqlite-registry` crate（或 graphify-core 內模組）：`graphify.db` 三表 schema（`workspaces` / `plugin_registrations` / `handoff_registry`），置於 XDG 資料目錄。
- `plugin_registrations` 含 `last_synced_at` 戳記（Local→Server Rehydration 判定點，RFC 1.3.1）與 `status`（Ready / Unavailable，非 SyncPending——SyncPending 由 `last_synced_at` 落後推導，避免狀態漂移）。
- `handoff_registry` 存取 `HandoffSnapshot`（P1 已定義的雙層 struct）。
- 被動觸發式同步：不做背景 daemon polling；下次 CLI 指令或 TUI 啟動時對 Embedding Provider 發 10ms ping，不可用→保持 Unavailable 印 Warning 續跑拓撲；恢復健康→觸發 MemorySyncJob 並洗回 Ready。
- Handoff auto-pruning：TTL 7 天 + 每 workspace 上限 20 份（FIFO），每次寫新快照時觸發清理。
- Cross-workspace 檢索預設嚴格禁止（堅守 #3086 workspace_key 隔離邊界；例外機制未來才開放）。

## Capabilities

### New Capabilities
- `sqlite-registry`: `graphify.db` 三表 schema 與存取 API（workspaces / plugin_registrations / handoff_registry），含 `last_synced_at` rehydration 戳記與 Ready/Unavailable 兩態。
- `memory-resync`: 被動觸發式同步——無常駐 daemon，CLI/TUI 啟動時 10ms ping provider，恢復則執行 MemorySyncJob。
- `handoff-pruning`: 快照自動清理——TTL 7 天 + 每 workspace 20 份 FIFO 上限，寫入時觸發。

### Modified Capabilities
- 無（`plugin-domain-memory` 不變，僅新增 registry 層）

## Impact

- 新 crate `graphify-registry`（或 graphify-core 模組）：依賴 rusqlite（唯一新增依賴）、`graphify_core`（HandoffSnapshot / workspace_key）、`graphify_memory`（provider 狀態查詢）。
- 影響 crates：graphify-cli（TUI workspace switcher 讀 registry）、graphify-mcp（provider ping 於啟動時）、graphify-memory（rehydration 判定讀 `last_synced_at`）。
- XDG 資料路徑：`~/.local/share/graphify/graphify.db`（Linux），遵循 #2503 config 同族慣例。
- CLI 新子指令：`graphify workspace list/switch`（供 TUI Stage 1 使用）。
