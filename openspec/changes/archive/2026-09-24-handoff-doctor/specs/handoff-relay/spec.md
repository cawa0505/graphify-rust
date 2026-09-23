# handoff-doctor 變更 — handoff-relay Delta Spec

## Purpose

`relay_init` 於非 git 的 `$HOME` 本身建立 relay root 的行為由「警告不阻擋」改為「硬錯拒絕」。依據 2026-09-24 事故覆盤：從 `$HOME` cwd 啟動的 AI session 跑 `relayInit` 即寫出 `/home/zeng/relay.json`（時代 1 舊碼 + 時代 2 警告通道皆可重現），doctor 只能事後偵測，init 硬錯才是事前預防。

## MODIFIED Requirements

### Requirement: relay root 綁定於 workspace root

relay root SHALL 為 workspace root，且 SHALL 不執行任何向上（walk-up）搜尋：cwd 位於 git repo 內時，workspace root 為 `git rev-parse --show-toplevel`；非 git 目錄時為 cwd 本身。workspace root 無 relay 狀態檔時，relay 工具 SHALL 回明確錯誤並指引（`relay_init` 或 `GRAPHIFY_RELAY_ROOT` env override），不得靜默建立或共用狀態檔。設定 `GRAPHIFY_RELAY_ROOT` 時 SHALL 跳過 workspace root 解析，直接綁定該路徑。

`relay_init` SHALL 拒絕在非 git 的 `$HOME` 本身建立 relay root（凍結錯誤訊息：`refusing to init relay at $HOME; run inside a project directory or set GRAPHIFY_RELAY_ROOT`），SHALL NOT 寫入任何檔案。此拒絕 SHALL NOT 適用於：(a) `$HOME` 為 git repo（罕見但合法）；(b) `GRAPHIFY_RELAY_ROOT` 明確指向 `$HOME`（顯式表態優先）。

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

- **REMOVED** — 警告語意修訂為硬錯（下方「init 於非 git 的 $HOME 硬錯」scenario 取代）

#### Scenario: init 於非 git 的 $HOME 硬錯

- **GIVEN** 使用者在非 git 目錄的 `$HOME` 本身執行 `relay_init`，且未設定 `GRAPHIFY_RELAY_ROOT`
- **WHEN** init 解析 workspace root = cwd = `$HOME`
- **THEN** 回傳錯誤 `refusing to init relay at $HOME; run inside a project directory or set GRAPHIFY_RELAY_ROOT`
- **AND** `$HOME` 下 SHALL NOT 新增 relay.json、specs/、.code-relay/ 任何檔案

#### Scenario: $HOME 為 git repo 時允許 init

- **GIVEN** `$HOME` 本身為 git repo（含 `.git`）且無 relay.json
- **WHEN** 在 `$HOME` 執行 `relay_init`
- **THEN** init 正常成功（workspace root = git toplevel = `$HOME`）

#### Scenario: env override 指向 $HOME 時允許 init

- **GIVEN** `GRAPHIFY_RELAY_ROOT=/home/user`（目錄）已設定，`/home/user` 非 git 且無 relay.json
- **WHEN** 在任意目錄執行 `relay_init`
- **THEN** init 於 `/home/user` 成功（顯式表態優先於 $HOME 防線）
