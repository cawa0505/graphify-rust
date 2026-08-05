## 1. Setup & Dependencies

- [ ] 1.1 新增 `ratatui` (0.28) 和 `crossterm` (0.28) 依賴至 `graphify-cli/Cargo.toml`
- [ ] 1.2 建立全新模組 `graphify-cli/src/tui.rs` 骨架與基礎架構

## 2. Core TUI Application State

- [ ] 2.1 實作 `App` 狀態管理結構體，包含 loaded nodes, filtered list, highlighted index, search text 欄位
- [ ] 2.2 實作圖譜載入，將 `GraphOutput` 轉換為 petgraph，預先建立邊與相鄰節點的雙向索引，用於 Inspector 高速查詢
- [ ] 2.3 實作 Panic Hook 攔截器，確保 TUI 在崩潰時自動強制恢復 Terminal 原生 Mode

## 3. UI Terminal Render Layout

- [ ] 3.1 實作 `ratatui::Layout` 雙欄版面切分（左欄 50% 拓撲樹，右欄 50% Inspector）
- [ ] 3.2 實作左欄節點與層級渲染列表，支援高亮與 CJK 中文字元寬度防偏移對齊
- [ ] 3.3 實作右欄 Inspector 渲染面板，格式化輸出 Node ID, Type, Language, Line, Inferred Incoming/Outgoing Call lists

## 4. Keyboard Event Loop

- [ ] 4.1 實作 Crossterm 事件監聽輪詢循環，捕獲 KeyPress 事件
- [ ] 4.2 實作 `j`/`k` 與方向鍵的上下捲動，並在 10 毫秒內驅動 Inspector 響應刷新
- [ ] 4.3 實作 `q` 與 `Esc` 的退出處理，還原 Terminal Raw Mode

## 5. Live Fuzzy Filtering

- [ ] 5.1 實作按下 `/` 鍵聚焦搜尋，即時捕獲鍵盤字元輸入至 `search_query`
- [ ] 5.2 實作 `O(N)` 高速字串模糊比對（不區分大小寫），即時更新 filtered_indices

## 6. Physical Editor Integration

- [ ] 6.1 實作按下 `g` 或 `Enter` 鍵時，乾淨釋放 Terminal 緩衝，拉起 `$EDITOR` 並附加 `+line` 參數進行精準跳轉
- [ ] 6.2 實作編輯器關閉後，完全恢復 Terminal Raw Mode 並重繪 TUI 畫面的重置邏輯

## 7. Command Line Integration

- [ ] 7.1 在 `graphify-cli` 的 `Commands` enum 中新增 `tui` 子指令
- [ ] 7.2 在 `main.rs` 的執行路由中綁定 `tui` 子指令，讀取指定的圖譜檔案並引導進入 `tui::run_tui` 流程
