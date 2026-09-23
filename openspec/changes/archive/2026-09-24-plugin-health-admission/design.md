# Design: Plugin Health & Admission Protocol

## Overview

三個 spec 對應 roadmap Beta-M2/M3/M4，實作上分三層：

```
┌─ TUI (tui.rs) ──────────────────────────────┐
│ workspace selector + plugin panel + [F5]    │
└──────────────┬──────────────────────────────┘
               │ reads/writes
┌──────────────▼──────────────────────────────┐
│ graphify-registry (SQLite)                  │
│ PluginStatus 四態 + quarantine + probe 結果  │
└──────────────┬──────────────────────────────┘
               │ feeds (quarantine read/write)
┌──────────────▼──────────────────────────────┐
│ graphify-mcp PluginHost (plugin_host/)      │
│ subprocess host: hard timeout + breaker     │
└─────────────────────────────────────────────┘
```

> **定位修正（2026-08-10）**：熔斷器/超時實作在 `graphify-mcp` 的 subprocess host（真實執行邊界），不在 CLI 的 in-process `PluginHost` — 後者在生產路徑從未註冊 plugin（`.register()` 僅測試使用）。CLI `graphify-cli/src/plugin_host.rs` 的 `catch_unwind` 保留不變。

## Key Decisions

### D1: 狀態機落點在 graphify-registry（SQLite），不在記憶體

Quarantine 必須跨程序存活（spec: plugin-health-status Scenario: Quarantine survives process restart）。因此失敗計數器是**程序內**的（CircuitBreaker struct），但**狀態**（Quarantined）寫入 SQLite。程序內計數器達 3 → 寫狀態 → 之後的調用直接讀 SQLite 判斷 bypass。

- 計數器：`graphify-mcp` 內新 `CircuitBreaker` struct（HashMap<(plugin_id, workspace_key), u32>），`#[derive(Default)]`，無需持久化。
- 狀態：`graphify-registry::db::PluginStatus` 四態（Phase 1 已實作）+ `set_status` 既有 API（db.rs:320 已存在）。
- Host 啟動時：讀 registry，`Quarantined` 的 plugin 直接 bypass（spec: Quarantined plugin is bypassed on startup）。

### D2: 硬超時實作 — 既有 recv_timeout，不新增執行緒

`graphify-mcp` 的 `PluginProcess::await_response`（process.rs:220）已有 `recv_timeout` 硬超時機制（`PLUGIN_TIMEOUT`，host.rs:14，非零且不可設定為零）。熔斷器在此邊界接入：

- tool call：`PluginHost::call_tool` 失敗（timeout / transport / dead process）→ `breaker.record_failure(plugin_id, workspace_key)`；成功 → `record_success`。
- notification：`broadcast_graph_updated` 的 send 失敗（broken pipe）→ `record_failure`。notification 本身 fire-and-forget（無 response 等待），慢 plugin 不會卡住 broadcast — 這比 CLI 同步模型結構上更強，500ms ceiling 已由 subprocess 隔離取代。
- 達 3 次連續失敗 → `set_status(plugin_id, workspace_key, Quarantined)` + 記憶體 bypass set。
- `ponytail:` 註記：不引入 thread pool / async runtime — subprocess 本身就是隔離單位，`recv_timeout` 已提供超時。

### D3: Schema Filter 位置 — plugin 結果寫入前的唯一閘口

envelope 驗證放在 `graphify-cli/src/rehydrate.rs`（PluginDomainMemory 寫入 Qdrant 的既有路徑，RehydrateJob::push_points / envelope_to_point）。驗證器是純函式 `validate_envelope(&PluginMemoryEnvelope) -> Result<(), String>`（檢查 record_id/workspace_key/plugin_id/payload/created_at 非空且型別正確），放 `graphify-core::plugin_memory`（envelope 型別所在處，非 plugin.rs），兩端共用。

### D4: TUI 保持簡潔 — workspace selector 不進主迴圈

