# Design: relay-workspace-context

## Context

nexus gateway 拓撲：opencode → nexus → graphify-mcp（stdio child）。stdio child 的 cwd 由 systemd user unit 決定（恆 `$HOME`），MCP `tools/call` payload 不含 caller 環境。relay 的身份推導（root 解析、repo resolution、渲染診斷）全部基於 `self.cwd()`（= server cwd），在 gateway 下恆為 `$HOME`。

既有 gateway 慣例（對齊對象）：
- `opendoc_index_path(path)`：absolute path 必填
- `memory_query(workspace_key)`：caller 傳入 routing key

## Decisions

### D1 — workspace path 參數：MCP 必填 / CLI optional（定案 2026-09-24）

MCP 端六個 relay 工具 schema 增加 `path`（absolute，必填）。CLI 端維持 process cwd 推導（optional）。

理由：MCP 是 gateway 拓撲下唯一可靠的身份來源（server cwd 恆錯）；CLI 的 process cwd 本身就是使用者所在 workspace，重複聲明只是噪音。兩端語意分離是刻意的，不是不一致。

實作面：plugin 層 `RelayPlugin` 的 bind 面改為吃 caller path（`WorkspaceContext.root_path` 已存在 — `bind_for_cli`/`build_relay_plugin` 已用它，MCP 端把 `path` 參數填進 context 即可，plugin 核心不需要新參數通道）。

### D2 — root 解析：caller path 的 git toplevel，零 cwd fallback

`workspace_root(start)` 改為：
1. `GRAPHIFY_RELAY_ROOT` env override（保留，顯式表態）
2. `git_toplevel(caller_path)` — caller path 在 git repo 內 → toplevel
3. caller path 本身（非 git workspace — 保留：非 git 專案目錄是合法 workspace，D4 模型本就允許）

**移除的是 server-cwd fallback**：MCP 端無 path 參數時直接回錯（凍結文：`workspace context required: pass the absolute path of your workspace`），絕不退回 server cwd。`$HOME` 硬錯（handoff-relay delta 已落地）保持不變。

### D3 — repo resolution 基於 caller path；Tried 候選去重

`resolve_repo_path(root, cwd, repo)` 的 `cwd` 參數改吃 caller path（workspace root 下的相對解析語意不變）。三層解析（絕對 → root 相對 → caller-path 相對）保留，但：

- caller path == root 時（CLI direct-spawn 常態）第二、三層同源 → **去重**（同一候選只列一次）
- 錯誤訊息出現重複同值候選 = 回歸信號（測試剛性綁定：`tried_windows_dedup`）

### D4 — registry 汙染：報而詢問是否刪（定案 2026-09-24 修訂）

doctor 對 registry 的 workspace 紀錄做唯讀檢查：`root_path` 為存在目錄但取不到 git toplevel（非 git），且（`derive_workspace_key(root_path)` 與紀錄 key 不符，或 `root_path == $HOME`）→ WARN（"registry workspace record likely polluted by gateway cwd; verify manually"）。實測真實汙染紀錄（`/home/zeng`）的 key 正是由 gateway cwd 自身 derive，key 相符不能作為免查條件，故列 $HOME 等值補位。doctor SHALL NOT 自動刪除；輸出後 SHALL 詢問使用者是否刪除該紀錄（互動確認），使用者明確同意才執行刪除。Non-TTY 跳過刪除僅報告；`--fix` 不觸發此刪除面 —— 刪除只走互動詢問同意路徑。

### D5 — 測試：gateway 拓撲模擬

新增 e2e：spawn graphify-mcp 時 cwd 設為非 git 的 tempdir（模擬 systemd `$HOME`），呼叫 save 帶 `path=/real/ws`：
- root 落在 `/real/ws` 的 git toplevel
- relay.json 寫入該 repo
- 不帶 path → 凍結錯誤文 + 零檔案寫入

既有 direct-spawn e2e 全數保留（CLI 不回歸驗證）。

## Risks / Trade-offs

- MCP schema 加必填欄位 = breaking change for既有 caller（nexus 側需同步傳 path）→ 凍結錯誤文明確指引，遷移成本一次
- CLI/MCP 語意分離 → 文件（PROTOCOL.md）須明寫兩端差異，避免未來被「統一」掉
- env override 與 path 參數同時存在時：env 優先（顯式表態層級高於參數），design 明記
