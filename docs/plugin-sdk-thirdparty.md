# Third-Party Plugin SDK — Architecture Proposal (Draft)

> Status: `draft` — 架構構想提案，基於已定案決策（D1: Stdio+JSON-RPC=MCP, D2: 獨立協議(MCP)+Adapter, D3: 自家內建/第三方走協議, D4: 事件廣播, Dual-Mode 定案）。未實作，待 OpenSpec proposal 批准。
> Last updated: 2026-08-09
> Related: [Decision Roadmap](plugin-sdk-roadmap.md) · [Plugin System](plugin_system.md) · [Dual-Mode MCP Architecture](ref/plugin-sdk-dual-mode-mcp.md)

## 0. Scope

**自家 plugin（first-party）**：以 v1 內建 trait（`graphify-core::plugin`）編譯整合進 graphify 本體，最嚴謹架構，不走 subprocess。

**第三方 plugin（third-party）**：本文件規範對象。以獨立 CLI/executable 存在，透過 Stdio + JSON-RPC 與 Graphify Host 通訊。官方 SDK 為薄包裝，協議本身是唯一契約。

---

## 1. D6 — 註冊與分發（Recommendation）

### D6a. TOML 宣告 schema

完整 argv 形式（與 MCP server 設定慣例對齊），支援 command + args + env 覆寫 + cwd：

```toml
[plugins.opendoc]
command = ["python", "-m", "graphify_opendoc"]
env = { GRAPHIFY_DEBUG = "0" }        # optional, merged over host env
cwd = "."                             # optional, default = workspace root
timeout_ms = 30000                    # optional, per-call timeout
```

### D6b. 生命週期：lazy spawn on first use

graphify 是 CLI 非 daemon；plugin 只在被實際呼叫時 spawn，閒置超時（建議預設 5 min，可設定）後 terminate。理由：零常駐資源、CLI 語意一致、社群 plugin 不必為「永遠在跑」設計。

### D6c. 分發方式：裸 command 為主

- Host 不綁定任何套件管理器（npm/PyPI/crates.io 皆可），`command` 只要能執行即可。
- 生態慣例由各語言自行演化（`npx`、`uvx`、`cargo install` 皆可作為 command 的一部分）。
- 官方 SDK 套件名保留命名空間：`@graphify/sdk`（TS）、`graphify-sdk`（Python）、`graphify-go`。

### D6d. 設定位置

`graphify.toml` 全域設定 + 各 workspace 可覆寫（優先權：workspace < user < system，對齊 XDG 慣例）。第三方 plugin 一律宣告在 plugins 區塊，不混入自家內建 plugin（後者由 binary 內建，無需宣告）。

### D6e. 協議版本化

JSON-RPC 協議附 `protocolVersion`（semver），`initialize` 時協商。Host 支援範圍：≥ 最小版本、< 下一個 major。不符合則明確拒絕並回報錯誤。

---

## 2. Protocol — MCP 標準 + Graphify Extensions（收斂後）

> **收斂決策（2026-08-09）**：第三方 plugin 的溝通介面即 **MCP 本身**，不發明自訂協議。
> MCP 的 Stdio transport 即 JSON-RPC 2.0；`tools/list` / `tools/call` 即工具介面；TS/Python/Go/PHP SDK 生態現成。
> 本協議 = MCP 標準 + **2 個 Graphify extension**（§2.4、§2.5）。以 OpenSpec change 追蹤，不視為 frozen。

### 2.1 Transport

- **MCP Stdio transport**（JSON-RPC 2.0 over Stdio）；**只有 stdout 承載協議訊息**，plugin 日誌一律走 stderr（避免 stdout 汙染）。
- Framing：MCP 慣例的 `Content-Length` header（LSP 風格），非 newline-delimited（訊息可含任意 JSON）。
- 方向：Host → plugin 為 request/notification；plugin → Host 為 response 與 plugin 發起的 notification。

### 2.2 MCP 標準 Methods（複用，不改寫）

| Method | Direction | Description |
|---|---|---|
| `initialize` | Host → Plugin (request) | 啟動握手，協商 `protocolVersion` 與 capabilities。Graphify 的 workspace 上下文（§2.4）在此帶入。 |
| `notifications/initialized` | Host → Plugin (notification) | 握手完成，plugin 可開始工作。 |
| `tools/list` | Host → Plugin (request) | 回傳 `Tool[]`（MCP 標準 schema，§2.3），供 AI Agent 呼叫。 |
| `tools/call` | Host → Plugin (request) | Agent 對 plugin 工具的實際呼叫。 |
| `notifications/log` | Plugin → Host (notification) | plugin 主動回報，可轉發給 Agent/CLI。 |
| `shutdown` / `exit` | Host → Plugin | 優雅關閉 / 強制終止（MCP 標準生命週期）。 |

### 2.3 ToolDefinition Schema（MCP 標準）

```json
{
  "name": "opendoc_search",
  "description": "Search documents in the workspace vector store",
  "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } } }
}
```

### 2.4 Graphify Extension — Workspace Context

