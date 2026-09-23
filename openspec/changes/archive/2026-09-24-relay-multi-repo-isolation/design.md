# Design: relay-multi-repo-isolation

## Context

`graphify-plugin-handoff` 的 relay 工具（`relay_init/save/switch/resume/close/status`）以單一狀態檔（`<root>/relay.json` + `.relay/relay.toon` 鏡像）管理跨 session handoff。實際使用中 relay root 常綁到 `$HOME`，多個 repo 的紀錄共用同一檔案；此時三個設計缺陷疊加，導致跨 repo 內容污染（詳 proposal.md）。

修改集中在 `GraphifyPlugins/graphify-plugin-handoff/src/`，MCP 與 CLI 兩端共用同一 plugin crate，行為修正一處生效。

## Goals / Non-Goals

**Goals**
- per-repo `project_context`，渲染不再跨 repo 污染
- `relay_save` 的 baton 語意明確且輸出誠實
- repo 路徑在寫入時驗證，git 狀態取得失敗時輸出可診斷
- relay root 綁定不再隱式落到 `$HOME` 共用檔

**Non-Goals**
- 不重寫 relay 狀態檔格式（維持 relay.json + toon 鏡像，僅加欄位）
- 不做多 relay root 同時活躍的管理 UI
- 不改 handoff snapshot 的 TTL/容量修剪（`handoff-pruning` capability 範圍）
- 不做破壞性 schema 遷移

## Decisions

### D1. `project_context` 下放到 `RepoState`（含唯讀 fallback）

`project_context` 從 `RelayState` 全域欄位改為 `RepoState.project_context: Option<String>`。

- `relay_init` 改為寫入目標 repo 紀錄的欄位（repo 未註冊時先 upsert repo 紀錄）。
- `vars_for` 渲染時取 `repo.project_context`；為 `None` 時 fallback 讀舊全域欄位（**唯讀**，save 永不寫全域欄位），再為空才用 "(unset)"。
- 舊 relay.json 不需遷移：serde 舊檔載入後全域值仍在，fallback 自然生效；新 save 會逐步把 context 落到各 repo。

**為什麼不用「渲染時以 baton repo 的 context 為準」**：baton 本身有 D2 的語意問題，用另一個全域單值去修另一個全域單值的污染，只是把錯位從 context 轉移到 baton。per-repo 是唯一能讓「N repo 共用一檔」正確的形狀。

### D2. `relay_save` 切換 baton（save 即工作於此 repo）

現行 first-save-wins（relay.rs:488-490）的問題：save 成功但 baton 不動，輸出卻印 "Active baton: <舊值>"，呼叫方（AI agent）無法區分「這是既有 baton 的回報」還是「已切換」；之後無參數的 `relay_resume` 靜默 resume 錯 repo。

改為：`relay_save` 無條件 `state.active_baton = repo_name`，輸出改為明確兩行：

```
Saved state for "NexusHub" (active baton switched to "NexusHub").
```

**保守替代方案（已考慮，不採）**：save 不切 baton，但輸出改為 `Saved state for "NexusHub". Active baton remains "ArgusOrchestrator" (use relay_switch to change).`——誠實但把切換負擔留給每次呼叫方，多一次 round-trip 且 agent 常忘。save 的使用語意（「我在這個 repo 存了進度」）本來就隱含「我現在在這個 repo」，切換符合最小驚訝。需要「存別的 repo 但不跟過去」的場景，明確走 `relay_switch` 保留。

**決策（2026-09-24，用戶確認）**：採 save 即切換；GraphifyRust 側無依賴 first-save-wins 的工作流。

### D3. repo 路徑解析與驗證（fail-loud）

`relay_save` 寫入前執行路徑解析：

1. 若 `repo` 參數是絕對路徑且存在 → 直接採用為 `RepoState.path`。
2. 否則依序嘗試 `root.join(repo)`、MCP server cwd 相對解析（`std::env::current_dir()` + repo 名）；取第一個存在的候選（**2026-09-24 修訂**：原要求「存在且為 git repo」，但 monorepo 子目錄（如 GraphifyPlugins 根下各 plugin 目錄）只是父 repo 的普通目錄、非獨立 git repo，一律要求 git 會誤殺合法案例 → 驗收改為「候選存在為目錄即可」；git-ness 只影響渲染診斷）。
3. 全部失敗 → **拒絕寫入**，錯誤訊息列出所有嘗試路徑，指引先確認 repo 位置或改用絕對路徑。不做靜默 upsert。

`RepoState.path` 一律存**絕對路徑**（`RepoState::for_repo` 的 `path: name.clone()` 預設值移除）。`vars_for` 的 git 計算改用解析後的絕對路徑；git 指令失敗時渲染 `"(git status unavailable: <path>)"` 而非 `"(not a git repo)"`，保留診斷線索。

**為什麼不只在渲染時驗證**：渲染時才發現路徑錯，污染已經寫進狀態檔（錯誤的 path、錯誤的 handoff 歸屬）。寫入時驗證讓壞資料進不了 relay.json。

### D4. relay root 綁定：workspace 主權模型（git root）

relay root **不做任何向上（walk-up）搜尋**。workspace root 定義：cwd 位於 git repo 內時為 `git rev-parse --show-toplevel`；非 git 目錄時為 cwd 本身。relay 狀態檔只認 workspace root 這一個位置：

