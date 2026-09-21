# Delta Spec: graphify-tui crate 抽取（Phase 2）

UPSTREAM: rust-tuikit-extraction

## ADDED Requirements

### Requirement: workspace 內新增 crate `graphify-tui`

graphify-tui 是 graphify 專屬 TUI 層（App shell、canvas、圖譜專屬 modal、ActiveTab/ActionTag），
依賴 `graphify-core`、`graphify-registry`、`rust-tuikit`、`ratatui`，不依賴 graphify-llm/memory/mcp。

- cwd 定義：`graphify-tui/src/` 含 `lib.rs`（`run_tui` + `App`）、`ui/{canvas,layout,modal,theme,mod}.rs`
- `graphify-cli` 移除 `src/tui.rs`、`src/ui/`，改以 `graphify_tui::run_tui(graph)` 呼叫
- `compose`（validate/graph/render）屬 CLI 子命令層，留在 graphify-cli

#### Scenario: 依賴方向單向

- **WHEN** 編譯 workspace
- **THEN** `graphify-tui` 只依賴 graphify-core + graphify-registry + rust-tuikit（+ratatui/anyhow）
- **AND** graphify-core / graphify-registry / rust-tuikit 的 Cargo.toml 不出現 graphify-tui

#### Scenario: 行為不變

- **WHEN** 執行 `graphify` 進入 TUI
- **THEN** App shell、雙欄 inspector、modal、footer 藥丸、事件日誌渲染行為與抽取前逐位一致
- **AND** `cargo test --workspace` 全綠（原 tui.rs/ui 內單元測試隨碼遷移並通過）

### Requirement: rust-tuikit 引入方式

graphify-tui 以 path dep `../../RustTuiKit` 引入 rust-tuikit（比照 GraphifyPlugins 慣例），
與 graphify-cli 的引入方式一致並共用同一 target 解析。
