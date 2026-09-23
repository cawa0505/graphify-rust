# relay-workspace-context — handoff-relay Delta Spec

## Purpose

relay 的身份推導從「server process cwd」改為「caller 傳入的 workspace context（absolute path）」。nexus gateway 拓撲下 stdio child 的 cwd 恆為 `$HOME`（systemd user unit 預設），MCP `tools/call` 不攜帶 caller 環境 —— 以 server cwd 推導身份在 gateway 下恆錯，且 cwd fallback 會讓任何成功 save 重建 `/home/zeng/relay.json` stray 檔（D4 情境復活通道）。

## MODIFIED Requirements

### Requirement: relay root 綁定於 workspace root

relay root SHALL 為 workspace root。workspace root 的解析來源 SHALL 為：(1) `GRAPHIFY_RELAY_ROOT` env override（顯式表態，優先級最高）；(2) caller 傳入的 workspace context `path` 參數 —— MCP 端必填（absolute），CLI 端 optional（缺省時以 process cwd 為 caller path）；(3) caller path 位於 git repo 內時為 `git rev-parse --show-toplevel`，否則為 caller path 本身。非 git 專案目錄作為 workspace root 合法（D4 模型）。`relay_init` 拒絕在非 git 的 `$HOME` 本身建立 relay root（凍結錯誤文：`refusing to init relay at $HOME; run inside a project directory or set GRAPHIFY_RELAY_ROOT`；`GRAPHIFY_RELAY_ROOT` env 表態或 `$HOME` 為 git repo 時豁免）。

MCP relay 工具（save/init/switch/resume/close/status）SHALL 要求 caller 傳入 `path`（absolute，必填）。未傳時 SHALL 回凍結錯誤 `workspace context required: pass the absolute path of your workspace` 且 SHALL NOT 寫入任何檔案 —— 絕不退回 server process cwd 推導身份（gateway 拓撲下 stdio child cwd 恆為 `$HOME`，退回即身分恆錯且重建 stray 檔）。CLI direct-spown 路徑不變更：process cwd 可用時維持現行為。

#### Scenario: 不再向上搜尋 relay 狀態檔

- **GIVEN** `/home/user/proj-a` 為 git repo 且其 root 無 relay.json，`/home/user/relay.json` 存在，MCP server 啟動於 `/home/user/proj-a/src`
- **WHEN** 呼叫任何 relay 工具
- **THEN** 嘗試綁定 workspace root `/home/user/proj-a`，不綁定 `/home/user/relay.json`
- **AND** 回傳的錯誤訊息指引先 `relay_init` 或設定 `GRAPHIFY_RELAY_ROOT`

#### Scenario: env override 刻意共用

- **GIVEN** `GRAPHIFY_RELAY_ROOT=/home/user/relay.json` 已設定
- **WHEN** MCP server 啟動於任意目錄並呼叫 relay 工具
- **THEN** 直接綁定 `/home/user/relay.json`，不執行 workspace root 解析

#### Scenario: init 於非 git 的 $HOME 時警告

- **REMOVED** — 警告語意修订为硬錯（下方「init 於非 git 的 $HOME 硬錯」scenario 取代）

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

### Requirement: repo 路徑寫入時驗證

`relay_save`/`relay_close` 寫入前 SHALL 把 repo 參數解析為實際目錄：絕對路徑 → root 相對 → caller path 相對；全部失敗 → fail-loud 拒寫，錯誤訊息 SHALL 列出嘗試路徑（`repo path could not be resolved. Tried: ...`）。解析基準 SHALL 為 caller workspace（root 與 caller path），SHALL NOT 以 server process cwd 為解析基準。錯誤訊息中的 Tried 候選 SHALL 去重（同一候選路徑 SHALL NOT 重複列出；重複列出即代表解析退回 cwd 同源，為回歸信號）。preserved：root 外的絕對路徑明確表態 → 允許；monorepo 子目錄不誤殺（同名目錄存在即合法）。

#### Scenario: 路徑無法解析時拒絕寫入

- **GIVEN** repo 名 "Foo" 在 root 下與 caller path 下皆無對應目錄
- **WHEN** 呼叫 `relay_save(repo="Foo", ...)`
- **THEN** 寫入被拒絕，狀態檔位元組不變
- **AND** 錯誤訊息包含所有嘗試過的路徑

#### Scenario: git 狀態取得失敗的可診斷輸出

- **GIVEN** repo 紀錄的 path 指向存在但 git 指令失敗的目錄
- **WHEN** 渲染該 repo 的 handoff
- **THEN** Status 顯示 `"(git status unavailable: <解析後路徑>)"`，不顯示誤導性的 "(not a git repo)"

#### Scenario: Tried 候選去重

- **GIVEN** 使用者 `Foo` 不是有效路徑，且 `Foo` 在 root 與 caller path 下解析為同一候選（caller path == root，CLI direct-spawn 常態）
- **WHEN** 呼叫 save 失敗時列出嘗試路徑
- **THEN** 錯誤訊息中該候選 SHALL 只出現一次
- **AND** 候選 SHALL 基於 caller workspace（root/caller path），SHALL NOT 依 server cwd 產生第四個同源候選

#### Scenario: gateway 拓撲下 repo 解析基於 caller path

- **GIVEN** nexus caller 儲存 repo 帶 `path=/mnt/.../repos/NexusHub`，repo 參數為 root 下相對名稱
- **WHEN** 解析 repo 目錄
- **THEN** 基準為 relay root（= caller path 的 git toplevel），成功時 `RepoState.path` 為 caller workspace 下的真實目錄
- **AND** 失敗時 Tried 候選不包含 server cwd 同源路徑

## Verification Evidence
