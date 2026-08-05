## Context

本項設計詳述如何在 `graphify-cli` 中整合 Ratatui 與 Crossterm，打造一個零網路開銷、毫秒級響應、且完全運作於終端機中的交互式圖譜導航器 (TUI Inspector)。本設計直接讀取並載入本機的 `.toon` 格式圖譜，將其轉化為內部的 `petgraph::graph::DiGraph` 實體，實現極速檢索與呼叫關係分析。

## Goals / Non-Goals

**Goals:**
- 提供全螢幕、美觀、流暢的雙欄終端交互 UI 面板。
- 支持 `j`/`k` / `Up`/`Down` 鍵的無縫捲動導航。
- 支持與本機系統編輯器 `$EDITOR`（如 Neovim, Vim, nano, VSCode）的物理級整合，按 `g` 鍵拉起子進程秒級定位至程式碼行號。
- 支援 `/` 即時輸入模糊檢索過濾，15 毫秒內高亮展示比對結果。
- 顯示選中節點的詳細入度與出度呼叫關係（誰呼叫它、它呼叫了誰）。

**Non-Goals:**
- 不支持 TUI 內的代碼直接編輯與保存（代碼編輯完全交由外部 $EDITOR 完成，遵循 Unix Philosophy 專注做好一件事）。
- 不支持 TUI 介面下的 Graph 寫入與持久化修改。
- 不引入過重的 3D 或 WebGL 終端繪製，完全聚焦於高性能 2D 文本與 Box 框架。

## Decisions

### 1. 框架選型：`ratatui` + `crossterm`
- **決策**：使用最新社群維護的 `ratatui = "0.28"` 與 `crossterm = "0.28"` 作為繪製 backend。
- **理由**：
  * `ratatui` 是 Rust 目前性能最高、社群最活躍的 TUI 繪製庫，提供豐富的 `Layout`, `Paragraph`, `List`, `Block` 與 `Border` 等元件。
  * `crossterm` 是純 Rust 跨平台終端控制庫，在 Linux, macOS 上支援完美的色彩、滑鼠與鍵盤事件捕獲。
- **替代方案**：使用基於 C 語言 FFI 的 `ncurses` 或過時的 `tui-rs`，但編譯開銷大、跨平台相容性差，故放棄。

### 2. 資料結構：In-Memory Petgraph 與動態 Filter
- **決策**：載入 `.toon` 圖譜後，在 TUI 狀態（`App` 結構體）中保持一個 `GraphOutput` 與一個 `petgraph::graph::DiGraph`，並動態維護一個 `filtered_indices: Vec<usize>` 陣列。
- **理由**：當使用者輸入 `/` 進行模糊搜尋時，我們直接對 `GraphOutput.nodes` 列表進行 `O(N)` 的快如閃電的比對（在數千個節點下耗時 <0.1ms），並將過濾後的 Node 索引存入 `filtered_indices`。TUI list 元件只需渲染該索引列表，完全免去每次過濾都重建整個 petgraph 的巨額開銷。

### 3. 與外部 $EDITOR 的整合：Crossterm Terminal Raw Mode 釋放與子進程拉起
- **決策**：當使用者按下 `g` 鍵觸發編輯器跳轉時，TUI 必須：
  1. 暫時關閉 `crossterm` 的 Raw Mode 並隱藏 TUI 畫面（`disable_raw_mode` 和 `execute!(stdout, LeaveAlternateScreen)`）。
  2. 拉起編輯器子進程（`std::process::Command::new(editor).arg(format!("+{}", line)).arg(file_path).status()?`）。
  3. 待編輯器退出（使用者關閉編輯器）後，**重新開啟 Raw Mode 並重置 TUI 繪製畫面**（`enable_raw_mode` 和 `execute!(stdout, EnterAlternateScreen)`）。
- **理由**：這能提供絕無僅有的順暢流暢度，編輯器啟動完全不與 TUI 衝突，且 TUI 状态保持完美不丟失。

## Risks / Trade-offs

- **[Risk] Terminal 崩潰殘留狀態** → 若 TUI 程式中途 panic 崩潰，Terminal 可能會卡在 Raw Mode 或 Alternate Screen 導致游標消失。
  * **Mitigation**: 實作 `std::panic::set_hook` 攔截 panic，在 panic 觸發時自動強制重置 `crossterm` terminal 狀態，確保終端機環境絕對安全乾淨。
- **[Risk] CJK 字元（中文字）寬度在終端機對齊偏移** → 中文字元在 Unicode 寬度上通常佔用兩格（Double Width），在計算 List 截斷時容易導致排版偏移。
  * **Mitigation**: 使用 `ratatui` 內建的 unicode-width 處理，不使用簡單的 `string.len()` 計算寬度，防禦排版混亂。