依使用者明確要求（「不希望 TUI 變複雜難用」）與 spec: tui-workspace-monitor Scenario: Default view unchanged：

- `graphify tui` 啟動流程改為：**先顯示 selector 全螢幕**（若 registry 非空）→ 選定後 `run_tui(graph)` 進入既有 inspector 主迴圈 — selector 是 startup 一次性畫面，不是常駐模式。
- Plugin 面板：`p` 鍵開啟覆蓋層（類似既有 modal 機制，tui.rs:136-253 已有 modal 基礎可重用），Esc 關閉，`[F5]` 在面板內觸發 reset。預設不顯示。
- 既有 inspector 的鍵盤/滑鼠/搜尋/$EDITOR 流程完全不動。

### D5: SQLite migration v2

`graphify-registry/src/db.rs` 既有 `migrate_to_v1`（db.rs:110）+ `SCHEMA_VERSION`。新增 `migrate_to_v2`：`ALTER TABLE plugin_registrations` — SQLite 無法直接改 CHECK 約束，做法：重建表（rename + create + copy + drop），`Ready`→`Healthy` 對映。更新 `SCHEMA_VERSION = 2`。

### D6: 被動探針 — 觸發式，不常駐

spec §2.2 拒絕 background polling。探針 = CLI 指令（如 `graphify plugin health`）與 TUI 啟動/[F5] 時對每個 plugin 呼叫 `on_health_check()`（trait 既有方法，plugin-api spec 已定義）。回報三態：

- 回傳 `true` → Healthy（若該 plugin 有註冊但 probe 未實作 → 以 `Unavailable` 看待，防假陽性）

> [待討論] on_health_check() 目前回傳 bool，三態的 Degraded 判斷（部分資源缺失）在 v1 探針是否需 plugin 主動回報額外資訊？目前 bool false = Unavailable/Degraded 二合一，細節待與使用者確認。

### D7: E2E Integration Test

`tests/e2e_health_admission.rs`（integration test）：
1. 註冊一個「好」plugin（RecordingPlugin 變體）與一個「壞」plugin（每次 timeout/panic）。
2. broadcast 3 次 → 壞 plugin 狀態變 Quarantined（讀 SQLite 驗證）。
3. 第 4 次 broadcast → 壞 plugin 被 bypass，好 plugin 仍收到事件。
4. 執行 `graphify extract` → `.toon` 正常導出（Core 不受影響）。
5. `[F5]` 等價 CLI reset 後重新 probe。

## Affected Files

| 檔案 | 變更 |
|------|------|
| graphify-registry/src/db.rs | PluginStatus 四態、migrate_to_v2（Phase 1 已完成 ✅） |
| graphify-core/src/plugin_memory.rs | validate_envelope 純函式 |
| graphify-mcp/src/plugin_host/host.rs | CircuitBreaker + call_tool/broadcast 失敗計數 + quarantine bypass |
| graphify-mcp/src/plugin_host/process.rs | （既有 recv_timeout 即硬超時，無改動） |
| graphify-mcp/src/main.rs | host 建構注入 registry + workspace_key |
| graphify-cli/src/main.rs | `plugin health` / `plugin reset` 指令、tui 啟動改 selector |
| graphify-cli/src/tui.rs | startup selector + plugin panel + [F5] |
| graphify-cli/src/workspace.rs | workspace list 讀取（既有）+ probe/reset 命令 |
| graphify-cli/src/rehydrate.rs | schema 驗證接入寫入路徑 |
| tests/e2e_health_admission.rs | 新增 E2E |

## Risks

- **計數器僅程序內**：MCP host 重啟後計數歸零（但 Quarantined 狀態跨程序存活，host 啟動時讀 registry 直接 bypass）。
- **CHECK 約束遷移**：SQLite 重建表有風險，需保留舊資料（copy 後驗證 row count）— Phase 1 已驗證。
- **Degraded 語意**：bool 探針無法完整表達三態，見 D6 [待討論]。
