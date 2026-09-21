# Tasks: RustTuiKit 範本層

## 1. 新 repo `../RustTuiKit`
- [x] 1.1 `git init` + GitLab remote `rust-tuikit` + `.gitignore` + README（定位：通用 Rust TUI 範本層）
- [x] 1.2 `Cargo.toml`：crate `rust-tuikit` v0.1.0、edition 2024、僅 `ratatui = "0.28"`、複製 GraphifyRust lint profile
- [x] 1.3 `theme.rs`：10 色常數（逐位複製 `ui/theme.rs`）
- [x] 1.4 `layout.rs`：`Chrome` + `split(area, log_ratio: Option<u16>)`
- [x] 1.5 `log.rs`：`LogEntry` + `push_log`（上限 200）+ `render_event_log`
- [x] 1.6 `modal.rs`：`ModalItem` + `centered_rect` + `render_list_modal`
- [x] 1.7 `flash.rs`：`Flash<K>` + `Pill<K>` + `render_pills`
- [x] 1.8 `component.rs`：最小 object-safe `Component` trait
- [x] 1.9 單元測試：split 比例、push_log 上限、centered_rect、Flash 220ms 生命週期、TestBackend 位元級渲染斷言

## 2. GraphifyRust 整合
- [x] 2.1 `graphify-cli/Cargo.toml` 加 `rust-tuikit = { path = "../../RustTuiKit" }`
- [x] 2.2 `ui/theme.rs` 改 re-export `rust_tuikit::theme`
- [x] 2.3 `ui/layout.rs`：`split`/`LogEntry`/`push_log`/`render_event_log`/`Flash`/`render_footer` 改委派 kit（ActionTag 留原地）
- [x] 2.4 `ui/modal.rs`：`ModalItem`/`centered_rect`/`draw_list_modal` 改委派 kit `render_list_modal`；plugin panel/workspace selector/compose 留原地改用 kit 骨架
- [x] 2.5 淨刪重複定義（目標 graphify-cli 淨刪 ~250 行）

## 3. 驗證
- [x] 3.1 `cargo clippy --all-targets` 兩 repo 零警告
- [x] 3.2 `cargo test` 兩 repo 全綠（含 TestBackend 渲染一致性斷言）
- [x] 3.3 TUI 手動煙霧測試（TestBackend 位元級斷言已覆蓋 modal/event log/footer 渲染一致性；live TUI 目檢 2026-09-21 使用者初驗通過）
- [x] 3.4 RustTuiKit 首次 commit + push（隱私審計：無 IP/hostname/個人路徑）