- `workspace_key` 是 plugin 與 graphify/opendoc-mcp 之間的路由金鑰（#3086），於 `initialize` 時以自訂 clientInfo 欄位傳遞：

```json
{
  "jsonrpc": "2.0", "id": 1, "method": "initialize",
  "params": {
    "protocolVersion": "2025-06-18",
    "capabilities": {},
    "clientInfo": { "name": "graphify", "version": "0.x" },
    "graphify": { "workspace_key": "<uuid>", "root_path": "/workspace/root" }
  }
}
```

### 2.5 Graphify Extension — `notifications/graph_updated`

- MCP 標準沒有 client→server 的圖更新通知，此為 graphify 唯一新增 notification：

```json
{
  "jsonrpc": "2.0", "method": "notifications/graph_updated",
  "params": { "workspace_key": "<uuid>", "modified_nodes": ["node1", "node2"], "event": "indexed" }
}
```

- `event` 值：`"indexed" | "extracted" | "manual"`（與 `plugin-events-v1` change 的 `GraphUpdateKind` 一致）。
- 觸發時機（D4 定案 D）：`graphify index` / `extract` 完成後廣播 + `graphify plugin run-hooks` 手動觸發。

### 2.6 Error Contract

- 標準 JSON-RPC error codes：`-32700` parse, `-32600` invalid request, `-32601` method not found, `-32602` invalid params, `-32603` internal。
- Plugin 自訂錯誤：`-32000` 起，附 `data: { code, message }`。
- 非零 exit code 或 stdout 亂碼 → Host 視為 crash，記錄 stderr 內容並以明確錯誤回報給呼叫者（不吞錯）。

---

## 3. D7 — Host 職責範圍（Recommendation）

### D7a. Registry 位置：`graphify-mcp` 內，不進 `graphify-core`

- 理由：`graphify-core` 零新依賴是硬約束（#3088）；plugin host 邏輯（spawn、framing、lifecycle、registry）與 extraction 無關。
- 目前由 `graphify-mcp` 管理（`graphify-mcp/src/plugin_host/`，含 framing、process、host 模組），遵守 300-line 限制拆檔。
- **YAGNI**：暫不抽 `graphify-plugin-host` 獨立 crate；待第二個需要 host 的 consumer 出現時再抽。

### D7b. 第三方暴露：Dual-Mode MCP 架構（已定案）

> **Dual-Mode 定案（2026-08-09）**：第三方 plugin 的對外暴露同時支援兩種模式，由 agent 端選擇（詳見 [Dual-Mode MCP Architecture](ref/plugin-sdk-dual-mode-mcp.md)）。

- **Mode 2 (Direct / Multi-Server)**：第三方 plugin 各自是獨立 MCP server，agent 直接連多個 server。**零額外實作**——MCP 原生多 server 模式。
- **Mode 1 (Unified Gateway)**：`graphify-mcp` 兼 MCP client，spawn 第三方 plugin、`tools/list` 彙整（tool 名加前綴 `graphify_plugin_<plugin_id>_<tool>`）、`tools/call` 轉發；`.toon` 拓撲注入仍為 opt-in。plugin 掃描與基本聚合已由 `plugin-scan-v1` 提供。
- 自家 plugin 直接用 v1 trait（最嚴謹）；`MCPPluginAdapter`（把外部 plugin 以 v1 trait 暴露給 Core）暫不實作——無 consumer。
- Mode 1 現行實作使用手寫、零新增依賴的 Content-Length framing；request id、notification 路由與 plugin crash/restart 的完整生命週期仍屬後續範圍。

---

## 4. D5 — SDK 語言順序（願景，暫緩實作）

1. **TypeScript (`@graphify/sdk`)** — 最大社群、OpenCode/editor 生態；協議薄包裝 + type definitions。
2. **Python (`graphify-sdk`)** — opendoc 向量生態（Qdrant/PyMuPDF/LangChain 對接）。
3. **PHP (`php-graphify-sdk`)** — 以 Composer 管理（`composer require graphify/graphify-sdk`）；plugin 經 `composer.json` 宣告，入口為 PHP CLI。照顧 PHP 生態（Laravel/WordPress 開發者）。
4. **Rust (`graphify-plugin-sdk`)** — 高階 plugin 開發、adapter 參考實作。
5. **Go (`graphify-go`)** — 高效能 CLI 擴充 / git-hook 工具。

> 各 SDK = 薄包裝（framing + method stubs + type），不重實作協議邏輯。

---

## 5. [待討論] 殘留項目

- [待討論] **Community-facing 變更彈性**：D6 宣告格式與 §2 協議 method 表為「構想」，社群推廣後可能調整 — 以 OpenSpec change 追蹤，不視為 frozen。
- [待討論] Plugin 沙盒/權限模型（第三方 plugin 可讀哪些路徑、可否碰 network）— 是否進 v1 範圍。
- [待討論] 第三方 plugin 的 discover/install 流程（是否提供 `graphify plugin add <name>` 自動寫入 TOML）。
