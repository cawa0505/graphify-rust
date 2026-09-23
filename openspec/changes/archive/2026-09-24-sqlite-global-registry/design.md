# sqlite-global-registry — Design

## Context

See proposal.md — Why. Registry 是 P3（TUI workspace switcher）與 P4（Qdrant Local fallback rehydration）的共同前置。現況：`PluginMemoryEnvelope`（graphify-core，P1 完成）提供 data contract；`PluginDomainMemory`（graphify-memory，Phase 3）提供 per-plugin JSONL namespace；`QdrantMemoryStore`（graphify-memory）持有 provider 狀態（`MemoryStatus::Unavailable` 已實作）。無任何 workspace/plugin 跨 session 註冊層。

Constraints: 零背景 daemon（#3097 pull-based）；workspace_key 為唯一路由鍵（#3086）；`last_synced_at` 與 `handoff_registry` 欄位必須與 P1 的 `HandoffSnapshot` struct 對齊；status 兩態（Ready/Unavailable）。

## Goals / Non-Goals

**Goals:**
- 單一 `graphify.db` 三表 schema，首次使用自動建立
- registry 成為 rehydration 判定點（`created_at > last_synced_at`）
- 被動觸發 sync：CLI/TUI 啟動邊界 10ms ping，無 daemon
- handoff 快照自維護（7 天 TTL + 20 份 FIFO，寫入時觸發）

**Non-Goals:**
- 不做 Local→Server 的實際資料遷移（那是 P4，本 change 只提供判定點與戳記更新）
- 不做 Cross-workspace 檢索（spec 已定嚴禁）
- 不做 embedding provider 的持續監控（被動模式）
- 不碰 Qdrant 集合管理（plugin-domain-memory 已定義）

## Decisions

### D1: 新 crate `graphify-registry`（不是 graphify-core 模組）
- **決策**：獨立 crate，依賴 `graphify-core`（types）與 `graphify-memory`（provider 狀態）。
- **理由**：graphify-core 有零非核心依賴鐵律（#3088/#3167——core 不能引進 rusqlite）；registry 是 infra 層，與 memory/llm 平行。TUI（graphify-cli）與 MCP 都需讀 registry，放 core 會污染核心。
- **替代**：graphify-core 模組——被 #3088 否決（rusqlite 是外部依賴）。

### D2: rusqlite（bundled）為唯一新增依賴
- **決策**：`rusqlite` with `bundled` feature（SQLite 編譯進 binary，無系統 lib 依賴）。
- **理由**：graphify 是 single-binary 分發（#2402），bundled 確保無 runtime 依賴；無 async runtime 需要（registry 是同步、低頻操作，符合 #2480 之外的主路徑）。
- **替代**：sqlx（async + 過重）；rusqlite bundled 最輕且確定性高。
- **規格常數綁定**：TTL 7 天 / 20 份 / 10ms ping / status 兩態 / `last_synced_at` 語意 → 全部進測試（AGENTS.md spec-to-test binding）。

### D3: DDL 與 P1 struct 對齊
- `handoff_registry` 欄位直接對應 `HandoffSnapshot`：`snapshot_id`(PK) / `session_id` / `workspace_key`(FK→workspaces) / `created_at` / `expires_at` / `payload`(JSONB TEXT)。
- `plugin_registrations`：(plugin_id, workspace_key) 複合 PK，`qdrant_collection_name` 由 `plugin_collection_name()` 派生（graphify-memory），`last_synced_at` INTEGER 預設 0，`status` TEXT CHECK IN ('Ready','Unavailable')。
- `workspaces`：`workspace_key`(PK) / `root_path` / `is_active`(INTEGER 0/1) / `last_indexed_at`。
- **FIFO 語意**：`created_at` 為排序鍵（非插入序），20 份上限用 `ORDER BY created_at DESC LIMIT 20` 保留、其餘刪除。

### D4: 被動 ping 與 MemorySyncJob 的邊界
- ping 10ms 只測 provider 健康（`QdrantMemoryStore::is_available` 已存在，包 10ms timeout），不做完整 query。
- MemorySyncJob 的實際資料遷移邏輯屬 P4（rehydration）；本 change 定義 job 的觸發介面（trait）與狀態轉移（Unavailable→Ready），job 本體留 P4 實作或 stub 明確回報未實作（#honesty，不得 mock）。

### D5: 寫入時 pruning 單一交易
- 新 snapshot insert + TTL scan + capacity eviction 同一 SQLite transaction（原子）。
- prune 順序：先 TTL（`expires_at < now`）再 FIFO（超過 20）。

## Risks / Trade-offs

- [rusqlite bundled 增加 binary 體積 ~1MB] → 可接受；換來無 runtime lib 依賴與 single-binary 保證
- [registry 與 Qdrant 資料可能不一致（registry 刪但 storage 有 orphan）] → 本 change 只管理 registry 生命週期；storage 層清理列 P4/plugin 範圍，spec 明示 registry 為 SSoT
- [10ms ping 在慢網路誤判 Unavailable] → ping 是「恢復偵測」非「故障偵測」；誤判成本 = 多一次 startup 檢查，無資料風險
- [FIFO 用 created_at 可能同秒碰撞] → 20 份上限場景下同秒碰撞機率極低；若發生以 rowid 作 tiebreaker

## Migration Plan

- 全新功能，無既有資料遷移。`graphify.db` 首次使用自動建立。
- 若未來 schema 演進：PRAGMA user_version 版控（本 change 建 v1）。

## Open Questions

無——規格已鎖定（TTL/上限/兩態/路徑），實作細節可在 tasks 層決定。
