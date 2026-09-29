# 架構設計：graphify-mcp 領域聚合 (Domain Hub)

## 1. 概念模型

```
[LLM Agent]
     │
     ▼ (呼叫 6 大領域工具之一，帶 action 參數)
[graphify-mcp Gateway]
     │
     ├─► graphify_graph(action="query", question="...")
     │        └── 轉發至內部 GraphState / BFS Traversal
     │
     ├─► graphify_relay(action="save", phase="BUILD", ...)
     │        └── 轉發至內部 RelayPlugin::relay_save
     │
     ├─► graphify_opendoc(action="get_context", symbol="...")
     │        └── 轉發至內部 OpendocPlugin::get_context
     │
     ├─► graphify_review(action="resolve", review_id="...")
     │        └── 轉發至內部 ReviewPlugin::resolve
     │
     ├─► graphify_metrics(action="coverage_get", node="...")
     │        └── 轉發至內部 CoveragePlugin / TelemetryPlugin
     │
     └─► graphify_compose(action="render", manifest="...")
              └── 轉發至內部 Compose Engine
```

## 2. 工具宣告規範 (Tool Definition)

每個領域工具宣告清晰的 enum `action` 與選填參數：

### `graphify_relay` 宣告範例：
```json
{
  "name": "graphify_relay",
  "description": "Code Relay 跨 Session/Repo 狀態接力管理入口",
  "inputSchema": {
    "type": "object",
    "properties": {
      "action": {
        "type": "string",
        "enum": ["status", "init", "save", "close", "switch", "resume", "add"],
        "description": "要執行的操作"
      },
      "path": { "type": "string", "description": "工作區絕對路徑 (必填)" },
      "repo": { "type": "string", "description": "Repo 名稱" },
      "phase": { "type": "string", "description": "階段 (PLANNING, EXECUTING 等)" },
      "conf": { "type": "number", "description": "信心指數 1-5" },
      "next": { "type": "string", "description": "下一步啟動指引" },
      "volatile": { "type": "string", "description": "短期易失狀態摘要" },
      "file": { "type": "string", "description": "要匯入的舊 TODO/handoff 文件路徑 (action=add 專用)" }
    },
    "required": ["action", "path"]
  }
}
```

## 3. 路由轉發與相容性保證

1. **`tools/list` 響應**：
   - 預設只回傳這 6 個聚合工具（節省 80% tokens）。
2. **`tools/call` 響應**：
   - 若呼叫 6 大聚合工具，依 `action` 參數轉發至原本的 handler 函式。
   - 若收到舊的具名呼叫（如 `graphify_relay_save`），依舊轉發至 handler，維持向後相容。
