# qdrant-local-fallback

## Why

RFC-0004 §1.3 定義了向量引擎的雙軌儲存（Qdrant Local / Server Dual-Track）：預設以 Local 模式開箱即用、零伺服器依賴，外部 Qdrant 伺服器可用時自動升級，斷線時秒級無縫降級。但 RFC 的 pseudocode 依賴 `qdrant_client::Qdrant::from_path()`——經調查（@librarian 兩輪研究 + 直接查證 docs.rs 0.11.1→1.19.0 全部版本），**該 API 從未存在於任何 qdrant-client 版本**，此機制無法直接實作。§1.3.1 的 Local→Server 單向 Delta Rehydration 亦因此懸空：P2（sqlite-global-registry）已建好 `last_synced_at` 戳記與 `SyncJob` trait，但 job body 明確留給 P4 實作。

本 change 以**受管 Standalone Qdrant 程序**（Option A，用戶已選定）取代不存在的 `from_path`，讓雙軌機制與 Rehydration 落地，補齊 RFC-0004 最後一個未實作的基礎設施閉環。

## What Changes

- **受管 Qdrant Local 程序（`QdrantLocalProcess`）**：首次使用 Local 模式時從 GitHub Releases 下載官方 prebuilt binary（x86_64-linux-gnu 31.8MB，sha256 digest 由 GitHub API 提供並驗證），以 `QDRANT__` 環境變數覆寫（`QDRANT__SERVICE__HTTP_PORT`、`QDRANT__STORAGE__STORAGE_PATH`、`QDRANT__TELEMETRY_DISABLED`）啟動為子程序，`/healthz` readiness 輪詢，Drop/關閉時優雅終止。
- **雙軌 StorageMode（`init_with_fallback`）**：`QdrantMemoryStore` 新增 `init_with_fallback(server_url, local_config)`——先探測外部 Server 健康；可用 → `ServerUrl` 模式，不可用 → 拉起 Local 程序連線至 `127.0.0.1:<port>`。儲存介面不變（仍為 `Qdrant::from_url` + REST），呼叫端零感知。
- **One-Way Delta Rehydration（`SyncJob` body）**：實作 P2 保留的 `SyncJob::run` 本體——讀取 Local 端 `PluginMemoryEnvelope` 點位（`created_at > last_synced_at`），批次以 `record_id`（確定性 hash）冪等 Upsert 至 Server 的 `graphify_plugin_<id>` collection，成功後更新 `last_synced_at` 並 Drain/Mark Local，最後切換 `StorageMode` 至 `ServerUrl`。一次性阻塞事件，無常駐雙寫。
- **Config 擴充**：`QdrantConfig` 新增 local fallback 欄位（`local_fallback_enabled`、`local_storage_dir`、`local_bin_dir`），serde default 保持向後相容，既有設定檔不需變更。

## Capabilities

### New Capabilities

- `qdrant-local-fallback`: 雙軌向量儲存——受管 Qdrant Local 程序生命週期（下載/驗證/啟動/health/關閉）與 `init_with_fallback` 降級語意（Server 可用自動升級、不可用降級 Local、斷線秒級切換）。
- `local-server-rehydration`: Local→Server 單向 Delta Rehydration——掃描 `created_at > last_synced_at` 的 envelope 點位、`record_id` 冪等批次 Upsert、`last_synced_at` 原子更新、Local Drain/Mark、完成後切換 `ServerUrl`。

### Modified Capabilities

- 無（P2 的 sqlite-global-registry change 尚未 archive，`memory-resync` 尚非主規格；本 change 之 `local-server-rehydration` 即為其 job body 之落地，後續 archive 時併入）。

## Impact

- **graphify-memory**（主要）：`memory.rs` 新增 `init_with_fallback` + `StorageMode`；新增 `local_process.rs`（下載/驗證/spawn/health/shutdown）與 rehydration job body。新依賴：`flate2`、`tar`（解壓 .tar.gz）。`QdrantConfig` 新增 serde-default 欄位。
- **graphify-registry**：不動（`SyncJob` trait 已存在）；P4 在 graphify-cli 組裝端提供 `SyncJob` 的實作（依賴注入，遵守 D1 無環相依）。
- **graphify-cli**：`sync_to_qdrant` 改走 `init_with_fallback`；啟動邊界（CLI 命令 / TUI 啟動）觸發 `check_and_resync`（P2 已建）。
- **graphify-mcp**：`MemoryQueryService` 建構改走 `init_with_fallback`（選擇性，Local fallback 預設可由 config 關閉）。
- **OpenSpec**：2 個新 delta spec；RFC-0004 §1.3 pseudocode 的 `from_path` 機制在 design.md 中註記為「不存在，以受管程序取代」，docs/ref 原條文不動（#3127 快照慣例）。
