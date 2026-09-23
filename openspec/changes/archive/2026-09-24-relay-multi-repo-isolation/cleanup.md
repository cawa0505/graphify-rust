# 手動清理指引（舊格式 relay.json 處置）

D4 workspace 主權模型生效後，舊的髒資料檔不會被程式自動刪除或遷移（設計決策：自動清理不動使用者既有資料）。以下檔案建議人工處置：

## 1. `/home/zeng/relay.json`

- 內容：兩個以上專案共用的一檔 relay 狀態（舊 `$HOME` 邊界的產物）。
- 處置：確認內含的 handoff 摘要皆已過時後刪除。若部分摘要仍要保留，先把需要的段落複製到該專案自己的 workspace root（`<repo>/relay.json`，或直接照舊格式把 `repos` 中該筆存進新 root 的新檔），再刪除原檔。
- 刪除後 `$HOME` 不再是 relay root；非 git 目錄下執行 relay 工具會以 cwd 為 workspace root，未 init 時回 NoRoot 指引。

## 2. GraphifyRust repo 的 `.relay/relay.json`（含 `.relay/relay.toon`、lock 檔）

- 內容：已混入 14 筆、12 個以上不相關專案的狀態（絕對路徑 key 與 basename key 並存、跨 workspace baton）。
- 處置：整份刪除（該檔對 GraphifyRust 而言只留跨專案案雜訊）。刪除後在 GraphifyRust root 重新 `relayInit`，即可獲得乾淨的單 workspace 狀態；需要保留的 thread 摘要可先以 `relayAdd` 收進新狀態。
- 注意：`graphify.db`（SQLite registry）不需清理 — DB 層原本就按 `workspace_key` 隔離；舊快照會隨 `handoff-pruning` 機制自然淘汰。

## 3. 其他 repo 內被污染的 relay 檔

各 repo 依同樣原則：刪除根目錄 stray `relay.json` / `.relay/` → 重新 `relayInit`。判斷準則：檔內 `repos` / `project_context` 是否含非本 workspace 的內容。

## 執行時機

不需要在本次 change 內執行。任何時間點删除後重新 init 即可；D1/D5 的「舊格式唯讀 fallback」保證未清理的舊檔仍能載入（不會壞），只是隔離與歸屬要等新寫入後才完全生效。
