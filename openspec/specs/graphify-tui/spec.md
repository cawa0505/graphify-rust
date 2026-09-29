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

### Requirement: TUI remote 模式（--remote 選 server）

`graphify tui` SHALL 支援兩種資料來源模式：(a) **local**（預設，現行契約零回歸
——直讀本機 `.toon` 圖譜，workspace host 上無感）；(b) **remote**——`--remote
<URL>` 旗標或 `GRAPHIFY_REMOTE` env 指向 graphify 服務，經同一 streamable HTTP
端點查詢（與 nexus 共用同一份狀態）。旗標 SHALL 覆蓋 env。

#### Scenario: local 模式不回歸

- **GIVEN** 在 workspace host 本機、無 `--remote` 無 `GRAPHIFY_REMOTE`
- **WHEN** 執行 `graphify tui`
- **THEN** 行為與本變更前完全一致（直接載入本機 `.toon`，不發網路請求）

#### Scenario: remote 查詢

- **GIVEN** `graphify tui --remote http://192.0.2.10:9899`（或等值 env）
- **WHEN** 使用者在 TUI 內檢索節點 / trace path / summary
- **THEN** 查詢經 streamable HTTP 送達 graphify 服務，由服務所在機的圖譜回答

#### Scenario: remote 不可達

- **GIVEN** `--remote` 指向的服務未啟動
- **WHEN** TUI 啟動
- **THEN** 顯示連線失敗與 server URL（可定位的錯誤），不靜默退化為 local 圖譜

## Verification Evidence

- `cargo clippy --workspace --all-targets`：0 警告（2026-09-21；2026-09-23 複驗 0 警告）
- `cargo test --workspace`：167 通過（2026-09-23 複驗 167 通過、0 失敗）
- git rename 偵測：tui.rs + ui/ 五檔零 diff 遷移（c39f91d，2026-09-23 複驗）
- 隨碼遷移測試：tui.rs 2 項（compose_tests）、ui/ 五檔 0 項——graphify-tui 本體 2 項
  （2026-09-23 複驗；早前記錄誤植為 26 項，已依遷移前舊碼實測修正）
- TUI live 目檢：使用者初驗通過（2026-09-21）
