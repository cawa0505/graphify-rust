# Proposal: relay-multi-repo-isolation

## Why

在 NexusHub 工作區透過 MCP 呼叫 `graphify_relay_save(repo="NexusHub", ...)`，回傳 "Saved state for 'NexusHub'" 但同時印出 "Active baton: ArgusOrchestrator"；渲染出的 RESUME handoff 中 **Project context 是 ArgusOrchestrator 的模組說明**（另一個 repo 的內容），且 Status 顯示 "Last commit: (not a git repo)"——儘管呼叫當下的 cwd（NexusHub）是合法 git repo。

追蹤後確認這不是單一 bug，而是**三個獨立缺陷疊加**，都位於 `GraphifyPlugins/graphify-plugin-handoff`（經 path dep 靜態連結進 graphify-mcp）：

1. **`project_context` 是全域單值**（state.rs:31），只由 `relay_init` 寫入（relay.rs:432），渲染時無條件注入每個 repo 的 handoff（relay.rs:276）。多 repo 共用一個 relay root 時，A repo 的 context 會渲染進 B repo 的 handoff。
2. **`relay_save` 不切換 baton**：只在 baton 為空時設定（relay.rs:488-490，first-save-wins）。對已持有 baton 的 root save 另一個 repo，baton 維持舊值，之後無參數的 `relay_resume` 會靜默 resume 錯的 repo；save 輸出卻印 "Active baton: ..." 造成已切換的錯覺。
3. **repo 路徑不驗證 + root 綁定過寬**：`RepoState.path` 預設等於 repo 名稱（state.rs:97-103），git 在 `root.join(path)` 執行（relay.rs:273-274）——root 解析到 `/home/zeng` 時 git 會跑在 `/home/zeng/NexusHub`（不存在）→ 永遠 "(not a git repo)"。且 walk-up 的 `$HOME` 邊界是**含端點**（root.rs:66），cwd 在 `$HOME` 下就會綁到共用的 `/home/zeng/relay.json`——這正是 root.rs:20-22 自己已記錄的 "$HOME stray relay shared by 20 projects" 失敗模式。`relay_save` 對 repo 參數做靜默 upsert（relay.rs:459-462），不檢查路徑存在、不檢查是否 git repo。

實際受害狀態檔：`/home/zeng/relay.json`（`active_baton="ArgusOrchestrator"`、`repos={ArgusOrchestrator, NexusHub}`、全域 `project_context` 為 ArgusOrchestrator 內容、NexusHub 紀錄 `path="NexusHub"`）——可直接作為重現與遷移測試案例。

## What Changes

- **`project_context` 改為 per-repo**：搬進 `RepoState`，由 `relay_init` 寫入目標 repo 紀錄；渲染只使用被渲染 repo 自己的 context。既有 relay.json 的全域值作為唯讀 fallback（repo 無自有值時繼承），不做破壞性遷移。
- **`relay_save` 語意改為「save 即工作於此 repo」**：save 時將 `active_baton` 切換到被 save 的 repo，輸出同時明確回報 saved repo 與當前 baton（決策點見 design.md，含保守替代方案）。
- **repo 路徑解析與驗證**：`relay_save` 必須將 repo 解析為實際目錄（絕對路徑，或相對綁定 root 可解析），不存在或非 git repo 時拒絕寫入並回報嘗試路徑；git 狀態無法取得時輸出必須帶明確警告與嘗試路徑，不得靜默渲染 "(not a git repo)"。
- **relay root 綁定安全化**：walk-up 邊界改為 `$HOME` **排除**（永不隱式綁定 `/home/zeng/relay.json`）；找不到合法 root 時 relay 工具回錯並指引先 `relay_init`，而非靜默建立/共用；需要刻意共用 root 時提供明確 env override（如 `GRAPHIFY_RELAY_ROOT`）。

## Capabilities

### New Capabilities

- `handoff-relay`: relay 狀態的多 repo 隔離語意——per-repo project context、baton 切換規則、repo 路徑驗證、relay root 綁定邊界。

### Modified Capabilities

- 無。現有 specs 中無 relay 能力規格（`handoff-pruning` 僅涵蓋 handoff snapshot 的 TTL/容量修剪）；本變更首次為 relay 工具建立 capability spec。

## Impact

- **主要修改位置**：`GraphifyPlugins/graphify-plugin-handoff/src/{relay.rs, state.rs, root.rs}`（GraphifyRust 透過 `graphify-mcp/Cargo.toml:16` 的 path dep 靜態連結；CLI dispatch 在 `graphify-cli/src/main.rs:1212-1237`，MCP dispatch 在 `graphify-mcp/src/main.rs:1354-1419`，兩端共用同一 plugin，行為修正一處生效）。
- **向後相容**：既有 relay.json 不需手動遷移——per-repo context 讀取時 fallback 到舊全域值；schema_version 維持可讀。
- **行為變更**：`relay_save` 會切換 baton（原為 first-save-wins）；在 `$HOME`（含）以上路徑啟動的 MCP server 不再隱式綁定共用 relay.json——依賴此行為的工作流程需改用 env override。
- **觀察證據**：`/home/zeng/relay.json` 為真實受害檔，可作為整合測試 fixture。
