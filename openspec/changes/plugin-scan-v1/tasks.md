# tasks: plugin-scan-v1

## 1. Config 擴充

- [x] 1.1 在 graphify-llm config schema 新增 `[plugins.<id>]` 段：`command`（必填 String）、`args`（選用 Vec<String>）、`env`（選用 map）、`cwd`（選用 PathBuf）
- [x] 1.2 新增 `PluginConfig` / `PluginsConfig` 結構與 serde 反序列化，缺 `command` 時報錯
- [x] 1.3 單元測試：解析含多 plugin 的 TOML、缺 `command` 的錯誤、無 plugins 段的空容器

## 2. plugin 子進程管理（graphify-mcp）

- [x] 2.1 新增 `graphify-mcp/src/plugin_host/` 模組（mod.rs + process.rs + framing.rs），維持 300 行/檔上限
- [x] 2.2 framing.rs：Content-Length framing 編碼/解碼（`Content-Length: <n>\r\n\r\n` + JSON body），含讀取循環測試
- [x] 2.3 process.rs：`PluginProcess` spawn（command/args/env/cwd）、stdin/stdout/stderr 管線、進程狀態（Spawning/Ready/Failed）
- [x] 2.4 process.rs：`initialize` 握手 + `notifications/initialized`，超時或錯誤 → Failed 狀態
- [x] 2.5 plugin_host.rs：`PluginHost`（scan from config → spawn all → 健康追蹤）+ `list_tools()` 聚合 + `call_tool()` 轉發
- [x] 2.6 單元測試：mock plugin 子進程（測試 fixture 二進位或 shell script）——initialize 成功/失敗、工具聚合、呼叫轉發、單 plugin 失敗隔離

## 3. MCP 整合（graphify-mcp main.rs）

- [x] 3.1 啟動時從 config 掃描 plugins，建 `PluginHost`
- [x] 3.2 `tools/list`：內建 6 工具 + plugin 工具（`graphify_plugin_<id>_<tool>` 前綴）
- [x] 3.3 `tools/call`：`graphify_plugin_*` 工具轉發到對應 plugin，plugin 不可用回明確錯誤
- [x] 3.4 `notifications/graph_updated`：工具完成後廣播 + `graphify_notify_plugins` 手動觸發工具
- [x] 3.5 驗證：無 plugin 段時 server 行為與現況完全一致（回歸）

## 4. 文件

- [x] 4.1 更新 docs/plugin_system.md：第三方 plugin 掃描/聚合/前綴命名章節
- [x] 4.2 更新 docs/plugin-sdk-thirdparty.md：設定範例（`[plugins.<id>]` TOML）與工具命名規則
- [x] 4.3 更新 docs/core.md（如需）：MCP 側 plugin 架構圖

## 5. 驗證

- [x] 5.1 `cargo fmt --all --check` + `cargo clippy -p graphify-mcp --all-targets`（零警告，#2368）
- [x] 5.2 `cargo test -p graphify-mcp -p graphify-llm` 全綠
- [x] 5.3 `openspec validate plugin-scan-v1` 通過
