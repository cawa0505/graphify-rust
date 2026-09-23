# Proposal: handoff-doctor

## Why

relay-multi-repo-isolation 上線後，舊格式與跨 workspace 污染的 relay 狀態檔需要人工清理。實際清理時（2026-09-24）驗屍邏輯是一次性 Python script：逐檔解析 relay.json、比對 repo 條目與磁碟實況、判定外來 repo 污染。這套判定不該只活在對話歷史裡—— 未來任何 repo 都可能再出現 stray relay.json、bare-name 假條目、指向不存在目錄的 path。需要一個產品化的 doctor tool：唯讀報告為預設、刪除為 opt-in，把清理準則變成可重跑、可驗證的指令。

## What Changes

- 新增 `graphify handoff doctor` 子指令（CLI 入口，實作於 graphify-plugin-handoff 新模組）：
  - **預設（唯讀報告，fail-safe）**：檢查指定 relay root（或 `--scan <dir>` 掃描多 repo）下的 relay 狀態檔，逐項回報狀態與證據，不修改任何檔案。
  - **`--fix`（ opt-in）**：僅刪除判定為 dirty 的檔案（relay.json + 對應 `.relay/` 鏡像目錄），刪除前列出將刪清單、刪除後回報結果。clean 與 info 級發現一律不動。
- 檢查項（沿用實證驗屍準則）：
  1. **unparseable**：relay.json JSON 解析失敗（dirty）。
  2. **foreign 條目**：`repos` 中 bare-name key ≠ relay root basename 且 root 下無同名子目錄（dirty）；`path` 為絕對路徑、解析後位於 relay root 外且非 root 本身（dirty）；`path` 指向不存在目錄（dirty）。合法 monorepo 子目錄（root 下存在同名目錄）不算 foreign。
  3. **stray 位置**：`$HOME` 正下方的 relay.json（dirty，workspace 主權模型下必為共用髒檔）；非 workspace root 位置的 relay.json。
  4. **legacy schema（info，不計 dirty）**：頂層仍有全域 `project_context`/`state_snapshot` 舊欄位 → 提示可用新格式重 init，不建議刪除（D1/D5 唯讀 fallback 保證可用）。
- 退出碼：0 = 全部乾淨、1 = 發現 dirty（供 CI / script 使用）、2 = 執行錯誤。
- 不修改 relay 執行期語意：doctor 是純消費層檢查工具，不觸碰 RelayPlugin 的 bind/save/close 流程。

## Capabilities

### New Capabilities

- `handoff-doctor`: relay 狀態檔健康檢查——檢查項判定準則（foreign/stray/unparseable/legacy）、唯讀報告輸出格式、`--fix` 刪除語意與 opt-in 邊界、`--scan` 多 repo 掃描、退出碼契約。

## Impact

- **graphify-plugin-handoff**：新增 `src/doctor.rs`（或 `doctor/mod.rs` + `doctor/checks.rs`，遵守 300 行上限）；報告/刪除核心邏輯 + 單元測試。
- **graphify-cli**：`HandoffCommand` enum 新增 `Doctor { fix, scan }` 變體 + run 函式接線。
- **graphify-mcp**：不在範圍——doctor 是人類操作者的清理工具，非 AI authoring 面（未來有需求再議，`[待討論]` 不預先擴充）。
- **既有行為不變**：relay 工具、handoff 渲染、registry 同步均不受影響；doctor 只讀檔案系統與 relay.json，`--fix` 只刪 dirty 檔。
- **參考**：`openspec/changes/archive/2026-09-24-relay-multi-repo-isolation/cleanup.md`（人工清理準則的原始版本，doctor 是其產品化）。
