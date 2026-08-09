# qdrant-local-fallback — Tasks

## 1. Config 擴充（graphify-memory）

- [ ] 1.1 `QdrantConfig` 新增 `local_fallback_enabled`、`local_bin_dir`、`local_storage_dir`、`local_version`、`local_http_port`、`local_grpc_port`（全數 `#[serde(default = ...)]`，既有 TOML 零遷移）
- [ ] 1.2 為新欄位補 serde 解析測試（default 值 + 顯式值 + 相容舊設定檔無欄位）

## 2. QdrantLocalProcess 模組（graphify-memory/src/local_process.rs）

- [ ] 2.1 新增 deps：`flate2`、`tar`（.tar.gz 解壓）
- [ ] 2.2 實作 binary 下載：GitHub Releases asset URL 組裝（平台 target 選擇，x86_64-linux-gnu 預設）+ `.tar.gz` 解壓至 `local_bin_dir`
- [ ] 2.3 實作 SHA-256 驗證：GitHub Releases API `digest` 欄位比對，不符回 Err 且不執行未驗證 binary
- [ ] 2.4 實作 spawn：`std::process::Command` + `QDRANT__` env 覆寫（HTTP/GRPC port、STORAGE_PATH、TELEMETRY_DISABLED、LOG_LEVEL）
- [ ] 2.5 實作 readiness：`/healthz` 輪詢（bounded timeout ~30s / 250ms 間隔）
- [ ] 2.6 實作 shutdown：`Drop` SIGTERM + `wait()`，無 orphan
- [ ] 2.7 單元測試：下載+驗證（digest 不符拒絕執行）、spawn+healthz、Drop 終止（可注入 fake binary / 非網路測試路徑）

## 3. 雙軌 StorageMode + init_with_fallback（graphify-memory/memory.rs）

- [ ] 3.1 新增 `StorageMode::{ServerUrl(String), LocalProcess}` 並掛到 store 狀態
- [ ] 3.2 實作 `init_with_fallback(lt_config, concurrency)`：Server `/healthz` 探測（10ms bounded）→ healthy 走 `ServerUrl`、否則 spawn Local → `LocalProcess`
- [ ] 3.3 `local_fallback_enabled=false` 時完全走現行行為（`new()` 向後相容，等價語意）
- [ ] 3.4 Local spawn 失敗 → `MemoryStatus::Unavailable` 語意，不硬崩潰
- [ ] 3.5 保留 `new()` 同步建構（內建 block_on 橋接或標註 deprecated 指引），不破壞 graphify-mcp/graphify-cli 既有呼叫
- [ ] 3.6 測試：Server healthy 選 ServerUrl / Server 不可用降級 Local / fallback disabled 維持 unavailable（fake health 注入）

## 4. Rehydration job body（graphify-cli/src/rehydrate.rs）

^- [x] 4.1 實作 `RehydrateJob`（struct 持 Local store + Server client）實作 `SyncJob::run(db, plugin_id, workspace_key)`
^- [x] 4.2 讀 `get_registration` 取 `last_synced_at` + `qdrant_collection_name`；以 `created_at > last_synced_at` scroll Local collection
^- [x] 4.3 批次 upsert 至 Server 對應 collection（`record_id` 確定性 hash 冪等）
^- [x] 4.4 成功後 Local Drain/Mark 已同步點位；失敗保留全數 pending + `last_synced_at` 不動
^- [x] 4.5 單元測試：pending 範圍正確（checkpoint 過濾）、重跑冪等（partial failure 後續補齊）、失敗保資料

## 5. 啟動邊界接線（graphify-cli）

- [ ] 5.1 `sync_to_qdrant` 改用 `init_with_fallback`
- [ ] 5.2 CLI 啟動邊界呼叫 `check_and_resync(db, probe, job, workspace_key)`（probe 包 10ms `is_available` 邊界，對齊 memory-resync spec）
- [ ] 5.3 `graphify-mcp` `MemoryQueryService` 選擇性改用 `init_with_fallback`（config 關閉時零行為變化）
- [ ] 5.4 端對端驗證：無 Server 環境 Local 模式可用 → 起 fake/真實 Server → 啟動時 rehydrate → registry 翻 Ready + StorageMode 切 ServerUrl

## 6. 文件與驗證

- [ ] 6.1 docs/architecture-memory-plugin.md / plugin_system.md 增補雙軌 + rehydration 段落（RFC-0004 §1.3 pseudocode 註記：`from_path` 不存在，以受管程序取代；docs/ref 原條文不動 #3127）
- [ ] 6.2 `cargo fmt` + `cargo clippy -D warnings`（workspace 零警告 #3154）+ 全測試通過
- [ ] 6.3 `openspec validate` 通過
