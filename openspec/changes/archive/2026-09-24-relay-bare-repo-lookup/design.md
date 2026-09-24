# Design — relay-bare-repo-lookup

## D1 層序

layer 0（已註冊回查）只適用**非絕對路徑**參數；絕對路徑維持第一順位（明確表態優先）。命中條件三者俱備：`state.repos` 同名 key、`repo_dir_of` 解析出路徑、該目錄 `is_dir()`。未命中或失效 → 既有三層解析，Tried 去重語意不變。

## D2 讀取來源

helper 每次 `state::load` fresh 讀盤（relay.json 為小檔），不吃 bind 快取 — 跨 process 已註冊的 repo 也能命中，避免 bind 後他程序才寫入的漏命中。

## D3 死路徑

已註冊但目錄消失 → 不採用、不寫入，回退解析；失敗時 Tried 只列三層候選（失效註冊路徑不列入，避免誤導「已試過存在的已註冊路徑」）。

## D4 面界

純 `relay_save` 內部解析；`relay_close`/`status`/`switch` 本就以 name 查 state，不經 `resolve_repo_path`，不受影響。MCP/CLI schema 零變更。

## 已定案

（無 [待討論]）
