# Handoff — Phase 2 plugin-health-admission 實作

**日期**：2026-08-10
**狀態**：Phase 1 完成（commit `ce037f7`），Phase 2 文件更新完成（commit `28a7243`），Phase 2 實作尚未開始。

---

## 已完成

### Phase 1 — PluginStatus 四態 + Schema v2 migration
- `PluginStatus` enum 四態：`Healthy` / `Degraded` / `Unavailable` / `Quarantined`
- `SCHEMA_VERSION` 1 → 2，`ensure_schema` 串接 v1 → v2
- `migrate_to_v2`：重建 `plugin_registrations` 表，CHECK 新詞彙，`Ready→Healthy` 對映
- `mark_synced` SQL `'Ready'` → `'Healthy'`
- `list_registrations` 加 `ORDER BY plugin_id`
- 18 tests pass，clippy 零警告，workspace check 乾淨
- tasks.md 1.1-1.4 全勾選
- **Commit**：`ce037f7`

### Phase 2 — 文件更新（spec/design/tasks 重定位到 MCP host）
- **關鍵決策**：熔斷器實作在 `graphify-mcp` subprocess host（真實執行邊界），非 CLI `PluginHost`（生產上空的，`.register()` 只有測試呼叫）
- spec `plugin-circuit-breaker`：timeout 在 subprocess boundary（既有 recv_timeout ceiling），breaker 計 tool-call/notification 失敗，3x → Quarantined
- design D1/D2/D3 更新；D2 簡化（recv_timeout 已是硬超時，不需 thread spawn）
- D3 修正：`validate_envelope` 位置 `plugin.rs` → `plugin_memory.rs`（envelope 型別所在處）
- 500ms → 既有 `PLUGIN_TIMEOUT`（30s）；notification fire-and-forget 不會 stall
- tasks Phase 2 重寫（2.1-2.5 目標 `graphify-mcp/src/plugin_host/`）
- **Commit**：`28a7243`

---

## Phase 2 待實作（5 tasks，文件已就緒）

### 2.1 `validate_envelope` 純函式
- **位置**：`graphify-core/src/plugin_memory.rs`（`PluginMemoryEnvelope<T>` 所在處）
- **簽名**：`pub fn validate_envelope<T: Serialize>(env: &PluginMemoryEnvelope<T>) -> Result<(), String>`
- **檢查**：`workspace_key`/`plugin_id`/`record_id` 非空、`created_at > 0`、`format_version == FORMAT_VERSION`、`payload` 序列化非 Null
- **回傳**：`Err(String)` 帶欄位名 + 原因

### 2.2 `CircuitBreaker` struct
- **位置**：`graphify-mcp/src/plugin_host/`（新增 module 或放 host.rs）
- **欄位**：`failures: HashMap<String, u32>`（per-plugin_id 連續失敗計數）、`quarantined: HashSet<String>`（bypass set）
- **方法**：
  - `is_bypassed(plugin_id) -> bool`：在 quarantined set 中
  - `record_failure(plugin_id) -> bool`：計數 +1，>=3 回 true（剛跨 threshold）
  - `record_success(plugin_id)`：計數歸零
  - `seed_quarantined(plugin_id)`：host 啟動時從 registry 預載
- **常數**：`THRESHOLD = 3`

### 2.3 Wire breaker into MCP `PluginHost`
- **位置**：`graphify-mcp/src/plugin_host/host.rs`
- `PluginHost` 新增欄位：`breaker: CircuitBreaker`、`workspace_key: String`（cwd-derived）、`registry: Option<RegistryDb>`
- `scan(config)`：
  - 從 cwd derive `workspace_key`（同 main.rs:652 的邏輯，用 `graphify_core::derive_workspace_key`）
  - `RegistryDb::open(&registry_db_path())` → 失敗 None + log
  - 遍歷 config.plugins，若 registry 已標 Quarantined → `breaker.seed_quarantined(id)`
- `call_tool(tool_name, args)`：
  - unprefix 取 plugin_id
  - `if breaker.is_bypassed(plugin_id) → return Err("quarantined")`
  - 呼叫 `proc.call_tool(...)`
  - Ok → `breaker.record_success(plugin_id)`
  - Err → `on_failure(plugin_id, err)`，return Err
- `broadcast_graph_updated(payload)`：
  - 遍歷 processes，`if !state.is_ready() || breaker.is_bypassed(id) → skip`
  - `proc.send_notification(...)`，Err → `on_failure(id, err)`
- `on_failure(plugin_id, err)`：
  - `if breaker.record_failure(plugin_id)`（剛跨 3）:
    - log `[plugin:{id}] quarantined after 3 consecutive failures`
    - `if let Some(db) = &self.registry { db.set_status(id, &workspace_key, PluginStatus::Quarantined) }`

**失敗判定（計入 breaker 的）**：
- tool call timeout（recv_timeout 已 abort）
- tool call transport/framing error（process.rs await_response 已 mark Failed）
- notification delivery error（send 失敗）
- **不計**：plugin 回傳 JSON-RPC error response（plugin 正常回應，屬 business error）

