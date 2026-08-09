# Graphify Plugin SDK — 技術選型與多語言支援規劃

> 狀態：規劃討論中（[待討論] 標記者尚未定案）
> 日期：2026-08-09
> 來源：用戶提供的 SDK 構想（口述/貼文）
> 註：本文為當時構想的逐字快照，保留原文。其中 `workspace_uuid` 的現行命名已改為 `workspace_key`（見 `docs/plugin-sdk-roadmap.md` 與 `docs/plugin_system.md`）。

## 動機

如果 Plugin SDK 限制只能用 Rust 開發，會大幅提高社群貢獻的門檻。為了兼顧 Rust 核心的極速（16ms）與多語言生態的擴充性，以下是針對 Graphify Plugin SDK 的技術選型與語言支援規劃。

## 1. 核心技術選型：基於 JSON-RPC / Stdio 的輕量化 IPC 或 WebAssembly (Wasm)

為了讓任何語言（TypeScript/JavaScript, Python, Go, Rust 等）都能輕鬆開發 Graphify Plugin，建議採用以下兩種選型架構之一（或組合）：

### 方案 A：Stdio + JSON-RPC / MCP Protocol（首選，最推薦）

**原理**：Graphify Core（Rust）作為 Host，以子程序（Subprocess）方式啟動 Plugin，雙方透過標準輸入輸出 (stdin/stdout) 傳遞 JSON-RPC 訊息（即 Model Context Protocol / MCP 原生模式）。

**優勢**：
- 0 語言門檻：任何語言只要能讀寫 stdin/stdout 並解析 JSON 即可寫 Plugin。
- 生態無縫對接：可以直接複用 Anthropic / OpenCode 社群現成的 MCP SDK（TypeScript SDK, Python SDK 等）。

**劣勢**：進程間通訊（IPC）有微秒級開銷，但對於 Review / Handoff / Vector 這種非 High-Frequency Loop 的操作，效能完全綽綽有余。

### 方案 B：WebAssembly (WASM via Extism / Wasmtime)

**原理**：Graphify 內嵌 wasmtime 或 Extism 執行引擎，Plugin 編譯成 .wasm 檔給 Graphify 載入。

**優勢**：沙盒安全、單一執行檔部署、跨平台、極速。

**劣勢**：Plugin 開發者需要將程式碼編譯成 Wasm（Rust, Go, C++, Zig 很適合，但 Python/JS 稍微繁瑣）。

## 2. 多語言 SDK 支援架構（Polyglot SDK Architecture）

採用「Protocol-First (協定優先)」的設計：Graphify Core 只定義 JSON Schema 與通訊協定，官方提供多語言薄包裝（Thin Wrappers）。

```
Plaintext
 ┌──────────────────────────────────────────────┐
 │ Graphify Core Engine (Rust)                  │
 └──────────────────────┬───────────────────────┘
                        │
       JSON-RPC via Stdio / MCP Protocol
                        │
 ┌──────────────────────┼────────────────────────────────────────┐
 ▼                      ▼                                        ▼
┌──────────────────┐ ┌──────────────────┐ ┌──────────────────┐
│ TypeScript SDK   │ │ Python SDK       │ │ Go SDK           │
│ `@graphify/sdk`  │ │ `graphify-sdk`   │ │ `graphify-go`    │
└────────┬─────────┘ └────────┬─────────┘ └────────┬─────────┘
         │                    │                    │
┌────────▼─────────┐ ┌────────▼─────────┐ ┌────────▼─────────┐
│ Node.js/Bun      │ │ Python Plugin    │ │ Go Plugin        │
│ Plugin           │ │ (OpenDoc Vector) │ │ (Code Review)    │
└──────────────────┘ └──────────────────┘ └──────────────────┘
```

### 各語言的守備場景與分工

**TypeScript / JavaScript (Node.js / Bun)**
- 適合開發：OpenCode Plugins、VS Code / Editor 整合、UI 互動介面。
- 優勢：Web/AI 社群開發者最多，與生態系無縫對接。

**Python**
- 適合開發：graphify-plugin-opendoc（Vector DB 檢索、LangChain/LlamaIndex 對接、Document Chunking、Data Science 解析）。
- 優勢：Heavy AI / ML / Vector SDK（Qdrant, PyMuPDF, Pandas）生態極度豐富。

**Go / Rust**
- 適合開發：高效能 CLI 擴充、Git Hook 審查工具、系統級運算模組。

## 3. Plugin 介面標準 (Standard Trait / Interface)

無論別的語言怎麼寫，只要實作以下 3 個核心 RPC 方法即可：

```typescript
// 任何語言的 SDK 實作合約 (TypeScript 範例)
export interface GraphifyPlugin {
  // 1. 模組初始化：接收 Graphify 傳來的 workspace_uuid 與核心設定
  initialize(context: { workspace_uuid: string; root_path: string }): Promise<PluginMetadata>;

  // 2. 核心 Hook：當代碼變更或 AST 重新解析時觸發
  onGraphUpdated?(event: { workspace_uuid: string; modified_nodes: string[] }): Promise<void>;

  // 3. MCP 工具暴露：提供給 AI Agent 呼叫的專屬能力
  getTools(): MCPToolDefinition[];
}
```

## 4. 總結技術選型建議

| 項目 | 建議技術選型 |
| --- | --- |
| 底層通訊 (IPC) | Stdio + JSON-RPC (MCP Native) |
| 官方優先提供 SDK | TypeScript / Node.js（主力生態） + Python（OpenDoc 向量生態） + Rust（原生極速） |
| Plugin 打包與分發 | 每個 Plugin 為獨立 CLI 命令或 Executable，在 graphify.toml 中聲明註冊 |

```toml
[plugins.opendoc]
command = "python -m graphify_opendoc"
```

這樣的選型既能保持 Graphify 本體極致純粹的 Rust 效能，又能讓別的語言開發者 10 分鐘寫出一個 Plugin，生態圈才能真正活起來！
