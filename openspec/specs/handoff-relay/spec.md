# handoff-relay Specification

## Purpose

定義 relay 工具（`relay_init/save/switch/resume/close/status`）的多 repo 隔離語意：per-repo project context、baton 切換規則、repo 路徑驗證、relay root 綁定邊界。目標：多個 repo 的紀錄共存於同一 relay 狀態檔時，任一 repo 的 handoff 渲染不得被其他 repo 的內容污染，且狀態檔不得寫入未經驗證的 repo 路徑。

## Requirements

### Requirement: Per-repo project context

`project_context` SHALL 以 repo 為單位儲存於各 repo 紀錄。渲染任一 repo 的 handoff 時，project context SHALL 只取自該 repo 自身的紀錄。對於尚無自有 context 的 repo（舊格式狀態檔），渲染 SHALL fallback 到狀態檔的舊全域 `project_context` 欄位（唯讀），兩者皆缺時顯示 "(unset)"。寫入路徑 SHALL 永不更新舊全域欄位。

#### Scenario: 各 repo context 不互相污染

- **GIVEN** relay 狀態檔內 repo A 的 `project_context` 為 "Alpha"、repo B 為 "Beta"
- **WHEN** 渲染 repo B 的 RESUME handoff
- **THEN** Project context 區塊顯示 "Beta"，不含 "Alpha" 的任何內容

#### Scenario: 舊格式唯讀 fallback

- **GIVEN** 舊格式狀態檔：全域 `project_context` 為 "Legacy"，repo B 無自有 context
- **WHEN** 渲染 repo B 的 handoff
- **THEN** Project context 顯示 "Legacy"
- **AND** 後續任何對 repo B 的 save SHALL 不回寫全域欄位

### Requirement: relay_save 切換 active baton

`relay_save` 將指定 repo 的狀態寫入後，SHALL 將 `active_baton` 切換為該 repo，並在輸出中明確回報 `Saved state for "<repo>" (active baton switched to "<repo>").`。輸出 SHALL 永不呈現與實際 baton 狀態不符的資訊。`relay_switch` 維持現行語意，作為反向切換的明確出口。

#### Scenario: Save 另一個 repo

- **GIVEN** active baton 為 "A"，狀態檔內 repo B 已註冊
- **WHEN** 呼叫 `relay_save` 並指定 repo "B"
- **THEN** 寫入成功且 `active_baton` 變為 "B"
- **AND** 輸出包含 `Saved state for "B" (active baton switched to "B").`

#### Scenario: 無參數 resume 命中最近 save 的 repo

- **GIVEN** 前述 save 之後
- **WHEN** 呼叫 `relay_resume` 且未指定 repo
- **THEN** resume 的對象為 "B"（非 "A"）

### Requirement: repo 路徑寫入時驗證

`relay_save` SHALL 在寫入前將 repo 解析為實際目錄：依序嘗試（1）repo 參數為絕對路徑且該目錄存在、（2）`relay root.join(repo)` 存在且位於 relay root 內、（3）MCP server cwd 相對路徑存在。候選僅需為存在的目錄（repo 是否為 git repo 不影響寫入驗收，僅影響渲染時的 git 狀態診斷）。全部無法解析時 SHALL 拒絕寫入，錯誤訊息 SHALL 列出所有嘗試路徑。解析成功時 `RepoState.path` SHALL 儲存絕對路徑，不得以裸 repo 名稱作為路徑預設值。

#### Scenario: 路徑無法解析時拒絕寫入

- **GIVEN** repo 名 "Foo" 在 root 下與 cwd 下皆無對應目錄
- **WHEN** 呼叫 `relay_save(repo="Foo", ...)`
- **THEN** 寫入被拒絕，狀態檔位元組不變
- **AND** 錯誤訊息包含所有嘗試過的路徑

#### Scenario: git 狀態取得失敗的可診斷輸出

- **GIVEN** repo 紀錄的 path 指向存在但 git 指令失敗的目錄
- **WHEN** 渲染該 repo 的 handoff
- **THEN** Status 顯示 `"(git status unavailable: <解析後路徑>)"`，不顯示誤導性的 "(not a git repo)"

### Requirement: Per-repo state snapshot

`state_snapshot`（open_threads、blockers）SHALL 以 repo 為單位儲存於各 repo 紀錄。`relay_save` 與 `relay_close` SHALL 只寫入目標 repo 自身的 snapshot；渲染任一 repo 的 handoff 時 SHALL 只讀取該 repo 自身的 snapshot。對於尚無自有 snapshot 的 repo（舊格式狀態檔），渲染 SHALL fallback 到狀態檔的舊全域 `state_snapshot`（唯讀）。寫入路徑 SHALL 永不更新舊全域 snapshot。

#### Scenario: threads 不跨 repo 污染

- **GIVEN** repo A 的 snapshot 含 open_thread "T-A"，repo B 的 snapshot 含 open_thread "T-B"
- **WHEN** 渲染 repo B 的 handoff
- **THEN** open_threads 顯示 "T-B"，不含 "T-A"

#### Scenario: 舊格式 snapshot 唯讀 fallback

