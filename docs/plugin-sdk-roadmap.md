# Plugin SDK — Decision Points & Roadmap

> Status: `draft` — 部分決策已收斂，未定案者標記 [待討論]
> Last updated: 2026-08-09
> Related: [Plugin System](plugin_system.md) · [Polyglot SDK Plan (verbatim)](ref/plugin-sdk-polyglot-plan.md) · [Dual-Mode MCP Architecture (verbatim)](ref/plugin-sdk-dual-mode-mcp.md) · [OpenSpec change `plugin-trait-v1`](../openspec/changes/plugin-trait-v1/proposal.md)

## 0. Purpose

This document collects every open decision for the polyglot Graphify Plugin SDK, **with options only — no decisions made yet**. Each decision point lists candidate options, their trade-offs, and a tentative recommendation marked `[待討論]`. Decisions are converged one at a time in discussion order (see §9).

## 1. Background: Two Layers

The plugin story has **two distinct layers** that must not be conflated:

| | Layer 1 — Embedded trait (v1, shipped) | Layer 2 — External SDK (this doc) |
|---|---|---|
| Location | In-process, inside a Rust crate | Subprocess spawned by Graphify Core |
| Language | Rust only | Any language |
| Transport | Direct method calls | Stdio + JSON-RPC (MCP-native) |
| Use case | Core-internal extension (e.g. handoff) | Community third-party plugins |
| Status | `implemented` (commit `36fa259`) | `[待討論]` |

They can coexist: **Layer 1 = internal extension point, Layer 2 = external ecosystem interface.** Open questions remain about how the two relate (§3).

## 2. Decision D1 — IPC Transport

**Question:** How does Graphify Core (host) talk to an external plugin?

### Option A: Stdio + JSON-RPC / MCP-native (recommended in source plan)
- **Mechanism:** Host spawns plugin as subprocess; JSON-RPC messages over stdin/stdout (MCP native mode).
- **Pros:** Zero language barrier (any language with stdin/stdout + JSON parsing works); reuse existing MCP SDKs (TypeScript, Python) from the Anthropic/OpenCode community.
- **Cons:** Sub-microsecond IPC overhead; irrelevant for Review/Handoff/Vector (non-hot-loop) operations.

### Option B: WebAssembly (Wasmtime / Extism embedded)
- **Mechanism:** Graphify embeds a WASM runtime; plugin compiles to `.wasm`.
- **Pros:** Sandbox safety, single-binary deploy, cross-platform, fast.
- **Cons:** Contributors must compile to Wasm (natural for Rust/Go/C++/Zig, awkward for Python/JS); heavy runtime dependency — conflicts with constraint #3088 (zero new core deps) and is a major `graphify-core` change.

### Option C: Hybrid (A now, B later)
- **Mechanism:** Ship Stdio+JSON-RPC first; keep WASM as a roadmap item if sandbox/perf requirements materialize.
- **Pros:** Fastest to ecosystem value; defer the heavy dependency until justified.
- **Cons:** Two transports to maintain eventually.

**✅ DECIDED (2026-08-09): C — pure A for v1, WASM deferred.** Stdio + JSON-RPC (MCP-native) 為 v1 唯一傳輸，WASM (Extism/Wasmtime) 留作 roadmap 項目，待沙盒或效能需求明確時再評估（避免違反 #3088 零新依賴原則）。

## 3. Decision D2 — Method Contract vs the v1 Trait

**Question:** The source plan defines 3 RPC methods (`initialize` / `onGraphUpdated` / `getTools`); the shipped v1 trait has 4 methods (`get_id` / `bind` / `get_workspace_key` / `sync_toon`). How do they relate?

### Option A: Independent protocol (recommended)
Define the SDK protocol on its own terms, **not** as a serialized mirror of the Rust trait. External lifecycle (spawn/register/teardown, error codes, stdout pollution handling) differs fundamentally from in-process calls.
- **Pros:** Clean protocol-first contract; each side evolves independently.
- **Cons:** Two contracts to keep in sync conceptually.

### Option B: Mirror the v1 trait 1:1
- **Pros:** Single mental model.
- **Cons:** Forces external plugins to mimic in-process semantics; awkward for languages without trait-like concepts.

