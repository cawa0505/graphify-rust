# 實施任務清單：Domain Hub

- [x] 1. 設計 6 大領域工具的 JSON Schema 與 Action 定義
- [x] 2. 修改 `graphify-mcp` 的 `tools/list` 處理常式，聚合輸出
- [x] 3. 實作 `tools/call` 的 Action 派發分流層 (Dispatcher)
- [x] 4. 保留舊工具名稱的相容轉發分支
- [x] 5. 撰寫單元測試與端對端 tools/call 驗證
      （驗證證據：`cargo test -p graphify-mcp` 56/56 全綠——
      `test_domain_tools_count_and_names`、`test_resolve_maps_action_to_legacy_name`、
      `test_resolve_inline_params_merge`、`test_resolve_missing_action_lists_valid`、
      `test_tools_list_returns_six_domain_hubs`、hub→legacy 轉發 e2e；clippy 0、fmt 乾淨）
- [x] 6. 重新打包發佈至 Unicron LXC 並更新 NexusHub 註冊狀態
      （併入 relay-remote-transport Task 3 單一 cutover 一次完成，見 2026-09-30 排程決策。
      實證：217/202 `/opt/nexus/bin/graphify-mcp` 已同步 release sha `d57de87bce69b19f`，
      三節點 nexus /health 回 graphify http healthy——domain-hub 6 工具即上線工具面）
