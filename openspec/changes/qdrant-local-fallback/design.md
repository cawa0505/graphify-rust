# qdrant-local-fallback — Design

## Context

See proposal.md — Why. 現況錨點：

- RFC-0004 §1.3 的 `Qdrant::from_path` 不存在於任何 qdrant-client 版本（0.11.1–1.19.0，@librarian 兩輪研究 + docs.rs 逐版直接查證）；用戶已選定 Option A：**受管 Standalone Qdrant 程序**。
- P2 已交付 `graphify-registry`：`SyncJob` trait（resync.rs:23）、`check_and_resync`（resync.rs:58）、`plugin_registrations.last_synced_at` 戳記、雙態 `PluginStatus`；job body 明言留 P4。
- `QdrantMemoryStore::new(config, concurrency)` 為同步建構（memory.rs:163）；`MemorySearcher` trait 提供 `is_available` hook；現行 REST 路徑（upsert/search/delete）齊備。
- 無 daemon 政策（#3097 / memory-resync spec）：health 只在 CLI 命令 / TUI 啟動邊界探測。

## Goals / Non-Goals

**Goals**：
- 以受管子程序提供零伺服器 Local 模式，Server 可用時自動升級（`init_with_fallback`）。
- 落地 P2 保留的 `SyncJob::run` body（Local→Server rehydration），一次阻塞事件。
- 呼叫端零感知：兩種模式走同一 `Qdrant::from_url` + REST 介面。

**Non-Goals**：
- 不常駐雙寫；rehydration 完成即切 `ServerUrl`，無背景同步。
- 不做 Qdrant Local 內嵌（`from_path`）——該 API 不存在，此為既定決策（RFC pseudocode 保留於 docs/ref，僅在 design 註記取代機制，遵守 #3127）。
- 不做跨 workspace 聯合檢索、不做 rehydration 的 TUI 控制台（屬後續 roadmap）。
- 不在本 change 解耦 `graphify-memory` 對 reqwest/tokio 的既有依賴。

## Decisions

### D1: `QdrantLocalProcess` — 下載/驗證/spawn/health/shutdown 封裝於 graphify-memory

新模組 `graphify-memory/src/local_process.rs`，結構：

```
QdrantLocalProcess {
    child: std::process::Child,
    http_port: u16,
    grpc_port: u16,
    storage_dir: PathBuf,
}
```

- **下載**：首次使用從 `https://github.com/qdrant/qdrant/releases/download/v<ver>/qdrant-<target>.tar.gz` 抓取（x86_64-unknown-linux-gnu 為預設；musl/aarch64 依平台）。`.tar.gz` 以 `flate2` + `tar` 解壓（新 dev/prod deps，僅此兩個）。
- **驗證**：GitHub Releases API 每 asset 附 sha256 `digest`（實測 `e4405091...`）；下載後比對，不符即 Err 且不執行未驗證 binary。**注意：release asset 無 `checksums.txt`**（@librarian 初報不實，已以 API 實測更正）——digest 來源即 API。
- **spawn**：`std::process::Command` + env 覆寫而非 config 檔：`QDRANT__SERVICE__HTTP_PORT`、`QDRANT__SERVICE__GRPC_PORT`、`QDRANT__STORAGE__STORAGE_PATH`（XDG data dir，`~/.local/share/graphify/qdrant-storage`）、`QDRANT__TELEMETRY_DISABLED=true`、`QDRANT__LOG_LEVEL=error`。此為官方最高優先覆寫機制（docs 查證，非 `--data-dir`/`--disable-telemetry` flags——那類 flags 不存在）。
- **health**：TCP connect 探測（`TcpStream::connect(127.0.0.1:<http_port>)`，bounded timeout ~30s 內循環 250ms）。與 `/healthz` 對 readiness 等價（進程 listen 前不接受連接），但測試可純 std 完成、省 HTTP 語義依賴。
- **shutdown**：`Drop` 發 SIGTERM（`libc::kill`，graceful flush）並 `wait()`；CLI 退出路徑亦顯式終止，避免 orphan。
- **port 選擇**：預設 HTTP 6333/gRPC 6334 與外部 Server 衝突——Local 模式採**偏移端口**（6333+1000=16333/16334 或動態空閒端口）；偏移值進 config（serde default）。

**替選**：嵌入 `qdrant-client` local feature（不存在，否決）；釋出時打包 binary 進 graphify artifact（32MB 膨脹，用戶重視 image bloat，否決——download-on-first-run）；直接 spawn 用戶既有 qdrant binary（破壞零伺服器開箱即用，否決）。

### D2: `StorageMode` + `init_with_fallback`

`QdrantMemoryStore` 新增欄位：

```
enum StorageMode { ServerUrl(String), LocalProcess }
```

`pub async fn init_with_fallback(lt_config: LongTermMemoryConfig, concurrency: Option<usize>) -> Result<Self>`：
1. 若 `local_fallback_enabled` 為 false → 現行行為（無 fallback，unavailable 語意）。
2. 先探測 `lt_config.qdrant.url` 的 `/healthz`（bounded 10ms ping，對齊 memory-resync spec）。
3. healthy → `ServerUrl`；否則 spawn `QdrantLocalProcess`，store 以 `Qdrant::from_url("http://127.0.0.1:<port>")` 建 client，`mode=LocalProcess`。
4. Local 亦失敗 → 回 `MemoryStatus::Unavailable` 語意（現有路徑），不硬崩潰。

