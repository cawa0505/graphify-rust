# Tasks: relay-multi-repo-isolation

## 1. Schema：project_context 下放 per-repo

- [x] 1.1 `state.rs`：`RepoState` 新增 `project_context: Option<String>`；`RelayState.project_context` 保留為舊格式唯讀 fallback 欄位（serde default），save 路徑不再寫入
- [x] 1.2 `relay.rs` `relay_init`：改寫入目標 repo 紀錄的 `project_context`（repo 未註冊時先 upsert）
- [x] 1.3 `relay.rs` `vars_for`：渲染取 `repo.project_context`，fallback 鏈 → 舊全域欄位 → "(unset)"
- [x] 1.4 `state.rs`：`RepoState` 新增 `state_snapshot`（open_threads/blockers per-repo）；`RelayState.state_snapshot` 保留為唯讀 fallback（serde default），save/close 不再寫全域
- [x] 1.5 `relay.rs`：`relay_save`/`relay_close` 寫入目標 repo 自身 snapshot；渲染只讀 repo snapshot，fallback 鏈 → 舊全域 → 空

## 2. relay_save 語意

- [x] 2.1 `relay.rs` `relay_save`：移除 first-save-wins guard，改為無條件切換 `active_baton`
- [x] 2.2 輸出訊息改為 `Saved state for "<repo>" (active baton switched to "<repo>").`，涵蓋 save 與 status 兩處輸出點
- [x] 2.3 `relay_switch` 行為不變（反向切換的明確出口，full_flow 測試覆蓋）

## 3. repo 路徑解析與驗證

- [x] 3.1 `state.rs`：`RepoState::for_repo` 移除 `path: name.clone()` 預設；`path` 改存絕對路徑
- [x] 3.2 `relay.rs` `relay_save`：寫入前三層解析（絕對路徑 → `root.join(repo)` 限 root 內 → cwd 相對），全失敗則拒絕寫入並列出嘗試路徑（**monorepo 修訂**：候選僅須存在目錄，非 git repo 也合法；層 2 另加 containment 檢查堵 `../`）
- [x] 3.3 `relay.rs` `vars_for`：git 計算改用解析後絕對路徑；失敗時渲染 `"(git status unavailable: <path>)"`
- [x] 3.4 `relay.rs` `persist_close_snapshot`：workspace_key 改由 `RepoState.path` canonical path derive（`derive_workspace_key`）

## 4. relay root 綁定安全化

- [x] 4.1 `root.rs`：移除 walk-up 搜尋；workspace 主權模型（git repo → toplevel、非 git → cwd、零向上搜尋）
- [x] 4.2 relay 工具無 root 時回明確錯誤 + 指引 `relayInit` 或 `GRAPHIFY_RELAY_ROOT`
- [x] 4.3 `GRAPHIFY_RELAY_ROOT` env override：值為檔案時取其 parent；指向 relay.json 檔本身亦可
- [x] 4.4 `relay_init` 在 `$HOME` 正下方時輸出共用檔警告（不阻擋；`home_warning_needed` 純函式 + 測試）

## 5. 測試

- [x] 5.1 單元：D1 fallback 鏈三案例（`vars_for_project_context_fallback_chain`）
- [x] 5.2 單元：D2 baton 切換 + 輸出格式（full_flow）；D3 拒絕路徑錯誤訊息（`save_rejects_unresolvable_repo_and_lists_tried_paths`）+ root 外絕對路徑（`save_accepts_absolute_path_outside_root`）
- [x] 5.3 單元：D4 workspace root 解析（git repo 內 subdir 不向上搜尋、非 git cwd、env override 生效；root.rs 既有 + 新增測試）
- [x] 5.4 整合 fixture：受害檔模型（全域 context + bare-name path，脫敏）載入 → fallback 與不回寫驗證（`legacy_global_context_used_as_readonly_fallback`；以 tempdir 重建結構，不觸碰真實 victim 檔）
- [x] 5.5 端到端：save A → save B → resume B，context 與 baton 正確（`full_flow_lifecycle` + `per_repo_context_does_not_leak_between_repos`；tempdir 取代真實 `~/proj-a`，避免寫入 $HOME）
- [x] 5.6 單元：D5 snapshot per-repo 隔離 + 舊格式 fallback；D6 close 快照 workspace_key 由 repo path derive（含跨 workspace repo 歸屬測試）

## 6. 驗證與收尾

- [x] 6.1 `cargo test -p graphify-plugin-handoff` 66 全綠；clippy 無警告；`cargo fmt` 已套用
- [x] 6.2 graphify-mcp / graphify-cli 重 build + 實機驗證通過（2026-09-24：臨時 git repo e2e — init 落 toplevel、D2 baton 無條件切換、D1 per-repo context 隔離（DemoWS 不洩漏）、D3 path 存絕對路徑、D4 bare 目錄回 NoRoot 凍結文、GRAPHIFY_RELAY_ROOT 指檔生效、git status 渲染正常）
- [x] 6.3 手動清理指引已寫入變更目錄（`cleanup.md`：`/home/zeng/relay.json` 與受害 repo 檔的刪除/歸檔準則；實際刪除留給使用者手動執行）
- [x] 6.4 openspec validate --strict 通過（"Change 'relay-multi-repo-isolation' is valid"）；主 spec 已 sync 為 `openspec/specs/handoff-relay/spec.md`（含驗證證據），change 已歸檔至 `archive/2026-09-24-relay-multi-repo-isolation`
