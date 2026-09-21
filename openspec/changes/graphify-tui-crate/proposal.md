# Proposal: graphify-tui crate 抽取（Phase 2）

## Why

Phase 1 已將通用層抽至 ../RustTuiKit（remote gitlab.com:saaslab/rust-tuikit）。graphify 專屬 TUI
（canvas 渲染 GraphOutput、App shell、ActiveTab/ActionTag、專屬 modal）仍留在 graphify-cli（tui.rs
1901 行 + ui/ 916 行）。抽成 workspace crate `graphify-tui` 後：

- graphify-cli 瘦身為 CLI 子命令層 + run_tui 呼叫
- 未來 P3/P5（#3216 TUI Stage 1/2）以 graphify-tui 為宿主
- 依賴方向收斂：graphify-tui → {graphify-core, graphify-registry, rust-tuikit}

## What Changes

- 新增 workspace member `graphify-tui`（lib crate，edition 2024，lint profile 同 workspace）
- 搬移 `graphify-cli/src/tui.rs` + `graphify-cli/src/ui/{canvas,layout,modal,theme,mod}.rs` → `graphify-tui/src/`
- graphify-cli 以 `graphify_tui::run_tui(graph)` 呼叫；`compose` 子命令層留守
- rust-tuikit 以 path dep `../../RustTuiKit` 引入（與 Phase 1 graphify-cli 相同慣例）

## Impact

- 零行為變更：渲染逐位一致（TestBackend 斷言 + 既有 26 測試隨碼遷移）
- graphify-mcp 不受影響（無 TUI）
