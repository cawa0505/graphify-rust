# handoff-doctor 變更 — Delta Spec

## Purpose

定義 `graphify handoff doctor` 的健康檢查語意：relay 狀態檔的檢查項判定準則（dirty / info / clean）、唯讀報告為預設行為、`--fix` 刪除的 opt-in 邊界、`--scan` 多 repo 掃描、退出碼契約。目標：把 relay-multi-repo-isolation 之後的人工清理準則產品化為可重跑、可驗證的指令，且任何情況下 doctor 預設不修改使用者檔案。

## ADDED Requirements

### Requirement: 唯讀報告為預設行為

`graphify handoff doctor`（無 `--fix`）SHALL 只讀取檔案系統與 relay 狀態檔並輸出報告，SHALL NOT 修改、移動或刪除任何檔案。報告 SHALL 逐項列出：檔案路徑、判定等級（DIRTY / INFO / CLEAN）、判定原因與證據（如 foreign 條目名稱與其 path 值）。

#### Scenario: 預設執行不修改檔案

- **GIVEN** 某 relay root 的 relay.json 含 foreign 條目
- **WHEN** 執行 `graphify handoff doctor`（無 `--fix`）
- **THEN** 該檔被回報為 DIRTY 且附證據
- **AND** relay.json 與 `.relay/` 目錄位元組不變（刪除前後內容一致）

### Requirement: 檢查項判定準則

doctor SHALL 依以下準則判定 relay 狀態檔：

1. **unparseable（DIRTY）**：relay.json 無法以 JSON 解析。
2. **foreign 條目（DIRTY）**：`repos` 中存在滿足任一條件的條目——(a) bare-name key 不等於 relay root 的 basename，且 relay root 下無同名子目錄；(b) `path` 為絕對路徑且 canonical 解析後位於 relay root 外（root 本身除外）；(c) `path` 指向不存在的目錄（例外：legacy 自身條目——key 等於 relay root basename 且 relative path 為同名裸名——代表 root 本身，SHALL NOT 判為 foreign）。relay root 下存在同名子目錄的 monorepo 條目 SHALL NOT 判為 foreign。
3. **stray 位置（DIRTY）**：relay.json 位於 `$HOME` 正下方；或位於非 workspace root 位置（`--scan` 模式下相對於掃描根）。
4. **legacy schema（INFO）**：頂層存在全域 `project_context` 或 `state_snapshot` 舊欄位。SHALL NOT 判為 dirty（D1/D5 唯讀 fallback 保證可用），報告 SHALL 提示可重新 init 升級。
5. 無任一 dirty 條件成立 SHALL 判為 CLEAN。

#### Scenario: foreign bare-name 條目判定為 dirty

- **GIVEN** relay root `/repos/AiToEarn` 的 relay.json 含 repo 條目 key `llm-stock-analyzer-integration`（bare-name、path 同名），且 root 下無該名稱子目錄
- **WHEN** doctor 檢查該檔
- **THEN** 判定 DIRTY，原因含 foreign 條目名稱

#### Scenario: monorepo 子目錄不誤殺

- **GIVEN** relay root `/repos/GraphifySDK` 的 relay.json 含 repo 條目 key `graphify-sdk-php`，且 `/repos/GraphifySDK/graphify-sdk-php/` 目錄存在
- **WHEN** doctor 檢查該檔
- **THEN** 判定不為 foreign（CLEAN 或僅 INFO）

#### Scenario: legacy 自身條目（stale bare path）不誤殺

- **GIVEN** relay root `/repos/AntigravityEnv` 的 legacy relay.json 含唯一 repo 條目 key `AntigravityEnv`、path `AntigravityEnv`（root 下無同名子目錄，workspace 內容在 root 本身）
- **WHEN** doctor 檢查該檔
- **THEN** 判定不為 foreign（僅 legacy INFO）
- **AND** `--fix` SHALL NOT 刪除該檔

