# relay-workspace-context

## Why

relay-multi-repo-isolation（D1–D6）已上線，D3 fail-loud 與 D4 零 walk-up 在 direct-spawn 實測生效。但生產拓撲（opencode → nexus → graphify-mcp stdio child）下發現結構性盲點：

- MCP `tools/call` 不攜帶 caller 的 workspace 身份，relay 卻用 **server process cwd** 推導一切（`root.rs workspace_root()`：env → git_toplevel(cwd) → cwd fallback；`relay.rs` repo resolution：`root.join(repo)` 與 `cwd.join(repo)`）。
- nexus 管理的 graphify-mcp cwd 恆為 `$HOME`（systemd user unit 預設）→ 非 git dir → cwd fallback → root = `$HOME` → 兩個候選路徑同源（錯誤訊息 `Tried: /home/zeng/NexusHub, /home/zeng/NexusHub` 兩筆同值即為證據）→ 真 workspace 永遠不參與解析。
- 同源副作用：任何成功 save 都會重建 `/home/zeng/relay.json` stray 檔（已修復的 D4 情境經由 cwd fallback 復活）。
- registry 已有被 gateway cwd 汙染的 workspace 紀錄（path=/home/zeng）。

此假設只在 direct spawn（CLI / 單獨跑 MCP）成立；nexus 拓撲下恆不成立。GraphifyRust 的 e2e 用 direct spawn 跑，測不出。

## What Changes

1. relay MCP 工具（save/init/switch/resume/close/status）schema 增加 caller 傳入的 workspace context：absolute path 必填（MCP 端），與 `opendoc_index_path(path)` absolute 必填、`memory_query(workspace_key)` caller 傳入的既有 gateway 模式一致。
2. root 解析改為「該 workspace path 的 git toplevel」，不再依賴 server cwd。
3. 移除非 git cwd fallback（root.rs 的 `start.to_path_buf()`）：解析不到 workspace context 時回明確錯誤指引傳 path —— 永遠不得靜默 root=$HOME。
4. repo resolution 同步改為基於 caller path；錯誤訊息中的 Tried 候選不得重複同值（重複即代表又退回 cwd 同源）。
5. CLI direct-spawn 路徑不回歸：cwd 可用時維持現行為（CLI 端 path optional）。
6. doctor 對 registry 的 cwd 汙染 workspace 紀錄報而詢問是否刪（WARN + 互動確認，使用者同意才刪）。

## 決策（2026-09-24 定案）

- **path 參數**：MCP 必填 / CLI optional —— 兩端語意分離。MCP 是 gateway 拓撲下唯一可靠的身份來源；CLI 的 process cwd 本身就是使用者所在 workspace，無需重複聲明。
- **registry 汙染清理**：報而詢問是否刪 —— doctor WARN 提示後互動詢問，使用者明確同意才刪除該紀錄；不自動刪。

## Impact

- `graphify-plugin-handoff`：root.rs（解析）、relay.rs（resolution + bind 面）、lib.rs（錯誤）
- `graphify-mcp`：工具 schema（path 必填）+ 呼叫端傳遞
- `graphify-cli`：零行為變更（驗證不回歸）
- e2e：新增「模擬 gateway cwd ≠ workspace」測試
