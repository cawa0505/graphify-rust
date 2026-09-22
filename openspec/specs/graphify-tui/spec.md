# graphify-tui Specification

## Purpose

Graphify 專屬 TUI 層（workspace crate）：App shell、canvas、圖譜專屬 modal 與 ActiveTab/ActionTag，通用元件委派 rust-tuikit，使 graphify-cli 成為薄殼。

## Requirements

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

#### Scenario: path dep 解析

- **WHEN** 於 GraphifyRust workspace 執行 `cargo build -p graphify-tui`
- **THEN** rust-tuikit 解析至 `../../RustTuiKit` 本地路徑，建置成功且無額外 registry 下載

## Verification Evidence

- `cargo clippy --workspace --all-targets`：0 警告（2026-09-21；2026-09-23 複驗 0 警告）
- `cargo test --workspace`：167 通過（2026-09-23 複驗 167 通過、0 失敗）
- git rename 偵測：tui.rs + ui/ 五檔零 diff 遷移（c39f91d，2026-09-23 複驗）
- 隨碼遷移測試：tui.rs 2 項（compose_tests）、ui/ 五檔 0 項——graphify-tui 本體 2 項
  （2026-09-23 複驗；早前記錄誤植為 26 項，已依遷移前舊碼實測修正）
- TUI live 目檢：使用者初驗通過（2026-09-21）