- **GIVEN** 舊格式狀態檔僅有全域 snapshot，repo B 無自有 snapshot
- **WHEN** 渲染 repo B 的 handoff
- **THEN** open_threads 來自全域 snapshot
- **AND** 後續任何對 repo B 的 save SHALL 不回寫全域 snapshot

### Requirement: Close snapshot 的 workspace 歸屬

`relay_close` 寫入 graphify.db 的 handoff snapshot，其 `workspace_key` SHALL 由目標 repo 解析後的絕對路徑（canonical path）derive，而非綁定時的 cwd。既有錯置的歷史快照 SHALL 不自動搬移。

#### Scenario: 在 workspace A 關閉 repo B

- **GIVEN** relay root 綁定於 workspace A，repo B 位於 workspace B（路徑已驗證）
- **WHEN** 呼叫 `relay_close(repo="B")`
- **THEN** 快照以 workspace B 的 workspace_key 寫入 graphify.db

### Requirement: relay root 綁定於 workspace root

relay root SHALL 為 workspace root，且 SHALL 不執行任何向上（walk-up）搜尋：cwd 位於 git repo 內時，workspace root 為 `git rev-parse --show-toplevel`；非 git 目錄時為 cwd 本身。workspace root 無 relay 狀態檔時，relay 工具 SHALL 回明確錯誤並指引（`relay_init` 或 `GRAPHIFY_RELAY_ROOT` env override），不得靜默建立或共用狀態檔。設定 `GRAPHIFY_RELAY_ROOT` 時 SHALL 跳過 workspace root 解析，直接綁定該路徑。

`relay_init` SHALL 拒絕在非 git 的 `$HOME` 本身建立 relay root（凍結錯誤訊息：`refusing to init relay at $HOME; run inside a project directory or set GRAPHIFY_RELAY_ROOT`），SHALL NOT 寫入任何檔案。此拒絕 SHALL NOT 適用於：(a) `$HOME` 為 git repo（罕見但合法）；(b) `GRAPHIFY_RELAY_ROOT` 明確指向 `$HOME`（顯式表態優先）。

#### Scenario: 不再向上搜尋 relay 狀態檔

- **GIVEN** `/home/user/proj-a` 為 git repo 且其 root 無 relay.json，`/home/user/relay.json` 存在，MCP server 啟動於 `/home/user/proj-a/src`
- **WHEN** 呼叫任何 relay 工具
- **THEN** 嘗試綁定 workspace root `/home/user/proj-a`，不綁定 `/home/user/relay.json`
- **AND** 回傳的錯誤訊息指引先 `relay_init` 或設定 `GRAPHIFY_RELAY_ROOT`

#### Scenario: env override 刻意共用

- **GIVEN** `GRAPHIFY_RELAY_ROOT=/home/user/relay.json` 已設定
- **WHEN** MCP server 啟動於任意目錄並呼叫 relay 工具
- **THEN** 直接綁定 `/home/user/relay.json`，不執行 workspace root 解析

#### Scenario: init 於非 git 的 $HOME 硬錯

- **GIVEN** 使用者在非 git 目錄的 `$HOME` 本身執行 `relay_init`，且未設定 `GRAPHIFY_RELAY_ROOT`
- **WHEN** init 解析 workspace root = cwd = `$HOME`
- **THEN** 回傳錯誤 `refusing to init relay at $HOME; run inside a project directory or set GRAPHIFY_RELAY_ROOT`
- **AND** `$HOME` 下 SHALL NOT 新增 relay.json、specs/、.code-relay/ 任何檔案

#### Scenario: $HOME 為 git repo 時允許 init

- **GIVEN** `$HOME` 本身為 git repo（含 `.git`）且無 relay.json
- **WHEN** 在 `$HOME` 執行 `relay_init`
- **THEN** init 正常成功（workspace root = git toplevel = `$HOME`）

#### Scenario: env override 指向 $HOME 時允許 init

- **GIVEN** `GRAPHIFY_RELAY_ROOT=/home/user`（目錄）已設定，`/home/user` 非 git 且無 relay.json
- **WHEN** 在任意目錄執行 `relay_init`
- **THEN** init 於 `/home/user` 成功（顯式表態優先於 $HOME 防線）

## Verification Evidence

- 2026-09-24 實作完成（change `relay-multi-repo-isolation`，實作於 GraphifyPlugins/graphify-plugin-handoff）：
  - `cargo test -p graphify-plugin-handoff`：66 passed / 0 failed（含 D1 fallback 鏈、D2 baton 切換、D3 拒寫與絕對路徑、D4 root 解析與 env override、D5 per-repo snapshot、D6 跨 workspace 歸屬、舊格式唯讀 fallback、無參數 resume 等情境測試）
  - `cargo test -p graphify-mcp -p graphify-cli`：51 passed / 0 failed
  - `cargo clippy --all-targets`：0 警告；`cargo fmt --check`：通過（兩 repo）
  - 實機 e2e（graphify CLI binary、臨時 git repo）：init 落 `git rev-parse --show-toplevel`、baton 無條件切換、per-repo context 不洩漏、`relay.json` 的 `path` 存絕對路徑、bare 目錄回 NoRoot 凍結文、`GRAPHIFY_RELAY_ROOT` 指向檔案生效、git status 渲染正常
  - `openspec validate relay-multi-repo-isolation --strict`：valid
