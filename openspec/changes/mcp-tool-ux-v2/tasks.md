## 1. Tool 命名一致性清理

- [ ] 1.1 在 main.rs 新增 `register_tool(&mut Vec<Tool>, name, desc, input_schema, handler)` helper
- [ ] 1.2 重新命名所有 graph 工具：`graphify_graphify_query` → `graphify_graph_query`，`graphify_graphify_path` → `graphify_graph_path`
- [ ] 1.3 重新命名 review 工具：`graphify_reviewGetContext` → `graphify_review_get_context`，`graphify_reviewIngest` → `graphify_review_ingest`，`graphify_reviewResolve` → `graphify_review_resolve`，`graphify_reviewSearchCrg` → `graphify_review_search_crg`，`graphify_reviewGetContext` → `graphify_review_get_context`
- [ ] 1.4 重新命名 opendoc 工具：`opendocIndex` → `graphify_opendoc_index`，`opendocGetContext` → `graphify_opendoc_get_context`，`opendocAuditDrift` → `graphify_opendoc_audit_drift`
- [ ] 1.5 重新命名 telemetry 工具：`telemetryIngest` → `graphify_telemetry_ingest`，`telemetryGetContext` → `graphify_telemetry_get_context`
- [ ] 1.6 重新命名 coverage 工具：`coverageIngest` → `graphify_coverage_ingest`，`coverageGetContext` → `graphify_coverage_get_context`，`coverageBlindspots` → `graphify_coverage_blindspots`
- [ ] 1.7 重新命名 relay 工具：`graphify_relayInit` → `graphify_relay_init`，`graphify_relaySave` → `graphify_relay_save`，`graphify_relayClose` → `graphify_relay_close`，`graphify_relayResume` → `graphify_relay_resume`，`graphify_relayStatus` → `graphify_relay_status`，`graphify_relaySwitch` → `graphify_relay_switch`，`graphify_relayAdd` → `graphify_relay_add`
- [ ] 1.8 重新命名 notify_plugins：`graphify_graphify_notify_plugins` → `graphify_plugin_notify`
- [ ] 1.9 移除舊工具名稱，不保留 alias

## 2. 新增 graphify_help 工具

- [ ] 2.1 實作 `graphify_help` 工具，從 `tool_registry` + `plugin_host.list_tools()` 動態列舉
- [ ] 2.2 按 domain（graph, memory, workspace, coverage, opendoc, review, telemetry, relay, plugin）分組
- [ ] 2.3 確保 `graphify_help` 本身在工具列表中第一個顯示

## 3. 自動 broadcast

- [ ] 3.1 在 `graph_reindex` MCP tool handler 成功後自動呼叫 `plugin_host.broadcast(graph_updated_event)`
- [ ] 3.2 在 CLI `index` 命令成功後自動 broadcast（取代手動 `notify_plugins`）
- [ ] 3.3 在 CLI `extract` 命令成功後自動 broadcast（取代手動 `notify_plugins`）

## 4. workspace_key 自動偵測

- [ ] 4.1 在 `memory_query` 等工具中，當 `workspace_key` 參數為空時從 `Registry::active()` 取得
- [ ] 4.2 新增 `graphify_workspace_status` 工具回傳當前 active workspace 資訊
- [ ] 4.3 更新 tool input schema 讓 `workspace_key` 變成 optional

## 5. Error 協議一致性

- [ ] 5.1 審查所有 tool handler 的回應格式，確保統一使用 `Ok(data)` / `Err(error)` / `Ok("[domain] feature: no data")` 三種狀態
- [ ] 5.2 確保所有「feature not configured」情況回傳明確錯誤訊息，而非空結果

## 6. Relay 流程簡化

- [ ] 6.1 在 `relay_close` 中自動執行 `relay_save`（若尚未 save）
- [ ] 6.2 在 `relay_switch` 中自動執行 `relay_save` 再切換
- [ ] 6.3 在 `relay_init` 中檢查是否有未完成的 relay 狀態，自動 save 後再 init

## 7. 測試與驗證

- [ ] 7.1 更新 MCP server 測試以新名稱驗證每個工具
- [ ] 7.2 驗證自動 broadcast 在 index/extract/reindex 後正確觸發
- [ ] 7.3 驗證 workspace_key 省略時正確使用 active workspace
- [ ] 7.4 驗證 empty result 與 error 的區分正確
- [ ] 7.5 驗證 relay auto-save 流程