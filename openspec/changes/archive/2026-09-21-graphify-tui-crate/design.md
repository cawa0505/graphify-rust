# Design: graphify-tui-crate

## Context

Phase 2 延續 rust-tuikit-extraction：將 graphify 專屬 TUI 層（App shell、canvas、專屬 modal、ui/ 五檔）從 graphify-cli 抽成 workspace crate `graphify-tui`，使 graphify-cli 變薄殼、TUI 邏輯可獨立測試與重用。

## Goals / Non-Goals

- Goals：tui.rs + ui/ 隨碼遷移（git rename 零 diff）；graphify-cli 移除 ratatui/crossterm 依賴；依賴單向 `graphify-tui → {graphify-core, graphify-registry, rust-tuikit}`
- Non-Goals：不改變任何 TUI 行為與互動；不動 rust-tuikit 通用層

## Decisions

| # | 決策 | 理由 | Alternatives considered |
|---|---|---|---|
| D1 | `graphify-tui` 進 workspace members（非獨立 repo） | 依賴 graphify-core/registry，屬專案內層；通用層才進獨立 repo | 獨立 repo（會拖入 graphify 依賴樹，違反重用初衷） |
| D2 | `git mv` 隨碼遷移 tui.rs + ui/ 五檔 | git rename 偵測保留歷史，diff 零 | 複製+刪除（歷史斷裂） |
| D3 | main.rs 僅改 `graphify_tui::run_tui(graph)` 呼叫點 + 移除 `pub mod tui/ui` | 最小接線面 | 保留 re-export mod（殘留雙路徑） |
| D4 | crate 級 allow（collapsible_if、missing_errors_doc）複製自 graphify-cli main.rs 慣例 | 遷移代碼 lint 基線與原 crate 一致，不為搬移重寫邏輯 | 全部重構修 lint（擴大 diff、違反行為不變） |
| D5 | compose 歸屬確認：tui.rs 的 compose 引用全是 `graphify_core::compose_merge`，`crate::compose`（CLI 子命令）留守 graphify-cli | 避免依賴循環；CLI 命令屬薄殼 | compose 進 graphify-tui（CLI 命令與 TUI 耦合） |

## Risks / Trade-offs

- graphify-cli 不再直接依賴 ratatui：未來 CLI 端若需 TUI 片段須經 graphify-tui——這正是分層目的
- workspace lint 繼承：graphify-tui 用 `[lints] workspace = true`，與 kit 的獨立 profile 語法不同但標準一致

## Migration Plan

1. `git mv` 六檔 → 2. 建 Cargo.toml + lib.rs（導出 `run_tui`/`App`/ui）→ 3. workspace members 註冊 → 4. main.rs 接線 + cli 依賴清理 → 5. clippy/test 全綠驗證

## Open Questions

- 無
