# Proposal: RustTuiKit — 通用 Rust TUI 範本層（獨立 repo）

## Why

graphify 的 TUI 基礎元件（theme 色盤、版面分割、modal 骨架、事件日誌、footer pills）零 graphify 語意，卻綁在 `graphify-cli/src/ui/*` 內，其他 Rust TUI 專案無法重用。目標：建立所有專案可引入的 tui 範本層，落點為獨立 repo（方案 A 已定案，2026-08-09）：

- repo：`../RustTuiKit`，remote `git@gitlab.com:saaslab/rust-tuikit.git`（crate 名 `rust-tuikit`）
- 依賴方向單向：`rust-tuikit → ratatui 0.28`（v0.1 不依賴 crossterm）；`GraphifyRust → ../RustTuiKit`（path dep，比照 GraphifyPlugins 慣例）
- 調研存檔：`docs/ref/tui-crate-research.md`

## What Changes

**新 repo `../RustTuiKit`（通用層，零 graphify 依賴）：**

| 模組 | 內容 | 抽自 |
|---|---|---|
| `theme` | Catppuccin/Tokio Night 色盤常數（BG…BLUE 共 10 色） | `ui/theme.rs`（14 行全抽） |
| `layout` | `Chrome` + `split(area, log_ratio)`：tabs/main/log/footer 版面，event log 比例參數化（graphify 用 30/100） | `ui/layout.rs:12-47` |
| `log` | `LogEntry`、`push_log`（上限 200）、`render_event_log` | `ui/layout.rs:49-75, 210-238` |
| `modal` | `ModalItem`、`centered_rect`、`render_list_modal`（title/hint/items/footer-count hover 列表骨架，回傳內部 Rect 供滑鼠命中測試） | `ui/modal.rs:12-26, 99-119, 521-600` |
| `flash` | `Flash<K>`（泛型 action 閃爍：trigger/is_active/tick，220ms）、`Pill<K>`、`render_pills`（footer 藥丸按鈕） | `ui/layout.rs:77-208`（ActionTag 枚舉不抽，屬 graphify） |
| `component` | 最小 `Component` trait（render + handle_key/handle_mouse 預設 no-op） | ratatui component 模式（調研 §3） |

**GraphifyRust 整合（行為不變）：**

- `graphify-cli` 加 path dep `rust-tuikit = { path = "../../RustTuiKit" }`。
- `ui/theme.rs`、`ui/layout.rs`、`ui/modal.rs` 通用部分改為 re-export，呼叫點不改；graphify 專屬部分（`draw_plugin_panel`/`draw_workspace_selector`/compose menu+diagram/canvas/ActiveTab/ActionTag）留在原地，改呼叫 kit 的 `render_list_modal` 與 `render_pills`。
- 淨刪 graphify-cli 約 250 行，重複定義歸零。

## Capabilities

### New: rust-tuikit

- 見 `specs/rust-tuikit/spec.md`：模組組成、渲染行為binding 測試、lint 規範。

### Modified: 無既有 capability 行為變更

- graphify-cli TUI 渲染輸出位元級一致（ratatui TestBackend 斷言 + 既有互動流程不變）。

## Impact

- 新 repo：~8 檔、~300 行（含測試）；獨立 git init + remote `rust-tuikit`。
- `graphify-cli/Cargo.toml` +1 行 path dep；`ui/*` 淨刪 ~250 行 + re-export 呼叫點不變。
- 版本不動：ratatui 0.28 / crossterm 0.28（graphify-cli 側）。
- #3216 執行順序不變（P1→P5）；TUI Stage 1/2 未來以 rust-tuikit 為宿主。
- lint：kit repo 複製 GraphifyRust workspace profile（unsafe forbid、pedantic/nursery deny、unwrap/expect deny）。
