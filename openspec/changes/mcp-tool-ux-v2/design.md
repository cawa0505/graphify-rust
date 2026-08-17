## Context

graphify-mcp 的 tool 註冊集中在 `main.rs`，每個 tool 以 `Tool` struct 定義 name/description/inputSchema，透過 `tool_registry` Vec 管理。外掛 plugin 的 tools 經由 `PluginHost::list_tools()` 動態加入。broadcast 機制在 `plugin_host/host.rs`，但 trigger 點散布在 `index`/`extract`/`reindex` 等 CLI 路徑，需手動呼叫 `notify_plugins`。

現有命名混亂源於多階段開發：graph 工具最早使用 `graphify_graph_*` prefix，後續加入的熱修復工具（review, coverage, telemetry, opendoc）沒有遵循統一規則，部分用 camelCase、部分掛在錯誤 domain 下。

## Goals / Non-Goals

**Goals:**
- 統一所有 tool 命名為 `graphify_<domain>_<action>` snake_case
- 將 coverage 工具從 review domain 獨立
- 自動 broadcast 在 index/extract/reindex 完成後
- 支援 workspace_key 省略（使用 active workspace）
- 新增 `graphify_help` 探索入口
- 標準化 error/empty 回應格式

**Non-Goals:**
- 不改變 CLI 命令列界面（只改 MCP tool 層）
- 不改變 plugin SDK 的通訊協定（JSON-RPC 格式不變）
- 不改變 graph.toon 序列化格式

## Decisions

### D1: 以 main.rs 的 tool_registry 為中心重構

**決策：** 在 `main.rs` 新增 `fn register_tool(&mut Vec<Tool>, name, desc, schema, handler)` helper，統一所有 tool 註冊入口。舊名稱直接刪除，不保留 alias。

**理由：** 現有 tool 註冊散落在數個 match blocks 中（opendoc tools → line 797, telemetry → line 881, coverage → line 930, relay → 獨立區塊）。集中註冊讓命名規則一目瞭然，未來新增工具也只需呼叫 helper。

**替代方案：** 保留舊名作為 alias → 被否決，因為保留 alias 只會讓混亂持續，且 agent 會繼續學到舊名稱。

### D2: 自動 broadcast 用 hook 模式加在 run_graph_tool 之後

**決策：** 在 `graph_reindex` 成功處理後，直接呼叫 `plugin_host.broadcast(graph_updated_event)`。CLI 的 `index`/`extract` 命令也改為完成時自動 broadcast。

**理由：** `reindex` 是 MCP tool 呼叫，回傳前 broadcast 即可。CLI 的 `index`/`extract` 本來就有 `notify_plugins` 手動呼叫，改成自動後流程更簡潔。

**替代方案：** 在 core 層加 event hook → 被否決，因為 broadcast 機制在 mcp 層，core 不該依賴 plugin。

### D3: workspace_key 用 Registry 的 active workspace 做預設

**決策：** 在 `memory_query` 等工具中，當 `workspace_key` 參數為空時，從 `Registry::active()` 取得當前 workspace key。

**理由：** Registry 已經有 `active_workspace` 概念（`list_returns_all_registered`, `switch_makes_one_active` 測試已驗證），直接複用即可。

**替代方案：** 新增 session context 物件 → 過度設計，Registry 已經夠用。

### D4: help tool 動態列舉已註冊 tools

**決策：** `graphify_help` 從 `tool_registry` + `plugin_host.list_tools()` 動態生成回應，按 domain 分組。

**理由：** 靜態列表會隨著 plugin 增減而過時。動態列舉保證永遠是最新的。

**替代方案：** 硬編碼幫助文字 → 會與實際工具列表脫節。

### D5: Error 協議用 Result 的統一格式化

**決策：** 所有 tool handler 回傳 `Result<String>`，在頂層 dispatch 統一處理三種狀態：
- `Ok(data)` → 正常回傳
- `Err(anyhow::anyhow!("..."))` → error
- 空結果用 `Ok("[domain] feature: no data".to_string())` 而非 `Err`

**理由：** 現有工具已經部分遵循此模式（如 `coverageBlindspots` 回傳 `"[coverage] no blindspots..."`），只需統一所有工具。

## Risks / Trade-offs

- **[Breaking] 舊工具名稱被移除** → agent 可能會在幾個 session 內嘗試舊名稱，需等 MCP server 更新後自動適應
- **[Risk] 自動 broadcast 增加 plugin 啟動次數** → 每次 reindex 都 broadcast，頻繁使用時 plugin 負載可能增加。mitigation: broadcast 是 fire-and-forget，plugin 非同步處理
- **[Risk] workspace_key 省略可能導致跨 workspace 混淆** → agent 可能忘記當前 workspace 是哪個。mitigation: 保留明確傳遞參數的能力，讓 agent 在多 workspace 場景下可明確指定
- **[Trade-off] 不保留舊名稱 alias** → 短期內有使用舊名稱的已有 script 會斷掉。但這是 MCP tool，只有 AI agent 使用，沒有外部 API 消費者

## Migration Plan

1. 先在 main.rs 的 tool_registry 加上 `register_tool()` helper
2. 批次重新命名所有工具（一次改完，避免部分過渡期）
3. 新增 `graphify_help` 工具
4. 在 reindex handler 中加上自動 broadcast
5. 在 CLI index/extract 命令中加上自動 broadcast
6. 讓 workspace_key 參數變為 optional
7. 統一 error 回應格式（最後做，因為不影響功能）
8. 全面測試後 deploy

### D6: notify_plugins 保留改名為 graphify_plugin_notify

**決策：** 保留 `graphify_graphify_notify_plugins` 並改名為 `graphify_plugin_notify`，作為自動 broadcast 的補充。在需要手動觸發（如外部修改 graph file 後）時仍可使用。

### D7: Relay auto-save 本次實作

**決策：** 在 relay 工具中加入 auto-save 機制：當 `relay close` 或 `relay switch` 被呼叫時，自動先執行 `relay save` 再進行後續操作。`relay init` 時若已有未完成的 relay 狀態，自動 save 後再 init。

## Open Questions

（無 — 本次變更範圍已確定）