**同步/非同步接縫**：`QdrantMemoryStore::new` 為同步（memory.rs:163），`init_with_fallback` 為 async（需 health probe + spawn 檢查）。呼叫端已有 runtime：graphify-cli `sync_to_qdrant` 內建 tokio（#2480 模式）、graphify-mcp `MemoryQueryService` 持 current-thread runtime。兩處各自 `block_on`；`QdrantLocalProcess` 本身的生命週期管理（spawn/wait）純同步，僅 health probe 走 async。保留 `new()` 向後相容（內部等價於 `local_fallback_enabled=false`）。

**替選**：建構子改 async（破壞全部呼叫端，否決）；spawn 延遲到首次 query（複雜化 state，否決）。

### D3: rehydration job body 落於 graphify-cli 組裝層

P2 D1 已鎖定 `graphify-registry` 不得依賴 `graphify-memory`（無環相依，注入探針）。因此 `SyncJob` 的具體實作放 **graphify-cli**（同時依賴 registry + memory + llm）：

- 新模組 `graphify-cli/src/rehydrate.rs`：`struct RehydrateJob { store: Arc<QdrantMemoryStore>, server_client: Qdrant }`。
- `run(db, plugin_id, workspace_key)`：
  1. `db.get_registration(plugin_id, workspace_key)` 取 `last_synced_at` + `qdrant_collection_name`。
  2. 以 `created_at > last_synced_at` 過濾條件 scroll Local collection（REST scroll/search with payload filter）。
  3. 批次 `upsert` 至 Server 對應 collection（`record_id` 為確定性 hash → 天然冪等）。
  4. 成功後 Local 側刪除/標記已同步點位（Drain/Mark）；registry 更新由 `check_and_resync` 呼叫 `mark_synced` 完成（原子單一 transaction，P2 已建）。
- 觸發：CLI 啟動邊界呼叫 `check_and_resync(db, probe, job, workspace_key)`——probe 包 `is_available`（10ms 邊界）。

**替選**：body 放 graphify-memory（與 registry 無環相依衝突，否決）；放 graphify-mcp（MCP 非 rehydration 的自然宿主，否決）。

### D4: Config 擴充（serde default，向後相容）

`QdrantConfig`（graphify-memory/config.rs:19）新增：

```
local_fallback_enabled: bool      // default false → 現行行為不變
local_bin_dir: PathBuf            // default ~/.cache/graphify/qdrant
local_storage_dir: PathBuf        // default ~/.local/share/graphify/qdrant-storage
local_version: String             // default "v1.19.0"（GitHub release 對齊）
local_http_port: u16              // default 16333（偏移避免衝突）
local_grpc_port: u16              // default 16334
```

全數 `#[serde(default = "...")]`——既有 TOML 零遷移（#2385/#2396 相容政策）。

### D5: `local_storage_dir` 與 registry 的關係

Local 模式僅在「記憶體層」獨立運作；`graphify.db`（registry）仍是 routing/rehydration authority（SSoT）。storage 點位的 drain/mark 由 rehydration job 於成功後執行——**registry 為 SSoT，storage 清理隨之**（對齊 P2 D5 既有決策）。

## Risks / Trade-offs

- [首次 Local 使用需網路下載 31.8MB binary] → 僅一次；之後離線可用；digest 驗證防篡改；下載失敗回 `Unavailable` 語意不硬崩潰。
- [spawn 的 qdrant 程序與外部 6333/6334 端口衝突] → 偏移端口（16333/16334）+ config 可調。
- [orphan 程序] → `Drop` SIGTERM + CLI 退出顯式終止；healthz 探測失敗視為不健康回收。
- [rehydration 中斷導致重複 push] → `record_id` 確定性 hash 冪等覆寫，重跑安全（spec 已綁定）。
- [download-on-first-run 與「開箱即用」張力] → 預設 `local_fallback_enabled=false`（現行行為零變更）；用戶啟用才觸發，符合漸進採用。
- [GitHub API digest 依賴外部服務] → digest 僅用於下載瞬間驗證；binary 落地後即離線；可後續改為 pinned 值。

## Migration Plan

1. 先加 config 欄位 + `QdrantLocalProcess` 模組（可獨立測試下載/驗證/spawn/health/shutdown）。
2. `init_with_fallback` 接入 `QdrantMemoryStore`，graphify-cli `sync_to_qdrant` 改用。
3. rehydration job body（graphify-cli/rehydrate.rs）+ CLI 啟動邊界 `check_and_resync` 接線。
4. graphify-mcp `MemoryQueryService` 選擇性改用（config 關閉時零行為變化）。
5. 回滾：`local_fallback_enabled=false` 即完全回到現行單 Server 行為；無 schema/資料遷移。

## Open Questions

- `local_version` 的升級策略：新版本 binary 何時/如何自動更新？（可安全延後——固定 pinned 版本已滿足 v1；升級機制屬 roadmap 級別，不改 spec/task 拆分。）
