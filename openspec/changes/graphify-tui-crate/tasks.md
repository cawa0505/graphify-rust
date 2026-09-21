# Tasks: graphify-tui crate

## 1. Crate 建立
- [x] 1.1 `graphify-tui/Cargo.toml`（edition 2024、deps: graphify-core/graphify-registry/rust-tuikit path/ratatui/anyhow）+ workspace members 註冊
- [x] 1.2 ��移 `tui.rs` + `ui/` 五檔 → `graphify-tui/src/`，`lib.rs` 導出 `run_tui`/`App`/ui 模組

## 2. graphify-cli 瘦身
- [x] 2.1 Cargo.toml 加 graphify-tui dep；main.rs 改 `graphify_tui::run_tui`；移除 `pub mod tui` 與 ui 相關殘留
- [x] 2.2 確認 compose 子命令/其餘模組無 tui/ui 殘留引用

## 3. 驗證
- [x] 3.1 `cargo clippy --workspace --all-targets` 零警告；`cargo fmt`
- [x] 3.2 `cargo test --workspace` 全綠（遷移測試通過、TestBackend 斷言生效）
