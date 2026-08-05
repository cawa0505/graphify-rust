## Why

現有大部分 AST 程式碼分析與關聯圖譜工具皆依賴於 Canvas 渲染的 Web 前端平台，此類方案通常面臨啟動緩慢、高記憶體開銷、登入/上傳等繁雜操作。本專案已實現 16 毫秒的極速本機 AST 靜態解析，若能將其與 Terminal 原生且硬核的 TUI（Terminal User Interface）結合，工程師將能直接以鍵盤在終端機中實現 0 延遲的 Symbol 拓撲檢索、二進位呼叫鏈穿梭與源碼跳轉，在輕量化、流暢度與隱私安全上對 Web Canvas 圖譜平台實現降維打擊。

## What Changes

- **新增 `graphify tui` 子指令**：在 `graphify-cli` 中新增全螢幕交互式 TUI 指令，使用鍵盤在 AST 節點間靈活穿梭。
- **雙欄/三欄交互面板**：左側展示代碼有向拓撲樹（Tree/List），右上展示選中節點的 Inspector（包括入度、出度與屬性），右下提供原始碼快速跳轉與 BFS 呼叫追蹤按鈕。
- **整合 16ms 圖譜引擎**：TUI 引擎直接讀取與載入本機的 `.toon` 格式圖譜，確保在大項目（數千個節點）下依然能實現 60 FPS 的絲滑滾動與響應。

## Capabilities

### New Capabilities
- `tui-graph-interaction`: 本地端全螢幕極速交互式 Ratatui TUI 圖譜拓撲檢索與原始碼跳轉能力。

### Modified Capabilities
無

## Impact

- 影響 `graphify-cli` 的 CLI 指令與編譯依賴，新增 `ratatui` 與 `crossterm` 庫。
- 完全不影響 `graphify-core` 與 `graphify-llm` 的 API 與後向相容性，保持核心高度解耦。
