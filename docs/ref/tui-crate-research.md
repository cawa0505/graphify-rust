# tui crate 抽取調研（docs/ref/ verbatim 快照）

> 用途：評估將 graphify 的 TUI 抽成獨立 crate（工作名 `graphify-tui`），使其他 Rust TUI 可重用 tui 元件。
> 本文件為調研原始結果存檔；openspec 提案見 `openspec/changes/tui-crate-extraction/`。

---

## 1. ratatui/templates（cargo-generate）README

來源：https://github.com/ratatui/templates （MIT，422★ / 50⎇ / 111 commits）

Repository contains templates for bootstrapping a Rust TUI application with `Ratatui` and `crossterm`.

### Getting Started

1. Install cargo-generate: `cargo install cargo-generate`
2. Create a new app based on this repository: `cargo generate ratatui/templates`
3. Choose one of the following templates:
   - **Hello World**: A "Hello, World!" example.
   - **Simple** | **Simple Async**: A simple example.
   - **Event Driven** | **Event Driven Async**: An example of an event-driven TUI application.
   - **Component**: An example of a component-based TUI application.

### 目錄結構（6 組模板 + 對應 -generated 快照）

`hello-world` `simple` `simple-async` `event-driven` `event-driven-async` `component`
附 `cargo-generate.toml` + `justfile`（`just generate-all` 保持 -generated 快照同步）。

---

## 2. ratatui.rs/showcase — Third Party Widgets（13 crates）

| Crate | 一句話定位 | 與本計畫的關聯 |
|---|---|---|
| `ratatui-image` | 圖片 widget，多圖形協定後端（sixel/iTerm2/kitty） | 非必要 |
| `ratatui-textarea` | 類 HTML `<textarea>` 多行編輯器 | 非必要 |
| `throbber-widgets-tui` | 活動指示器（spinner） | 非必要 |
| `tui-big-text` | font8x8 像素大字 | 非必要 |
| `tui-checkbox` | 客製樣式勾選框 | 非必要 |
| `tui-logger` | 捕捉並顯示 logs | 非必要 |
| `tui-menu` | 可巢狀選單 | 與 WorkspaceSelector/menu 層可比對 |
| `tui-nodes` | **Node graph 視覺化 widget** | 與 canvas.rs 同類，抽取時可比對設計 |
| `tui-piechart` | 餅圖（標準/高解析） | 非必要 |
| `tui-scrollview` | 可捲動視圖 | canvas 超出畫面捲動可比對 |
| `tui-term` | 偽終端 widget | 非必要 |
| `tui-tree-widget` | 樹狀資料 widget | 非必要 |
| `tui-widget-list` | stateful widget list（`Listable` trait） | component list 模式可比對 |

官方同節點另列 **ratcn**（/ecosystem/ratcn/）：themeable, customizable components for Ratatui，是 component 生態中最接近「主題化元件庫」的參考。

---

## 3. ratatui.rs/templates/component — Component 模式全文

### Features

- Uses **tokio** for async events
- Start and stop key events to shell out to another TUI like vim
- Supports suspend signal hooks
- Logs using tracing
- better-panic / color-eyre / human-panic
- clap for command line argument parsing
- **`Component` trait** with `Home` and `Fps` components as examples

### Usage

```
cargo generate --git https://github.com/ratatui/templates component --name ratatui-hello-world
```

### Background（設計動機，接近 verbatim）

- `ratatui` is based on the principle of **immediate rendering with intermediate buffers**. At each new frame you have to build all widgets that are supposed to be part of the UI. The `ratatui` library largely handles just drawing to the terminal.
- Additionally, the library **does not provide any input handling nor any event system**. The responsibility of getting keyboard input events, modifying the state of your application based on those events and figuring out which widgets best reflect the view is on you.
- Since `ratatui` is an immediate mode rendering based library, there are _multiple_ ways to organize your code, and there's no real "right" answer. Choose whatever works best for you!
- 組織方式光譜：`ratatui/examples` → `simple` → `simple-async` → json-editor tutorial → **`component`**（Tokio + Crossterm + "Components"）→ `tui-realm`（framework with reusable components with properties and states）。

Note: 可另查 ratcn（themeable component library）作為替代起點。

---

## 4. 本地盤點（現有 TUI 程式碼）

| 檔案 | 行數 | 內容 | 耦合點 |
|---|---|---|---|
| `graphify-cli/src/tui.rs` | 1901 | App shell、ModalState、鍵盤/滑鼠事件、compose 面板 | `graphify_core::{GraphOutput, Node}`、`build_graph`/`query_bfs`/`from_toon`、compose_{manifest,merge,manifest}、`graphify_registry::registry_db_path` |
| `graphify-cli/src/ui/canvas.rs` | 315 | 粗粒度 ASCII 圖 | `graphify_core::{GraphOutput, Node, NodeId}` |
| `graphify-cli/src/ui/modal.rs` | 600 | Modal 面板渲染 | `graphify_registry::db::{PluginRegistrationRow, PluginStatus, WorkspaceRow}` |
| `graphify-cli/src/ui/layout.rs` | 300 行內 | 版面分割 | 無（純 ratatui） |
| `graphify-cli/src/ui/theme.rs` | 14 | 主題 | 無（純 ratatui） |
| `graphify-cli/src/ui/mod.rs` | 12 | 匯出 | — |

版本：`ratatui = "0.28"`、`crossterm = "0.28"`（graphify-cli/Cargo.toml 27–28）。事件迴圈為同步 crossterm poll，無 tokio。

---

## 5. 抽取結論（供 openspec 提案使用）

1. **模板取向**：component 模板（Component trait + 分層）最符合「獨立 crate 供重用」目標，但 graphify 不需要 tokio（現有同步事件迴圈夠用，行為不變優先）。
2. **分層切法**（元件可重用的關鍵）：
   - `graphify_tui::components`：**純 ratatui 層**（theme、layout、modal 骨架、selector）— 只依賴 ratatui/crossterm，零 graphify 依賴 → 其他 Rust TUI 可直接重用。
   - `graphify_tui::app`：**graphify 層**（canvas 渲染 GraphOutput、inspector、compose 面板、registry workspace/plugin 資料）— 允許 dep graphify-core/graphify-registry。
3. **版本不動**：維持 ratatui 0.28 / crossterm 0.28（行為不變、零新依賴）。
4. **重用匯出**：`graphify_tui::components` 以 pub API 匯出（Theme、Layout、Modal、Selector、Component trait）；`tui-nodes`/`tui-scrollview` 等 third-party crate 僅作設計比對，不引入。
5. **library code 禁 unwrap/expect**（#3149）、object-safe trait。
6. **對既定路線的影響**：#3216 TUI Stage 1（P3）/Stage 2（P5）未來將以 `graphify-tui` crate 為宿主；本次抽取不改變 P1/P2 順序。

---

Note (2026-08-09)：本文件為調研快照。第三方 crates 版本與 ratatui 官網內容以原文為準，行文未經改寫歷史。
