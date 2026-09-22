# Design: TUI Compose Tab

## Context

`workspace-compose`（commit e2e29bd）已落地：`graphify-core` 提供 manifest 載入/驗證（`compose_manifest`）、合併（`compose_merge`）、投影（`compose_render`）；CLI 提供 `graphify compose validate/graph/render`。TUI（`graphify-cli/src/tui.rs`，ratatui 0.28）現有 modal 機制（`ModalState`，`ui/modal.rs`）已承載 BFS trace、Relations、Plugin panel、Workspace selector。

使用者需求：TUI 內以 menu 選 manifest → 顯示 workspace 關聯 ASCII 圖。

## Goals / Non-Goals

**Goals**
- 單鍵（`y`）開啟 Compose 面板；預設畫面維持 graph inspector 不變（AGENTS.md TUI 約束）
- Menu 層：列出 workspace 根目錄與 `manifests/` 下的 `*.yaml`
- Diagram 層：粗粒度手繪 ASCII（workspace box + composition 邊），j/k 與滾輪捲動
- 驗證錯誤紅字逐行顯示，不靜默空白
- Esc 逐層返回（diagram → menu → inspector）
- 滑鼠：menu 點選 manifest、diagram 滾輪捲動（mouse 為本專案 baseline）

**Non-Goals**
- 不在 TUI 內 shell-out 到 `npx box-of-rain`（延遲 + Node 依賴）；fine-grained 佈局走 CLI `compose render`
- 不做 manifest 編輯（唯讀檢視；寫入走 MCP `graphify_compose_write`）
- 不新增常駐背景執行緒（同步一次性載入，AGENTS.md 約束）

## Decisions

### D1: 手繪 ASCII，不用 box-of-rain
粗粒度層通常 2–8 個 workspace，box-drawing 字元網格（`┌─┐│└┘` + `ws_a ── uses ──▶ ws_b`）由 `render_compose_ascii()` 直接產生。零外部依賴、零延遲。成員標籤每 workspace 最多顯示 3 行（可讀性上限，完整圖走 `compose graph`）。

### D2: 重用 ModalState 機制，非獨立 tab
`ModalState::ComposePanel` 變體（雙層：`selected: None` = menu、`Some(path)` = diagram），重用既有 modal 邊框/鍵盤/滑鼠基礎設施。符合「新功能走覆蓋層」約束，Esc 關閉後回到與今天完全相同的 inspector。

### D3: 資料路徑
`scan_compose_manifests()`（同步掃描 cwd + `manifests/`）→ `select_compose_manifest()` → `graphify_core::compose_manifest::load_manifest` + `compose_merge::build_unified_graph` → `render_compose_ascii()`。錯誤收集為 `Vec<String>`（不 fail-fast），於 diagram 層紅字顯示。

### D4: 鍵盤
`y` 開啟（任何狀態）；menu 層 j/k/↑/↓ 移動、Enter/g 選取、Esc/c 關閉；diagram 層 j/k/↑/↓ 捲動、Esc 回 menu。既有 `w`/`p`/`g`（editor）等按鍵行為不變。

## Risks / Trade-offs

- 手繪網格在 workspace 數 > 10 時可能超出終端寬度 → 捲動已涵蓋垂直方向；水平截斷可接受（粗粒度視角本就少於 10 個 workspace）。
- 同步載入大 manifest 可能短暫凍結 UI → manifest 規模小（人寫的組裝說明書），可接受；未來有需要再改 async。
