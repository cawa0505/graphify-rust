# Proposal: TUI Compose Tab（workspace 關聯 ASCII 視角）

## Why

workspace-compose（e2e29bd）已提供 Assembly Manifest → unified graph 的 CLI/MCP 能力，但 TUI 內看不到跨 workspace 關聯。使用者想在 TUI 以「選 manifest → 看 ASCII 關聯圖」的方式檢視組裝視角，不必離開 TUI 去 shell-out。

## What Changes

- 新增 TUI compose 面板（`ModalState::ComposePanel`），單鍵 `y` 開啟，Esc 關閉後回到與今天完全相同的 inspector。
- 兩層互動：
  1. **Menu 層**：掃描當前 workspace 根目錄的 `*.yaml`（Assembly Manifest 慣例位置：根目錄與 `manifests/`），列表 + hover + Enter 選取（沿用 WorkspaceSelector 骨架）。
  2. **Diagram 層**：選定後顯示該 manifest 的 unified graph 粗粒度 ASCII 圖（workspace box + 跨域 composition 邊，box-drawing 字元手繪），Esc 回 menu。
- 渲染走 `graphify-core` 既有 pub 函式（`compose_manifest::load_manifest` + `compose_merge::merge` + `compose_render::project`），**不 shell-out npx box-of-rain**（零延遲、零 Node 依賴；粗粒度層 2–8 個 workspace box 手繪網格足夠）。
- 驗證錯誤（manifest 不合法、節點引用不存在）在 Diagram 層以紅字列出，不靜默空白。
- 滑鼠：menu 點選、diagram 區域 Esc/滾輪捲動（超出畫面時），符合滑鼠互動基線。

## Capabilities

### New: tui-compose-tab

- `tui.rs` 新增 `ModalState::ComposePanel { stage, manifests, hovered, diagram, error }`。
- `ui/modal.rs` 渲染分支 + `title()`。
- 鍵盤：`y` 開啟、`j/k` hover、Enter 選取、Esc 逐層返回（diagram → menu → inspector）。
- 不改變既有 inspector 預設畫面與互動；不新增常駐執行緒（開啟時同步掃描+合併，2–8 workspace 規模 < 10ms）。

### Modified: 無既有 capability 行為變更

既有 modal 機制重用，無 schema/CLI/MCP 變更。

## Impact

- `graphify-cli/src/tui.rs`（+~80 行：開啟函式、鍵盤分支）
- `graphify-cli/src/ui/modal.rs`（+~120 行：ComposePanel 渲染 + ASCII 手繪）
- 既有行為零變更（Esc 關閉後回到今天完全相同的 inspector）。