### Option C: Adapter bridge
Keep the v1 trait; add a `MCPPluginAdapter` in Rust that wraps the external protocol as an in-process trait implementation, so internal and external plugins share a common facade.
- **Pros:** Both layers under one Rust interface; internal plugins stay cheap, external plugins plug into the same slot.
- **Cons:** Adapter complexity; v1 trait may need extension to express async/events.

**✅ DECIDED (2026-08-09): A + C combined — protocol-first, with an adapter if unification is wanted.** SDK 協議獨立定義（非 Rust trait 的序列化鏡像）；如需要統一介面，Rust 側另建 `MCPPluginAdapter` 包住外部協議。

## 4. Decision D3 — MCP Server Integration Mode

**Question:** Graphify already ships `graphify-mcp` (an MCP server exposing graph/summary/path tools). Plugins expose their own tools via `getTools`. How do plugin tools surface to AI agents?

### Option A: Merge into a single graphify-mcp server
Plugin tool definitions register into the existing server; one MCP endpoint for everything.
- **Pros:** One config, one connection, simple agent UX.
- **Cons:** Requires graphify-mcp to load/forward plugin tools; plugin lifecycle (crash/restart) impacts the shared server.

### Option B: Each plugin runs its own MCP server (MCP multi-server mode)
The agent connects to N servers (graphify-mcp + one per plugin).
- **Pros:** Isolation; matches MCP's native multi-server model; plugins are plain MCP servers.
- **Cons:** Agent-side config/onboarding burden grows per plugin.

### Option C: Configurable per plugin
`[plugins.x]` decides: `mode = "merged" | "standalone"`.
- **Pros:** Flexibility; opt-in per plugin.
- **Cons:** Two code paths to maintain.

**✅ DECIDED (2026-08-09): 自家 plugin 內建、第三方走 SDK 協議。** 自家開發的 plugin（opendoc / review / handoff）以內建 trait 編譯整合進 graphify 本體（省 IPC 開銷、單一二進位、與 Core 同程序共享 `.toon`）；外部 SDK 協議（Stdio + JSON-RPC）只服務第三方 plugin。兩層各自乾淨，不強制自家 plugin 走 subprocess。

### D3 延伸：Dual-Mode MCP Architecture（第三方 plugin 暴露方式）

第三方 plugin 對 agent 的暴露方式採「可切換雙模式」（構想全文：`docs/ref/plugin-sdk-dual-mode-mcp.md`）：

- **Mode 1 — Unified Gateway**：graphify-mcp 兼 MCP client，spawn 第三方 plugin 子程序，`tools/list` 彙整（tool 前綴 `graphify_<plugin>_<tool>`）、`tools/call` 轉發、可注入 `.toon` 拓撲資訊。Agent 只需連單一 server。
- **Mode 2 — Multi-Server / Direct**：第三方 plugin 各自是獨立 MCP server，agent 直接連 N 個 server。graphify-mcp 保持純 server。零額外實作（MCP 原生模式）。

- **[待討論] 定案範圍**：Mode 2 立即成立（零成本）；**Mode 1 已定案為確定要做的項目**（2026-08-09，用戶確認）——graphify-mcp gateway client 排程在 `plugin-events-v1` 之後實作。`.toon` 注入為 opt-in；tool 前綴命名策略（`graphify_<plugin>_<tool>`）實作時定。
- 實作評估（orchestrator, 2026-08-09）：Mode 1 僅需 3 件事——`tokio::process::Command` spawn + stdio 接管、JSON-RPC 2.0 line-delimited framing + `tools/list` 彙整、`tools/call` 前綴剝離轉發。Rust 側可選 `mcp-rs`（client/server 雙角色，成熟度中等）或手寫輕量 framing（零依賴）。真正複雜點：並發 request id 管理、notification 路由、plugin crash/restart 生命週期。

## 5. Decision D4 — `onGraphUpdated` Trigger Mechanism

**Question:** The plan says the hook fires "on code change or AST re-parse". Core is currently pull-based (`index` / `extract` commands); there is no resident daemon watching the filesystem.

### Option A: Resident daemon in Core
Core watches files and pushes events continuously.
- **Pros:** True event-driven UX.
- **Cons:** Major architecture change; memory/perf cost; conflicts with "pure Rust, minimal" stance.

