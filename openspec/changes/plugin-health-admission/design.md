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
               │ feeds
┌──────────────▼──────────────────────────────┐
│ graphify-cli PluginHost (plugin_host.rs)    │
│ 500ms timeout + schema filter + breaker     │
└─────────────────────────────────────────────┘
```

## Key Decisions

### D1: 狀態機落點在 graphify-registry（SQLite），不在記憶體

Quarantine 必須跨程序存活（spec: plugin-health-status Scenario: Quarantine survives process restart）。因此失敗計數器是**程序內**的（CircuitBreaker struct），但**狀態**（Quarantined）寫入 SQLite。程序內計數器達 3 → 寫狀態 → 之後的調用直接讀 SQLite 判斷 bypass。

- 計數器：`graphify-cli` 內新 `CircuitBreaker` struct（HashMap<(plugin_id, workspace_key), u32>），`#[derive(Default)]`，無需持久化。
- 狀態：`graphify-registry::db::PluginStatus` 擴充四態 + `set_status` 既有 API（db.rs:274 已存在）。

### D2: 500ms timeout 實作 — 不引入 async runtime

`graphify-core` 禁止 async（AGENTS.md）。`graphify-cli` 內 PluginHost 目前是同步 `broadcast`。方案：**thread + channel + 等待**，每次 hook 呼叫 spawn 一個 `std::thread`，`recv_timeout(500ms)`：

```rust
let (tx, rx) = std::sync::mpsc::channel();
std::thread::spawn(move || {
    let result = catch_unwind(AssertUnwindSafe(|| plugin.on_graph_updated(event)));
    let _ = tx.send(result);
});
match rx.recv_timeout(Duration::from_millis(500)) {
    Ok(Ok(())) => /* success, reset counter */,
    Ok(Err(panic)) => /* failure: panic */,
    Err(RecvTimeoutError::Timeout) => /* failure: timeout */,
}
```

每 plugin 呼叫 spawn thread 成本約數十 µs，遠低於 500ms ceiling；event 廣播為批次操作，無即時性要求，可接受。`ponytail:` 註記：最省做法是重複用一個 worker thread pool，但 plugin 數少（<10）、呼叫頻率低，每次 spawn 更簡單且無 shared state。

> [待討論] 若未來 plugin 數量大增，可改 thread pool。目前 YAGNI。

### D3: Schema Filter 位置 — plugin 結果寫入前的唯一閘口

envelope 驗證放在 `graphify-cli/src/rehydrate.rs`（PluginDomainMemory 寫入 Qdrant 的既有路徑）與 plugin_host 的結果接收點。驗證器是純函式 `validate_envelope(&PluginMemoryEnvelope) -> Result<(), String>`（檢查 record_id/workspace_key/plugin_id/payload/created_at 非空且型別正確），放 `graphify-core::plugin`（envelope 型別所在處），兩端共用。

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
| graphify-registry/src/db.rs | PluginStatus 四態、migrate_to_v2、status API 擴充 |
| graphify-core/src/plugin.rs | validate_envelope 純函式 |
| graphify-cli/src/plugin_host.rs | 500ms timeout + CircuitBreaker + bypass |
| graphify-cli/src/main.rs | `plugin health` / `plugin reset` 指令、tui 啟動改 selector |
| graphify-cli/src/tui.rs | startup selector + plugin panel + [F5] |
| graphify-cli/src/workspace.rs | workspace list 讀取（既有）+ probe/reset 命令 |
| graphify-cli/src/rehydrate.rs | schema 驗證接入寫入路徑 |
| tests/e2e_health_admission.rs | 新增 E2E |

## Risks

- **thread spawn 開銷**：每次 hook 呼叫 spawn thread。影響：僅在 index/extract 完成時 broadcast，頻率低，無感。
- **CHECK 約束遷移**：SQLite 重建表有風險，需保留舊資料（copy 後驗證 row count）。
- **Degraded 語意**：bool 探針無法完整表達三態，見 D6 [待討論]。
