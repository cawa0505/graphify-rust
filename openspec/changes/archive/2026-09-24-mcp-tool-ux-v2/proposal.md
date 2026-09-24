## Why

Graphify 的 MCP tools 是 AI agent 與 codebase 深度互動的核心介面，但現有工具命名不一致、探索門檻高、缺少自動化流程，導致 agent 使用效率不如預期。本次變更旨在優化工具 UX，讓常用操作更快、更直覺，降低 agent 的認知負擔。

## What Changes

1. **Tool 命名一致性清理** — 統一所有 graphify MCP tools 為 `graphify_<domain>_<action>` 格式，消除 `graphify_graphify_*` 雙重 prefix
2. **workspace_key 自動偵測** — `memory_query` 等需要 workspace 路由的工具，在當前 workspace 下可省略參數
3. **探索入口** — 新增 `graphify_help` 工具，列出所有可用工具與簡短說明
4. **自動 broadcast** — `index`/`extract`/`reindex` 完成後自動發送 `graph_updated` 事件，不再需要手動呼叫 `notify_plugins`
5. **Relay 流程簡化** — 支援 auto-save 在 session 邊界，減少手動 init/save/close 流程
6. **Error 一致性** — 統一空結果與錯誤的表示方式，讓 agent 可一致判斷
7. **Coverage 工具重新命名空間** — 將 `coverageIngest/coverageGetContext/coverageBlindspots` 從 `review` 領域移至獨立的 `coverage` 領域

## Capabilities

### New Capabilities
- `mcp-help-tool`: 提供 `graphify_help` 工具，列出所有 MCP tools 與說明
- `mcp-auto-broadcast`: index/extract/reindex 完成後自動發送 `graph_updated` 通知
- `mcp-workspace-context`: 當前 workspace 的 MCP 工具可省略 `workspace_key` 參數
- `mcp-error-protocol`: 統一 MCP tools 的空結果與錯誤回應格式

### Modified Capabilities
- `mcp-server`: **BREAKING** — 重命名多個工具，變更 tool ID 格式，變更部分工具參數簽名
- `plugin-events`: 從手動 broadcast 改為自動 broadcast，變更事件觸發時機

## Impact

- **graphify-mcp**: 主要變更區域，tool 註冊、命名、路由
- **graphify-core**: 新增自動 broadcast hook 點
- **graphify-cli**: relay 流程簡化，支援 auto-save
- **Plugin SDK**: 所有 plugin 工具 prefix 可能變更
- **Backward Compatibility**: 舊工具名稱將不再可用，需更新使用方