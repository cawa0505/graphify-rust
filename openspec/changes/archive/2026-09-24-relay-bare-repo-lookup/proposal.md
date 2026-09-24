# relay-bare-repo-lookup — 裸名 repo 先回查已註冊 path

## Why

`resolve_repo_path` 只做三層目錄候選（絕對路徑 → root 相對 → caller 相對），不回查 relay state 裡已註冊的 `name → path`。對「記得 repo 名的呼叫端」（如 gateway 測試中 `repo="NexusHub"`）是 UX 地雷：解析成 `root/NexusHub/NexusHub` 失敗，只有省略 repo 或傳絕對路徑才過。已註冊的 repo 有權威絕對 path，裸名理應直接命中。

## What Changes

`relay_save` 的裸名（非絕對路徑）參數解析前，先查 state 已註冊的同名 repo：`repo_dir_of` 解析出路徑且目錄存在 → 直接採用；未註冊或路徑失效 → 回退既有三層解析（fail-loud 語意與 Tried 去重不變）。純解析面修正，MCP/CLI schema 零變更。

## Capabilities

### Modified Capabilities
- `handoff-relay` — 「repo 路徑寫入時驗證」解析序新增已註冊回查層

## Impact

- `graphify-plugin-handoff/src/relay.rs`（relay_save 解析 + helper + 測試）
- CLI/MCP 呼叫端同時受益；無 schema 變更
