# Delta Spec: rust-tuikit

## ADDED Requirements

### Requirement: 通用層零 graphify 依賴

`rust-tuikit` crate SHALL 僅依賴 `ratatui`（v0.1 固定 0.28），SHALL NOT 依賴任何 graphify crate、crossterm（v0.1）或其他專案語意型別。`GraphifyRust → rust-tuikit` 依賴方向 SHALL 單向。

#### Scenario: 依賴邊界

- **WHEN** 檢視 `../RustTuiKit/Cargo.toml`
- **THEN** `[dependencies]` 僅含 `ratatui = "0.28"`
- **AND** crate 內無 `graphify` 字串引用

### Requirement: theme 色盤

`rust_tuikit::theme` SHALL 提供與 graphify `ui/theme.rs` 相同的 10 個色常數（BG、SURFACE、SURFACE_HI、TEXT、SUBTLE、GREEN、GOLD、CYAN、MAUVE、RED、BLUE），RGB 值逐位一致。

#### Scenario: 色值一致

- **WHEN** graphify-cli re-export `rust_tuikit::theme` 後編譯
- **THEN** 既有呼叫點零修改即通過編譯，色值不變

### Requirement: Chrome 版面分割

`rust_tuikit::layout::split(area, log_ratio)` SHALL 回傳 `Chrome { tabs, main, log, footer }`：tabs 高 3、footer 高 3、main/log 依 `log_ratio`（`None` 時 log 高 0、main 佔滿）。graphify 呼叫 `split(area, Some(30))` 行為 SHALL 與現行 `split(area, true)` 一致。

#### Scenario: 版面比例

- **WHEN** `split(Rect::new(0,0,100,100), Some(30))`
- **THEN** tabs=3 行、footer=3 行、main=70%、log=30%
- **WHEN** `split(area, None)`
- **THEN** log=0、main=100%

### Requirement: 事件日誌

`rust_tuikit::log` SHALL 提供 `LogEntry`、`push_log`（上限 200 筆，超出移除最舊）與 `render_event_log`（標題含筆數、邊框 SURFACE_HI）。

#### Scenario: 日誌上限

- **WHEN** push 第 201 筆
- **THEN** 最舊一筆被移除，長度維持 200

### Requirement: Modal 骨架

`rust_tuikit::modal` SHALL 提供 `ModalItem`、`centered_rect(percent_x, percent_y, area)` 與 `render_list_modal(f, title, hint, items, hovered, area) -> Rect`（Clear 疊加、MAUVE 亮紫粗體邊框、hint 行、hover `▶` 箭頭 + `>` highlight、footer `N items`、回傳列表內部 Rect 供滑鼠命中測試）。渲染輸出 SHALL 與 graphify 現行 `draw_list_modal` 位元級一致（TestBackend 驗證）。

#### Scenario: hover 列表渲染

- **WHEN** `render_list_modal` 帶 3 個 items、hovered=1
- **THEN** 第 2 列顯示 `▶` 箭頭與 `> ` highlight 符號
- **AND** 回傳值為列表內部區域 Rect

### Requirement: Flash 與 Footer Pills

`rust_tuikit::flash` SHALL 提供泛型 `Flash<K: Copy+Eq>`（trigger/is_active/tick，220ms）與 `Pill<K>`、`render_pills(f, pills, flash, title, area)`（藥丸反白閃爍：active 時 bg=藥丸色、fg=BG）。`ActionTag` 枚舉屬 graphify，SHALL NOT 進入 kit。

#### Scenario: 藥丸閃爍

- **WHEN** `flash.trigger(K)` 後立即 `render_pills`
- **THEN** 對應藥丸 bg=藥丸色、key/label fg=BG
- **WHEN** 220ms 後 `tick()` 再渲染
- **THEN** 恢復 SURFACE_HI 底色

### Requirement: Component trait

`rust_tuikit::component` SHALL 提供最小 object-safe `Component` trait：`render(&self, f, area)` 必要、`handle_key`/`handle_mouse` 預設 no-op。

#### Scenario: object safety

- **WHEN** 宣告 `Box<dyn Component>`
- **THEN** 編譯通過（object-safe）

### Requirement: lint 規範

`../RustTuiKit` SHALL 複製 GraphifyRust workspace lint profile：`unsafe_code = "forbid"`、clippy all/pedantic/nursery deny、`unwrap_used`/`expect_used` deny。所有編譯與 clippy 警告 SHALL 為零。

#### Scenario: lint 乾淨

- **WHEN** `cargo clippy --all-targets` 於 `../RustTuiKit`
- **THEN** 零警告

### Requirement: graphify-cli 整合不變

graphify-cli SHALL 以 path dep 引入 `rust-tuikit`，`ui/theme.rs`、`ui/layout.rs`、`ui/modal.rs` 通用部分改 re-export/委派；graphify 專屬渲染（plugin panel、workspace selector、compose、canvas、ActionTag）留在 graphify-cli。TUI 渲染輸出與互動行為 SHALL 不變。

#### Scenario: 行為不變

- **WHEN** `cargo test -p graphify-cli` 與 TUI 手動煙霧測試
- **THEN** 全綠，且 inspector/modal/footer 渲染與抽取前一致
