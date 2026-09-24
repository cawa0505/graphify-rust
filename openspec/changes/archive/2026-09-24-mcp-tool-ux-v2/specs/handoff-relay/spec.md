## Purpose

為 relay 邊界操作（close / switch / init 遇既有狀態）建立自動狀態保存（auto-save）機制，確保尚未顯式 relay_save 的現況在 session 邊界不會流失。

## ADDED Requirements

### Requirement: Relay auto-save at session boundaries

relay 邊界操作（close、switch、init 遇既有狀態）SHALL 先自動執行 relay_save 等價 flush（更新目標 repo 的 `last_updated` 並渲染 RESUME 快照）再進行後續操作。自動 save 為無條件冪等 flush —— 「若尚未 save」在 plugin 層無 saved-marker 可判別，以冪等重複 flush 實作（重複執行僅刷新 `last_updated` 與 RESUME 渲染，無資料損失）。自動 save 失敗時 SHALL fail-loud 回傳該錯誤並中止後續操作，不得默默吞錯。

現況引用（以讀碼結果為準）：MCP close arm（`graphify-mcp` 的 `graphify_relay_close`）已具 save-then-close，但僅在呼叫端帶 state params（role/phase/volatile/conf/debt/kind）時觸發；plugin 層 `relay_close` 原先只在 close ritual 內寫 `next_session_starter`、`relay_switch` 原先只切 `active_baton`，兩者皆無自動 save，由本 requirement 補齊。

#### Scenario: relay close auto-saves before the close ritual

- **WHEN** relay close 被呼叫（目標 repo = `repo` 參數，缺省為目前 workspace repo）
- **THEN** 系統 SHALL 先自動執行 relay_save 等價 flush（目標 repo、含 `next` 寫入與 RESUME 渲染）再執行 close ritual（consistency check、spec diff、next_step.md、commit、handoff snapshot）
- **AND** 自動 save 失敗時 SHALL 回傳該錯誤且不執行 close ritual

#### Scenario: relay switch auto-saves before the baton handoff

- **WHEN** relay switch 被呼叫且目標 repo 已註冊（驗證通過）
- **THEN** 系統 SHALL 於切換 baton 前先自動執行 relay_save 等價 flush（SaveArgs 預設：對齊 relay_save 無參時的 `last_updated` / RESUME render 語意，flush 對象為目前 workspace repo）
- **AND** 自動 save 失敗時 SHALL 中止切換、回傳該錯誤且不得切換 baton（fail-loud，不得默默吞錯）

#### Scenario: relay init flushes existing state before refusing

- **WHEN** relay init 被呼叫且 `resolve_root` 已找到既有 relay root（既有未完成 relay 狀態）
- **THEN** 系統 SHALL 先自動執行 relay_save 等價 flush（flush 現況 + render RESUME 快照）至該既有 root
- **AND** init SHALL 仍回傳 RootExists 拒絕（既有狀態不得被 init 的 fresh 建立覆寫；D4 防護語意不變）
- **AND** 自動 save 失敗時 SHALL 回傳該 save 錯誤（fail-loud）