- 綁定 = workspace root 存在 relay.json → 載入；否則 relay 工具回明確錯誤，指引 `relay_init` 或 `GRAPHIFY_RELAY_ROOT`。
- `relay_init` 建立於 workspace root（含 `.relay/`）。非 git 的 `$HOME` 本身 init 仍允許但輸出警告（該檔僅會被 cwd=$HOME 的 session 使用）。
- env override `GRAPHIFY_RELAY_ROOT`：設定時跳過 workspace root 解析，直接綁定該路徑。給刻意共用 root 的場景（例如跨 repo 的單一 baton 工作流）一個明確出口，而非隱式行為。
- `root.rs` 的 walk-up 搜尋邏輯整段移除，`$HOME` 排除特例隨之消失。

**為什麼完全不向上搜尋**：walk-up 讓「上層任何 relay.json」決定你的 root——污染是結構性的（root.rs:20-22 記錄的 "$HOME stray relay shared by 20 projects" 即實證）。workspace 主權模型下 root 由 git 邊界唯一決定，無法被上層檔案劫持。已受害的 `/home/zeng/relay.json` 不由程式自動刪除——屬使用者資料，由人決定（本變更附手動清理指引）。（2026-09-24 修訂：原「$HOME 排除 + walk-up」升級為本模型，用戶裁示「以 workspace 為主，向上存都視為錯誤」。）

### D5. `state_snapshot`（open_threads/blockers）per-repo 化

`RelayState.state_snapshot`（state.rs:37）是跨 repo 全域單值：open_threads 與 blockers 在多 repo 共用一檔時互相污染，與 D1 的 `project_context` 同類病（本次 issue 的殘留污染通道之一，2026-09-24 併入）。

改為 per-repo：`RepoState` 新增自有 `state_snapshot`（open_threads/blockers），`relay_save`/`relay_close` 寫入目標 repo 自己的 snapshot；渲染（resume/status）只讀被渲染 repo 的 snapshot。舊全域 `state_snapshot` 保留為唯讀 fallback（serde default，repo 無自有值時繼承），與 D1 的 fallback 策略一致，不做破壞性遷移。

### D6. close 快照 workspace_key 改用 repo 實際路徑

`persist_close_snapshot`（relay.rs:570）以 bind cwd derive 的 workspace_key 寫入 graphify.db：在 workspace A 關閉「B 的 repo」時，快照掛在 A 的 key 下，rehydration 歸屬錯位（殘留污染通道之二，2026-09-24 併入）。

D3 落地後 `RepoState.path` 已是驗證過的絕對路徑，close 快照的 workspace_key 改由 `RepoState.path` 的 canonical path derive（`derive_workspace_key`），使快照歸屬與 repo 真實 workspace 一致。DB 層（graphify-registry）查詢已按 workspace_key 隔離，無需變更。

### D7. 測試策略

- 單元測試（plugin crate 內）：D1 fallback 鏈（repo 有值 / repo 無值全域有值 / 皆無）、D2 baton 切換與輸出格式、D3 三層解析與拒絕路徑、D4 workspace root 解析（git repo 內 subdir 不向上搜尋、非 git cwd、env override）、D5 snapshot per-repo 隔離與 fallback、D6 workspace_key 由 repo path derive。
- 整合 fixture：以受害檔 `/home/zeng/relay.json` 的結構（脫敏後）作為舊格式載入測試——全域 context + 兩 repo + bare-name path，驗證 fallback 與路徑修復。
- MCP 端到端：在臨時 dir 結構模擬 `~/proj-a`、`~/proj-b`，驗證 save A → save B → resume B 的 context 與 baton 正確性。

## Risks / Trade-offs

- **D2 行為變更**可能影響依賴 first-save-wins 的既有流程 → 輸出訊息明確回報切換，`relay_switch` 仍可用於反向操作。
- **D4 workspace 主權模型**：現存依賴 `/home/zeng/relay.json` 或其他上層 relay.json 的 session 立即失去綁定 → 錯誤訊息直接指引 `GRAPHIFY_RELAY_ROOT` 或 `relay_init`，遷移路徑單一步驟。先前在 workspace 子目錄 init 的狀態檔將不再被發現 → 需在 workspace root 重新 init（一次性）。
- **D3 拒絕寫入**比現行靜默 upsert 嚴格 → 這是刻意的 fail-loud；誤拒的代價（一次明確錯誤）遠低於壞資料進狀態檔。
- **D5 per-repo snapshot**：與 D1 同型的唯讀 fallback 模式，風險已被 D1 模式覆蓋。
- **D6 workspace_key 變更**：既有錯置快照不自動搬家（歷史資料維持原狀）；新快照起以 repo 真實 workspace 歸屬。

## Migration Plan

1. 程式面：D1-D6 一次落地（同一 plugin crate，分開上反而要處理中間態相容兩次）。
2. 資料面：無需遷移。舊 relay.json 保留；使用者可選擇手動清理受害檔（`/home/zeng/relay.json`）或留著讓 fallback 生效。
3. 部署面：GraphifyRust 重 build 後，依賴 relay 的 session 首次呼叫會收到 D4 的明確錯誤 → 依訊息 init 或設 env。

## Open Questions

- env override 命名已定案：`GRAPHIFY_RELAY_ROOT`（符合 `GRAPHIFY_CONFIG_PATH` 慣例，2026-09-24）。
- **[待討論]** `/home/zeng/relay.json` 受害檔的清理時機（建議：D1-D6 上線後手動刪除或歸檔）。
