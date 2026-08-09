# Dual-Mode MCP Architecture (可切換模式架構構想)

> Verbatim draft from discussion, 2026-08-09. Status: `[待討論]` — 未定案，待整合進 `docs/plugin-sdk-roadmap.md` 後此檔轉為參考文件。
> Related: [Plugin SDK Roadmap](../plugin-sdk-roadmap.md) · [Polyglot SDK Plan (verbatim)](plugin-sdk-polyglot-plan.md)
> 註：本文為構想快照，保留原文。其中 `workspace_uuid` 的現行命名已改為 `workspace_key`（見 `docs/plugin-sdk-roadmap.md`）。

## 原始構想（逐字）

🏛️ 「可切換模式」的靈活架構設計 (Dual-Mode MCP Architecture)

為了同時支援這兩種使用方式，graphify-mcp 的核心可以設計為「微核心路由 (Micro-Router)」，並透過設定檔或參數輕鬆切換：

```
 ┌─────────────────────────────────────────────────────────┐
 │ AI Agent (Cursor / OpenCode)                            │
 └────────────────────────────┬────────────────────────────┘
                              │
     ┌────────────────────────┴────────────────────────┐
     │ Mode 1: Unified Gateway (B)  │ Mode 2: Multi-Server / Direct (A)
     ▼                             ▼
 ┌──────────────────────────────┐  ┌──────────────────────────────┐
 │ `graphify-mcp` (Gateway Pipe)│  │ `graphify-mcp` (Core Server) │
 │ - Host all tools             │  ├──────────────────────────────┤
 │ - Manage Sub-plugins         │  │ Third-Party Plugin A Server  │
 │ - Auto-enrich with `.toon`   │  ├──────────────────────────────┤
 └──────────────┬───────────────┘  │ Third-Party Plugin B Server  │
                │ Spawn / Stdio Pipe └──────────────────────────────┘
                ▼
 ┌──────────────────────────────┐
 │ Plugin Sub-processes         │
 │ (Python / TS / Go / Rust)    │
 └──────────────────────────────┘
```

兩種模式的動態選擇（Developer Choice）：

**極致體驗者（選擇 Mode B - Gateway）：**

Agent 設定檔只需要設定單一 MCP Server (graphify-mcp)。

開發者在 graphify.toml 裡面定義它掛載了哪些外掛（例如 opendoc, custom-review）：

```toml
[mcp.gateway]
enabled = true

[mcp.plugins.opendoc]
command = "python"
args = ["-m", "graphify_opendoc"]

[mcp.plugins.custom_tool]
command = "node"
args = ["/path/to/custom_tool.js"]
```

graphify-mcp 在啟動時會以 Subprocess 方式把這些 Tool 註冊進來，並負責轉發。Agent 的 context 視窗乾淨俐落，且所有 Output 都能經過 Graphify 的 `.toon` 增強！

**微服務 / 獨立運作者（選擇 Mode A - Independent）：**

開發者如果不想經過 Gateway，直接把 graphify-mcp、opendoc-mcp 當成獨立的 2 個 MCP Servers 寫進 Cursor / OpenCode 的 config。

Graphify 依然正常提供 16ms 靜態 AST 服務，完全不影響運作！

## 💡 實作 B (Gateway Client) 的 Rust 輕量解法

如果 Rust 現有的 MCP Client 生態還在發展中，你其實不需要依賴重型的全功能 MCP Client 庫，因為 MCP 的核心底層就是標準的 Line-delimited JSON-RPC 2.0 via Stdio！

要讓 graphify-mcp 作為 Gateway，只要做以下 3 件極輕量的事：

1. **Subprocess Spawn & Pipe (進程管理)：**
   使用 Rust 的 `tokio::process::Command` 啟動插件，並接管子進程的 stdin / stdout。

2. **Tool Aggregation & Prefix (工具彙整)：**
   啟動時發射 `tools/list` 給子進程，取得插件暴露的 Tools，自動幫它們加上前綴（例如 `graphify_opendoc_search`），並合併成一份完整的 Tool List 報給 Agent。

3. **Message Forwarding & Context Injection (訊息轉發與圖譜增強)：**
   當 Agent 呼叫 `graphify_opendoc_search` 時，graphify-mcp 剝掉前綴轉發給子進程，拿到結果後，可選擇將 Graphify 的 workspace_uuid 拓撲資訊注入回傳內容，再吐還給 Agent。

這樣的設計不僅掌握了「讓 Agent 乾淨、簡單」的極致體驗，更讓全語言生態（TS / Python / Go）的開發者有 100% 的自由選擇權！這絕對會是生態設計上最成熟、最靈活的形態。

---

## 評估摘要（orchestrator, 2026-08-09）

- 此構想與已定案決策相容：Mode 2 = MCP 原生多 server（D3 Option B）；Mode 1 = D3 Option A merged + D7b `MCPPluginAdapter` 角色。
- Mode 2 零額外實作（現有 graphify-mcp 已是獨立 server）。
- Mode 1 需 graphify-mcp 兼 MCP **client** 角色：spawn + stdio framing + tools/list 彙整（前綴）+ tools/call 轉發 + `.toon` 注入。
- 研究佐證（@librarian, 2026-08-09）：MCP 多 server / gateway 模式生態存在；Rust 側 `mcp-rs` 支援 client/server 雙角色（成熟度中等）；亦可手寫輕量 framing（line-delimited JSON-RPC 2.0 via Stdio）符合零依賴原則。
- **待決策點**：Mode 1 是否排入 roadmap；前綴命名策略（`graphify_<plugin>_<tool>`）；`.toon` 注入是 opt-in 還是預設。
