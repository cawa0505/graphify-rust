//! Graphify 專屬 TUI 層：App shell、canvas、圖譜 modal。
//! 通用元件（theme/layout/log/modal 骨架/flash）來自 rust-tuikit。

// ponytail: allow missing errors doc as these are TUI entry points (原 graphify-cli main.rs 同款 allow，隨 tui.rs 遷移)
#![allow(clippy::missing_errors_doc)]
// ponytail: collapsible_if 用於事件處理巢狀按鍵分支，攤平反而降低可讀性
#![allow(clippy::collapsible_if)]

mod tui;
pub mod ui;

pub use tui::{App, run_tui};
