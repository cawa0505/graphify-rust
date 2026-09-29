# Delta Spec: relay-remote-transport（graphify-tui 能力）

## ADDED Requirements

### Requirement: TUI remote 模式（--remote 選 server）

`graphify tui` SHALL 支援兩種資料來源模式：(a) **local**（預設，現行契約零回歸
——直讀本機 `.toon` 圖譜，workspace host 上無感）；(b) **remote**——`--remote
<URL>` 旗標或 `GRAPHIFY_REMOTE` env 指向 graphify 服務，經同一 streamable HTTP
端點查詢（與 nexus 共用同一份狀態）。旗標 SHALL 覆蓋 env。

#### Scenario: local 模式不回歸

- **GIVEN** 在 workspace host 本機、無 `--remote` 無 `GRAPHIFY_REMOTE`
- **WHEN** 執行 `graphify tui`
- **THEN** 行為與本變更前完全一致（直接載入本機 `.toon`，不發網路請求）

#### Scenario: remote 查詢

- **GIVEN** `graphify tui --remote http://192.0.2.10:9899`（或等值 env）
- **WHEN** 使用者在 TUI 內檢索節點 / trace path / summary
- **THEN** 查詢經 streamable HTTP 送達 graphify 服務，由服務所在機的圖譜回答

#### Scenario: remote 不可達

- **GIVEN** `--remote` 指向的服務未啟動
- **WHEN** TUI 啟動
- **THEN** 顯示連線失敗與 server URL（可定位的錯誤），不靜默退化為 local 圖譜
