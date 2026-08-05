## Purpose

本功能為 GraphifyRust 提供一個極速、完全本機端且具交互性的 Terminal User Interface (TUI) 圖譜導航與原始碼檢索面板，大幅優化工程師在終端機中穿梭 AST 節點、呼叫關係與追蹤架構之開發者體驗。

## ADDED Requirements

### Requirement: Interactive TUI Launch
當使用者在命令列中執行 `graphify tui` 指令時，系統 MUST 在 100 毫秒內載入指定的圖譜檔案（預設為 `graphify-out/graph.toon`），初始化全螢幕終端繪製介面，並在退出時乾淨還原 Terminal 狀態。

#### Scenario: Launch with default graph
- **WHEN** 使用者輸入 `graphify tui` 且本機預設路徑存在合法的 `.toon` 檔案
- **THEN** 系統立即渲染全螢幕雙欄面板並進入事件監聽循環

#### Scenario: Launch with missing graph file
- **WHEN** 使用者輸入 `graphify tui` 但指定的圖譜檔案不存在
- **THEN** 系統 MUST 乾淨報錯退出，不留殘留終端緩衝，並引導使用者先執行 `extract`

### Requirement: Keyboard Navigation and Source Code Jump
使用者 MUST 能透過 `j`/`k` 或方向鍵在上、下節點之間流暢滾動移動選中標線。當使用者在選中節點上按下 `g` 或 `Enter` 時，系統 MUST 直接開啟外部編輯器（如 Neovim, Vim, nano, 或是 VSCode）並自動跳轉至該節點對應之原始碼精密行號。

#### Scenario: Smooth node scrolling
- **WHEN** 使用者在節點列表上按下 `j` 鍵
- **THEN** 選中標線下移一格，且右側的 Inspector 面板與 Incoming/Outgoing 拓撲呼叫關係必須在 10 毫秒內同步重新渲染完成

#### Scenario: Jump to source code on-disk
- **WHEN** 使用者在某個 AST 節點上按下 `g` 鍵
- **THEN** 系統檢索該節點的 `source_file` 與 `start_line`，並以子進程啟動 `$EDITOR`（或預設 `vi`）自動定位到該行

### Requirement: Live Fuzzy Filtering
TUI 介面 MUST 提供即時模糊搜尋框，當使用者按下 `/` 時聚焦搜尋輸入。使用者輸入任意字元時，節點列表 MUST 在 15 毫秒內完成 Live 過濾高亮，並動態更新左側拓撲樹。

#### Scenario: Trigger search and filter
- **WHEN** 使用者按下 `/` 鍵並輸入 "MemoryConfig"
- **THEN** 左側列表立即僅過濾保留符合該 Label 或 ID 模糊比對之節點
