# Design: rust-tuikit-extraction

## Context

graphify-cli 的 TUI 通用層（theme/layout/log/modal/flash）需抽成獨立 repo，供其他 Rust TUI 專案重用。用戶決策：直接建 `../RustTuiKit`（方案 A，比照 GraphifyPlugins 跨 repo 慣例），不採 workspace 內先養再抽（方案 B）。

## Goals / Non-Goals

- Goals：通用元件零 graphify 依賴；graphify-cli 行為逐位不變；獨立 repo 可獨立開源
- Non-Goals：不抽 graphify 專屬層（canvas/App shell/ActionTag）——屬 Phase 2（graphify-tui-crate change）

## Decisions

| # | 決策 | 理由 | Alternatives considered |
|---|---|---|---|
| D1 | 落點 `../RustTuiKit` 獨立 repo + path dep `../../RustTuiKit` | 邊界第一天鎖死、可獨立開源、用戶已有 GraphifyPlugins 同款工作流 | workspace 內 graphify-tui 先養（多一次遷移） |
| D2 | kit 僅依賴 `ratatui 0.28`，不引入 crossterm/tokio | 現有同步事件迴圈夠用；零新依賴 | component 模板全套（tokio + clap，過重） |
| D3 | `split(area, log_ratio: Option<u16>)` 參數化 event log 比例 | graphify 傳 `Some(30)` 即現行 70/30；其他專案可自訂 | 硬編碼 30（不通用） |
| D4 | `Flash<K>` 泛型化，`ActionTag` 枚舉留在 graphify | kit 不含任何專案語意 | 具體 ActionTag 進 kit（污染通用層） |
| D5 | `render_list_modal` 回傳內部 items Rect | 保留滑鼠命中測試能力（tui.rs 既有行為） | 回傳 `()`（破壞滑鼠互動） |
| D6 | graphify-cli 端 theme.rs/layout.rs/modal.rs 改薄殼 re-export/委派 | 呼叫點（tui.rs 97 處）零改動 | 全域改 import 路徑（大 diff、易錯） |
| D7 | lint profile 完整複製（pedantic+nursery deny、禁 unwrap/expect） | 與 workspace 同標準，library code 品質不降級 | 寬鬆 lint（重用者踩雷） |

## Risks / Trade-offs

- path dep 脆弱性：sibling repo 不在則 build 失敗——已知且接受（與 GraphifyPlugins 同款），未來可改 git dep + 版本 pin（relay debt: rust-tuikit-no-version-pin）
- `[lints]` table 語法 edition 2024：kit 獨立 Cargo.toml 用新語法，與 workspace 繼承語法不同但效果一致

## Migration Plan

1. 建 kit repo（六模組 + 11 測試）→ 2. graphify-cli 三 shim 接線 → 3. TestBackend 位元級渲染斷言驗證 → 4. 兩 repo 各自 commit/push

## Open Questions

- 無（版本 pin 策略延後至 RustTuiKit 首次升級時）
