# Delta Spec: relay-remote-transport（mcp-server 能力）

## ADDED Requirements

### Requirement: HTTP-only transport（stdio mode 移除）

`graphify-mcp` SHALL 以 **streamable HTTP 作為唯一 MCP transport**：`serve`
模式提供 `/mcp`（MCP initialize / tools/list / tools/call）與 `GET /health`
（無 auth、無 payload 探測端點）。stdio 迴圈 SHALL 移除，不得以 stdio 啟動。
listener SHALL 綁定內網介面（nexus 與 TUI/CLI 皆為遠端 client），可選 Bearer
token（soundwave 先例：nexus.yaml `headers.Authorization`）。

#### Scenario: stdio 移除

- **GIVEN** graphify-mcp 任何啟動方式
- **WHEN** client 嘗試以 stdin/stdout JSON-RPC 呼叫
- **THEN** 不存在此啟動模式（二進位僅 serve；舊 stdio 行為不得復活）

#### Scenario: nexus 接線

- **GIVEN** graphify 服務跑在 workspace host
- **WHEN** nexus 以 `type: http` upstream 指向 `http://<workspace-host>:<port>/mcp`
- **THEN** MCP initialize / tools/list / tools/call 經 streamable HTTP 完成，
  工具行為語意與既有 stdio 版一致（per-call `path` 綁定）

#### Scenario: 服務健康探測

- **GIVEN** 服務執行中
- **WHEN** nexus / 本機腳本探測 `GET /health`
- **THEN** 回 `200`（無需 auth、無 payload 內容）；服務停止時連線失敗（nexus
  據此標 upstream unhealthy）
