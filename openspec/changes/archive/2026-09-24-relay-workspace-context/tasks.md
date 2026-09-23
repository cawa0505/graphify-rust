# Tasks: relay-workspace-context

## 1. plugin 核心身份改 caller workspace（graphify-plugin-handoff）

- [x] 1.1 `root.rs`：`workspace_root(caller)` 重構 —— env override → `git_toplevel(caller)` → caller 本身；移除 server-cwd fallback 面（spec handoff-relay MODIFIED：身份來源 SHALL 為 caller workspace）（驗證：caller path in git repo → toplevel；caller path 非 git 專案 → caller 本身；env 表態優先）
- [x] 1.2 `relay.rs`：六工具 bind 面吃 caller path（`WorkspaceContext.root_path` 已存在，bind 已吃 context —— 核對 bind/bind_for_cli 傳遞鏈零遺漏）（驗證：`git_toplevel` 邏輯測試 + bind 傳遞鏈測試）
- [x] 1.3 `resolve_repo_path`：解析基準改 caller workspace（root + caller path）；Tried 候選去重（spec：重複候選 = 回歸信號，凍綁定測試 `tried_windows_dedup`）（驗證：caller==root 時單候選單列；gateway 模擬時候選不含 server cwd 同源路徑）

## 2. MCP 工具 schema（graphify-mcp）

- [x] 2.1 六個 relay 工具 schema 加 `path`（absolute 必填）+ 未傳時凍結錯誤 `workspace context required: pass the absolute path of your workspace`、零檔案寫入（spec MODIFIED Scenario 必驗）（驗證：未傳 path → 凍結錯誤文 + $HOME 零殘留；帶 path → root 落 caller toplevel）

## 3. CLI 不回歸驗證（graphify-cli）

- [x] 3.1 直接以臨時 git repo 實跑 `graphify handoff`（無 path）：維持 process cwd toplevel 推導，零行為變更（spec CLI Scenario）（驗證：terminal 實跑輸出）

## 4. doctor registry 檢查面（報而詢問是否刪）

- [x] 4.1 doctor 對 registry workspace 紀錄唯讀檢查（`path` 非 git 目錄且與 `workspace_key` derive 來源不符 → WARN）（驗證：tempdir registry fixture 測試 WARN 觸發/不觸發兩案例）
- [x] 4.2 WARN 後互動詢問是否刪除，使用者明確同意才刪；不自動刪、`--fix` 不觸發此面（design D4 修訂版）（驗證：同意 → 刪；拒絕 → 報告零變更； Non-TTY → 跳過刪除僅報告）

## 5. e2e gateway 拓撲模擬

- [x] 5.1 e2e：spawn graphify-mcp 時 cwd 設非 git tempdir（模擬 systemd `$HOME`）；帶 `path=/real/ws` save → root 落 caller toplevel、relay.json 寫該 workspace、診斷顯真實 commit（spec nexus Scenario）（驗證：e2e terminal 輸出）
- [x] 5.2 e2e：同拓撲未傳 path → 凍結錯誤、零殘留（spec MCP Scenario e2e 面）（驗證：e2e terminal 輸出）

## 6. 收尾

- [x] 6.1 兩 workspace `cargo fmt` + `clippy`（all+pedantic）+ 全測試綠、零警告；PROTOCOL.md 補 CLI/MCP 語意分離明文（design Risks：避免未來被統一掉）
- [x] 6.2 openspec validate --strict 通過；spec sync 主 spec `handoff-relay` + archive
