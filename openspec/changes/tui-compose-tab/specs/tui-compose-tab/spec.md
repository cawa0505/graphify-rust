# Delta Spec: tui-compose-tab

## ADDED Requirements

### Requirement: TUI Compose 面板開啟與關閉

TUI SHALL 提供單鍵 `y` 開啟 compose 面板，且關閉（Esc）後 SHALL 回到與開啟前完全相同的 inspector 狀態。

#### Scenario: 單鍵開啟

- **WHEN** 使用者在 inspector 主畫面按下 `y` 或 `Y`
- **THEN** 顯示 compose 面板 menu 層，列出可用的 manifest 檔
- **AND** 預設畫面（雙欄拓撲 + Inspector）不受影響

#### Scenario: 逐層關閉

- **WHEN** 使用者在 diagram 層按 Esc
- **THEN** 返回 menu 層
- **WHEN** 使用者在 menu 層按 Esc
- **THEN** 關閉面板，回到 inspector，且 inspector 的選取/捲動狀態與開啟前一致

### Requirement: Manifest Menu 層

Menu 層 SHALL 掃描當前 workspace 根目錄與 `manifests/` 子目錄下的 `*.yaml` 檔案，以列表呈現，支援 `j/k` 或方向鍵 hover、Enter 選取、滑鼠點選。

#### Scenario: 列出 manifests

- **WHEN** workspace 根目錄存在 `ecommerce.yaml` 且 `manifests/` 存在 `observability.yaml`
- **THEN** menu 列出兩個檔名（相對路徑顯示）
- **AND** 無任何 manifest 時顯示提示文字（「找不到 Assembly Manifest（*.yaml）」）而非空白

#### Scenario: 滑鼠點選

- **WHEN** 使用者以滑鼠點選列表中某一 manifest
- **THEN** 效果等同 hover 該項後按 Enter

### Requirement: Diagram 層 ASCII 關聯圖

選定 manifest 後 SHALL 顯示該 manifest 組裝後的粗粒度 ASCII 關聯圖：每個 workspace 一個 box-drawing 容器（`┌─┐│└┘`，標題為 workspace id），跨 workspace composition 邊以 `─ relation ─▶` 呈現。渲染 SHALL 呼叫 `graphify-core` 的 `load_manifest` + `merge` + `project`，不得 shell-out 外部程序（npx）。

#### Scenario: 顯示 workspace 關聯

- **GIVEN** manifest 宣告 workspaces `agentshop`、`nexusledger` 與 relation `agentshop uses nexusledger`
- **WHEN** 使用者選取該 manifest
- **THEN** 顯示兩個 workspace box，其間有一條標註 `uses` 的箭頭線

#### Scenario: 驗證錯誤顯示

- **WHEN** manifest 引用不存在的 workspace 或節點（`load_manifest` 回傳錯誤）
- **THEN** diagram 層以紅字逐行列出錯誤訊息，而非靜默空白或離開面板

#### Scenario: 圖高於畫面

- **WHEN** ASCII 圖高度超過面板可視高度
- **THEN** 支援上下捲動（鍵盤 `j/k` 於 diagram 層捲動、滑鼠滾輪）

### Requirement: 效能與無常駐負擔

面板開啟與 manifest 選取 SHALL 為同步一次性計算（掃描 + 讀檔 + 合併），不得引入常駐背景執行緒或輪詢。

#### Scenario: 開啟延遲

- **WHEN** manifest 涵蓋 2–8 個 workspace
- **THEN** menu 開啟與 diagram 渲染在互動延遲內完成（無可感知停頓）

### Requirement: 既有互動不受影響

本面板 SHALL 不得改變既有鍵盤/滑鼠/搜尋/`$EDITOR` 跳轉/BFS modal 的行為，不得產生按鍵衝突（`y` 目前未綁定）。

#### Scenario: 按鍵衝突檢查

- **WHEN** 檢視 inspector 主畫面按鍵綁定表
- **THEN** `y`/`Y` 未被既有功能使用
