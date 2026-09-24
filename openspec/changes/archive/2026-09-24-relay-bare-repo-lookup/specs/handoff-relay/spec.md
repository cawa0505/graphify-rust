## MODIFIED Requirements

### Requirement: repo 路徑寫入時驗證

`relay_save`/`relay_close` 寫入前 SHALL 把 repo 參數解析為實際目錄：絕對路徑 → 裸名先回查 relay state 已註冊的同名 repo 路徑 → root 相對 → caller path 相對；全部失敗 → fail-loud 拒寫，錯誤訊息 SHALL 列出嘗試路徑（`repo path could not be resolved. Tried: ...`）。已註冊回查 SHALL 僅採用解析後目錄仍存在者：未註冊或已註冊路徑失效 SHALL 回退後續層。解析基準 SHALL 為 caller workspace（root 與 caller path），SHALL NOT 以 server process cwd 為解析基準。錯誤訊息中的 Tried 候選 SHALL 去重（同一候選路徑 SHALL NOT 重複列出；重複列出即代表解析退回 cwd 同源，為回歸信號）。preserved：root 外的絕對路徑明確表態 → 允許；monorepo 子目錄不誤殺（同名目錄存在即合法）。候選僅需為存在的目錄（repo 是否為 git repo 不影響寫入驗收，僅影響渲染時的 git 狀態診斷）。解析成功時 `RepoState.path` SHALL 儲存絕對路徑，不得以裸 repo 名稱作為路徑預設值。

#### Scenario: 裸名回查已註冊 repo 命中

- **GIVEN** state 已註冊 repo "NexusHub"（`path` 為絕對路徑且目錄存在），root 下無同名子目錄
- **WHEN** 呼叫 `relay_save(repo="NexusHub", ...)`
- **THEN** 解析命中已註冊路徑，寫入成功
- **AND** `RepoState.path` 為該已註冊絕對路徑

#### Scenario: 已註冊路徑失效時回退既有解析

- **GIVEN** state 已註冊 repo "Foo" 但其 `path` 目錄已不存在，root 下亦無 `root/Foo`
- **WHEN** 呼叫 `relay_save(repo="Foo", ...)`
- **THEN** 回退既有三層解析並 fail-loud 拒寫，錯誤訊息列出嘗試路徑
- **AND** 狀態檔不寫入失效的已註冊路徑

#### Scenario: 路徑無法解析時拒絕寫入

- **GIVEN** repo 名 "Foo" 在 root 下與 caller path 下皆無對應目錄，且 state 未註冊同名 repo
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