#### Scenario: 絕對路徑指向 root 外判定為 dirty

- **GIVEN** relay root `/repos/A` 的 relay.json 含 repo 條目 `path: /repos/B/sub`
- **WHEN** doctor 檢查該檔
- **THEN** 判定 DIRTY，原因含該絕對路徑

#### Scenario: $HOME 正下方 stray 檔判定為 dirty

- **GIVEN** `/home/user/relay.json` 存在
- **WHEN** doctor 檢查該檔
- **THEN** 判定 DIRTY（stray：workspace 主權模型下必為跨 workspace 共用檔）

#### Scenario: legacy schema 僅提示不判死

- **GIVEN** relay.json 頂層有全域 `project_context`，所有 repo 條目均無 foreign 特徵
- **WHEN** doctor 檢查該檔
- **THEN** 判定 INFO（legacy schema），非 DIRTY
- **AND** `--fix` SHALL NOT 刪除該檔

### Requirement: --fix 刪除語意（opt-in）

`--fix` SHALL 只刪除判定為 DIRTY 的檔案：relay.json 本身與其同層 `.relay/` 鏡像目錄（若存在）。刪除前 SHALL 先輸出將刪清單；INFO 與 CLEAN 判定 SHALL 一律不動。`--fix` SHALL NOT 修改任何檔案內容（無就地修復、無 schema 遷移寫回）。

#### Scenario: fix 只刪 dirty

- **GIVEN** 掃描結果為 1 個 DIRTY、1 個 INFO（legacy schema）、1 個 CLEAN
- **WHEN** 執行 `--fix`
- **THEN** 僅 DIRTY 檔與其 `.relay/` 目錄被刪除
- **AND** INFO 與 CLEAN 檔位元組不變

#### Scenario: fix 刪除前列清單

- **GIVEN** 存在 2 個 DIRTY 檔
- **WHEN** 執行 `--fix`
- **THEN** 輸出先列出 2 個將刪路徑，再執行刪除並回報各檔結果

### Requirement: --scan 多 repo 掃描

`--scan <dir>` SHALL 以指定目錄為掃描根，找出其下（有限深度）所有 relay.json 與 `.relay/` 位置並逐檔套用檢查項準則。無 `--scan` 時 SHALL 只檢查當前 workspace root（沿用 relay root 解析規則：git toplevel 或 cwd）。

#### Scenario: 掃描多 repo

- **GIVEN** `/repos/` 下有 3 個 repo 各含 relay.json，其中 1 個為 foreign 污染
- **WHEN** 執行 `graphify handoff doctor --scan /repos`
- **THEN** 3 個檔全數列入報告，污染檔判定 DIRTY，其餘 CLEAN
- **AND** 退出碼為 1

### Requirement: 無狀態檔時視為乾淨

檢查目標（workspace root 或掃描結果）不含任何 relay.json 時，doctor SHALL 回報 "no relay state file"、判定 CLEAN，退出碼 0。SHALL NOT 提示 init 或自動建立任何檔案。

#### Scenario: 未 init 的 workspace

- **GIVEN** 某個 git repo 的 workspace root 無 relay.json
- **WHEN** 執行 doctor（無 `--scan`）
- **THEN** 回報 "no relay state file" 且退出碼 0
- **AND** 不建立任何檔案

### Requirement: 退出碼契約

doctor SHALL 回傳退出碼：0 = 無 DIRTY 發現；1 = 發現 DIRTY（含 `--fix` 執行後仍有殘留或刪除失敗）；2 = 執行錯誤（如掃描根不存在）。`--fix` 成功刪除所有 DIRTY 檔後 SHALL 回傳 0。

#### Scenario: CI 可用退出碼

- **GIVEN** 掃描發現 1 個 DIRTY 檔
- **WHEN** 執行 doctor（無 `--fix`）
- **THEN** 退出碼為 1
- **WHEN** 隨後執行 `--fix` 且刪除成功
- **THEN** 退出碼為 0