### 2.4 Wire `validate_envelope` 進 rehydrate.rs
- **位置**：`graphify-cli/src/rehydrate.rs`，`SyncJob::run`（line 131-156）
- **接線點**：`pending` 取出後（line 141-148）、`push_points` 之前（line 152-154）
- **邏輯**：
  ```rust
  let valid: Vec<_> = pending.into_iter().filter(|env| {
      match validate_envelope(env) {
          Ok(()) => true,
          Err(reason) => {
              eprintln!("[plugin:{plugin_id}] dropping invalid envelope ({}): {reason}", env.record_id);
              false
          }
      }
  }).collect();
  if valid.is_empty() { return Ok(()); }
  self.rt.block_on(self.push_points(&reg.qdrant_collection_name, &valid))
  ```
- `pending` 來自 `local_jsonl_store.pending_records_since::<serde_json::Value>(...)`

### 2.5 Unit tests
- `validate_envelope`：valid envelope Ok、缺欄位 Err 帶欄位名、payload Null Err、created_at=0 Err、format_version 不符 Err
- `CircuitBreaker`：1-2 次失敗不 quarantine、3 次 → quarantined、success 重置、`is_bypassed` 行為
- `PluginHost` wiring：tool call failure 計數、3x 後 call_tool 回 Err("quarantined")、broadcast skip quarantined、quarantined 在 registry 寫入（mock registry 或 temp db）
- rehydrate：invalid envelope 被 drop + warning、valid envelope 照常 push

---

## 關鍵 API 參考（實作時查證）

| API | 位置 | 備註 |
|-----|------|------|
| `PluginMemoryEnvelope<T>` | graphify-core/src/plugin_memory.rs:20 | 泛型 payload |
| `derive_workspace_key` | graphify-core | cwd → workspace_key hash |
| `RegistryDb::open` | graphify-registry/src/db.rs | `open(path)` |
| `registry_db_path()` | graphify-registry | XDG path |
| `get_registration(plugin_id, ws_key)` | db.rs | → `Option<PluginRegistration>` |
| `set_status(plugin_id, ws_key, status)` | db.rs | 寫 status |
| `PluginStatus::Quarantined` | db.rs:33-59 | Phase 1 新增 |
| `PluginHost::scan(config)` | graphify-mcp/src/plugin_host/host.rs | 建 processes HashMap |
| `PluginHost::call_tool` | host.rs:93-102 | unprefix + route |
| `PluginHost::broadcast_graph_updated` | host.rs:106-115 | send_notification loop |
| `PluginProcess::await_response` | process.rs | recv_timeout(PLUGIN_TIMEOUT=30s) |
| `PluginState` | process.rs | Spawning/Ready{tools}/Failed(String) |
| `graphify-mcp` deps | Cargo.toml | 已依賴 graphify-registry + graphify-core |

---

## 驗證門檻
- `cargo test -p graphify-core`（validate_envelope）
- `cargo test -p graphify-mcp`（CircuitBreaker + PluginHost wiring）
- `cargo test -p graphify-cli`（rehydrate 驗證接線）
- `cargo clippy --workspace`（零警告 #2368）
- `cargo check --workspace`（跨 crate 無破壞）

---

## Phase 3-6 概覽（後續，未動）
- Phase 3：CLI `plugin health` / `plugin reset` 指令 + probe
- Phase 4：TUI workspace selector + plugin panel + [F5]
- Phase 5：E2E integration test
- Phase 6：docs/release-process.md + 版本號 `v2.0.0-beta.1`

---

## 決策紀錄
1. **熔斷器落點**：MCP subprocess host（非 CLI PluginHost）— 使用者授權，因 CLI host 生產上無 plugin 註冊
2. **500ms → 30s**：subprocess boundary 用既有 PLUGIN_TIMEOUT；notification fire-and-forget 不 stall。已_flag 待使用者確認（若要 500ms 嚴格 tool-call ceiling 需另行討論）
3. **`validate_envelope` 位置**：plugin_memory.rs（envelope 型別所在），修正 tasks 原本的 plugin.rs
4. **workspace_key**：MCP host cwd-derived（同 broadcast 既有邏輯），breaker 計數 per-plugin_id，registry 寫入用 cwd workspace_key

## 已知疑點（實作前查證）
- `RegistryDb` 是否 Send？host 持有 `Option<RegistryDb>`，main.rs 用 `Rc<RefCell<PluginHost>>`（單執行緒），應無問題
- `PluginStatus` 是否 `PartialEq`？`get_registration` 回傳的 status 比對需要（db.rs Phase 1 已加 derive）
- `graphify_core::derive_workspace_key` 確切函式名與路徑（實作時查）
- MCP host 的 `call_tool` 是否已有 unprefix 邏輯（host.rs:93-102，實作時讀完整）