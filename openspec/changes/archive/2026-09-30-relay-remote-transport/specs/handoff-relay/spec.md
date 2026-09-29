# Delta Spec: relay-remote-transport

## MODIFIED Requirements

### Requirement: relay root 綁定於 workspace root

relay root SHALL 為 workspace root，且 SHALL 不執行任何向上（walk-up）搜尋。workspace root 的解析來源 SHALL 為：(1) `GRAPHIFY_RELAY_ROOT` env override（顯式表態，優先級最高）；(2) caller 傳入的 workspace context `path` 參數 —— MCP 端必填（absolute），CLI 端 optional（缺省時以 process cwd 為 caller path）；(3) caller path 位於 git repo 內時為 `git rev-parse --show-toplevel`，否則為 caller path 本身。非 git 專案目錄作為 workspace root 合法（D4 模型）。workspace root 無 relay 狀態檔時，relay 工具 SHALL 回明確錯誤並指引（`relay_init` 或 `GRAPHIFY_RELAY_ROOT` env override），不得靜默建立或共用狀態檔。

`relay_init` SHALL 拒絕在非 git 的 `$HOME` 本身建立 relay root（凍結錯誤訊息：`refusing to init relay at $HOME; run inside a project directory or set GRAPHIFY_RELAY_ROOT`），SHALL NOT 寫入任何檔案。此拒絕 SHALL NOT 適用於：(a) `$HOME` 為 git repo（罕見但合法）；(b) `GRAPHIFY_RELAY_ROOT` 明確指向 `$HOME`（顯式表態優先）。

MCP relay 工具（save/init/switch/resume/close/status）SHALL 要求 caller 傳入 `path`（absolute，必填）。未傳時 SHALL 回凍結錯誤 `workspace context required: pass the absolute path of your workspace` 且 SHALL NOT 寫入任何檔案 —— 絕不退回 server process cwd 推導身份（gateway 拓撲下 stdio child cwd 恆為 `$HOME`，退回即身分恆錯且重建 stray 檔）。CLI direct-spawn 路徑不變更：process cwd 可用時維持現行為。

root/path 解析失敗時，錯誤訊息 SHALL 可定位（本變更新增，design D4）：區分「path 不存在於服務所在機」、「path 存在但無 relay.json」、「寫入 IO 失敗」三種狀況，服務機 hostname 入錯誤訊息。

#### Scenario: 不再向上搜尋 relay 狀態檔

- **GIVEN** `/home/user/proj-a` 為 git repo 且其 root 無 relay.json，`/home/user/relay.json` 存在，MCP server 啟動於 `/home/user/proj-a/src`
- **WHEN** 呼叫任何 relay 工具
- **THEN** 嘗試綁定 workspace root `/home/user/proj-a`，不綁定 `/home/user/relay.json`
- **AND** 回傳的錯誤訊息指引先 `relay_init` 或設定 `GRAPHIFY_RELAY_ROOT`

#### Scenario: env override 刻意共用

- **GIVEN** `GRAPHIFY_RELAY_ROOT=/home/user/relay.json` 已設定
- **WHEN** MCP server 啟動於任意目錄並呼叫 relay 工具
- **THEN** 直接綁定 `/home/user/relay.json`，不執行 workspace root 解析

#### Scenario: init 於非 git 的 $HOME 硬錯

- **GIVEN** 使用者在非 git 目錄的 `$HOME` 本身執行 `relay_init`，且未設定 `GRAPHIFY_RELAY_ROOT`
- **WHEN** init 解析 workspace root = caller path = `$HOME`
- **THEN** 回傳錯誤 `refusing to init relay at $HOME; run inside a project directory or set GRAPHIFY_RELAY_ROOT`
- **AND** `$HOME` 下 SHALL NOT 新增 relay.json、specs/、.code-relay/ 任何檔案

#### Scenario: $HOME 為 git repo 時允許 init

- **GIVEN** `$HOME` 本身為 git repo（含有效 `.git` 結構）且無 relay.json
- **WHEN** 在 `$HOME` 執行 `relay_init`
- **THEN** init 正常成功（workspace root = git toplevel = `$HOME`）

#### Scenario: env override 指向 $HOME 時允許 init

- **GIVEN** `GRAPHIFY_RELAY_ROOT=/home/user`（目錄）已設定，`/home/user` 非 git 且無 relay.json
- **WHEN** 在任意目錄執行 `relay_init`
- **THEN** init 於 `/home/user` 成功（顯式表態優先於 $HOME 防線）

#### Scenario: nexus gateway caller 傳 path

- **GIVEN** nexus 管理的 graphify-mcp 啟動於非 git 的 `$HOME`（systemd cwd），caller 呼叫 save 帶 `path=/mnt/.../repos/NexusHub`
- **WHEN** save 解析 relay root
- **THEN** root 落在 `/mnt/.../repos/NexusHub` 的 git toplevel
- **AND** relay.json 寫入該 workspace，`RepoState.path` 為真實目錄（絕對路徑）
- **AND** 渲染診斷顯示該 repo 的真實 git commit 與 project_context

#### Scenario: MCP 未傳 path 時凍結錯誤且零寫入

- **GIVEN** graphify-mcp 啟動於非 git 的 `$HOME`，caller 未傳 `path`
- **WHEN** 呼叫任何 relay MCP 工具
- **THEN** 回錯誤 `workspace context required: pass the absolute path of your workspace`
- **AND** `$HOME` 下 SHALL NOT 新增 relay.json、specs/、.code-relay/、.relay/ 任何檔案

#### Scenario: CLI direct-spawn 不回歸

- **GIVEN** 使用者在 git repo 內直接執行 `graphify handoff`（無 path 參數）
- **WHEN** handoff 解析 workspace root
- **THEN** 維持現行為（process cwd 的 git toplevel），零行為變更

#### Scenario: caller path 不存在於服務所在機（gateway 拓撲除錯訊號）

- **GIVEN** graphify-mcp 服務跑在主機 A，caller 傳入的主機 B 路徑在 A 上不存在
- **WHEN** 呼叫任一 relay 工具（`relay_status` / `relay_init` / `relay_save` …）
- **THEN** 回錯 `workspace path not found on this host <A-hostname>: <path>`
- **AND** 不回 NoRoot 文案、不回裸 io error

#### Scenario: path 存在但尚未初始化

- **GIVEN** path 是服務所在機上的有效 git workspace、無 relay.json
- **WHEN** 呼叫 `relay_status`
- **THEN** 回 `No relay.json found at <root> — run relayInit first`（帶實際 root）

#### Scenario: init 寫入 IO 失敗

- **GIVEN** path 存在但不可寫（權限/磁碟）
- **WHEN** 呼叫 `relay_init`
- **THEN** 回 `init failed: <io error>`，不裸回 `io: Permission denied`

## ADDED Requirements

### Requirement: HA 拓撲下 relay 身份不漂移

NexusHub HA（gateway caddy → node1/node2/serve-host）任一 nexus 節點接手時，relay
工具的身份 SHALL 不隨節點漂移：relay.json / specs/ / registry DB 一律讀寫
**服務所在機**（workspace 機）的檔案系統，stdio 節點不再持有 relay 狀態。

#### Scenario: HA failover 後 relay 狀態連續

- **GIVEN** relay baton 儲存在 serve-host，nexus primary 從 node1 failover 到 node2
- **WHEN** caller 呼叫 `relay_status`
- **THEN** 看到的 baton / repos / spec-drift 與 failover 前一致（同一份
  serve-host 檔案系統）