### Option B: Broadcast after `graphify index` / `extract` completes (recommended)
The CLI emits an update event (to registered plugins) at the end of each index/extract run.
- **Pros:** Minimal change; fits existing pull-based model; plugins stay reactive to real re-parses.
- **Cons:** No events for ad-hoc file edits outside index runs.

### Option C: Manual trigger
A `graphify plugin run-hooks` command fires hooks on demand.
- **Pros:** Simplest; fully user-controlled.
- **Cons:** Poor DX; easy to forget.

### Option D: B + C combined
Broadcast after index/extract, plus manual trigger for scripts/CI.
- **Pros:** Covers automation and interactivity; small incremental cost over B.

**✅ DECIDED (2026-08-09): D — B + C combined.** index/extract 執行完成後廣播更新事件，加上 `graphify plugin run-hooks` 手動觸發供 scripts/CI 使用。

## 6. Decision D5 — Multi-language SDK Priority Order

**Question:** Which official SDKs ship first?

- **TypeScript / Node.js (Bun)** — target: OpenCode plugins, editor integrations, UI. Largest Web/AI community; ecosystem-native. (Source plan: 主力生態)
- **Python** — target: `graphify-plugin-opendoc` (vector retrieval, LangChain/LlamaIndex, document chunking). Rich AI/ML/vector stack (Qdrant, PyMuPDF, Pandas).
- **Rust** — native speed; internal plugins; adapter reference implementation.
- **PHP** — PHP plugin SDK（composer 管理，`php-graphify-plugin`）。`composer.json` `require` 項目：`graphify/graphify-sdk`。插件開發者透過 `composer require graphify/graphify-sdk` 下載基礎 SDK，再透過 `php-graphify-plugin` 組件（`composer.json` 內的 `autoload` + `composer.json`）管理。

D5：PHP 放在 Python 之後（PHP 插件生態相對較新，適合以 composer 打包的 PHP SDK）。

**⏸ DEFERRED (2026-08-09): 暫緩 — 屬構想提出階段，待 SDK 核心（D1–D4、D6）落地後再安排。** 先保留為官方 SDK 願景清單，不排入近期實作。

## 7. Decision D6 — Plugin Packaging & Registration Format

**Question:** How is a plugin declared, distributed, and launched?

Source plan proposes: each plugin is an independent CLI command/executable, registered in `graphify.toml`:

```toml
[plugins.opendoc]
command = "python -m graphify_opendoc"
```

Open sub-questions:
- **[待討論] D6a:** Schema shape — `command` string only, or `command + args + env + cwd`? (Recommend full argv form for parity with MCP server configs.)
- **[待討論] D6b:** Lifecycle — lazy spawn on first use, or eager spawn at `graphify` startup?
- **[待討論] D6c:** Distribution — bare commands, crates.io/npm/PyPI packages, or prebuilt binaries (e.g. GitHub releases)?
- **[待討論] D6d:** Config location — `graphify.toml` vs per-workspace `.graphify/plugins.toml`?
- **[待討論] D6e:** Version/compat — protocol versioning (semver on the JSON-RPC schema)?

## 8. Decision D7 — SDK-v1 Trait Coexistence & Core Scope

**Question:** What belongs in Core vs the SDK layer?

- **[待討論] D7a:** Does `graphify-core` gain a plugin *registry* (discovery, lifecycle, event routing), or does that live in `graphify-cli`/`graphify-mcp` to keep Core dependency-free (per #3088)?
  - *Lean: registry + protocol types in a new crate or `graphify-cli`; Core keeps zero new deps.*
- **[待討論] D7b:** Does the v1 in-process trait remain Rust-only, or does an `MCPPluginAdapter` (D2-C) surface external plugins through it?
  - *Update (2026-08-09): Dual-Mode Mode 1 即此角色的具體形態——graphify-mcp 兼 MCP client 代理第三方 plugin。待 Mode 1 定案後決定實作層級（mcp-rs vs 手寫 framing）。*

## 9. Discussion Order (to be arranged)

已定案：D1 (C)、D2 (A+C)、D3 (自家內建 / 第三方走協議)、D4 (D)。D5 暫緩。剩餘待議：

1. **D6** — packaging/registration format（第三方 plugin 如何宣告、分發、啟動）
2. **D7** — core scope / coexistence（registry 放哪、v1 trait 與 SDK 協議的界線）
