# handoff-doctor 驗證證據（2026-09-24）

## 單元測試（spec 情境剛性綁定）

```
cargo test -p graphify-plugin-handoff
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
```

- doctor 情境 15 測試：foreign bare-name / monorepo 不誤殺 / legacy 自身條目（stale bare path）不誤殺 / 絕對路徑 root 外 / unparseable / $HOME stray / scan 孤兒（git repo 非 toplevel）/ legacy INFO（`fresh()` 空欄位不誤報）/ 多髒全列 / fix 只刪 dirty（INFO、CLEAN 位元組不變）/ fix 前列清單 / scan 多 repo 退出碼 1 / no-state 退出碼 0 / fix 後退出碼契約 / 掃描根不存在
- 檔案切分：`doctor/checks.rs` 187 行、`doctor/mod.rs` 135 行、`doctor/tests.rs` 299 行（300 限內）

## clippy / fmt（兩 workspace 零警告）

```
cargo clippy -p graphify-plugin-handoff --all-targets → 0 warnings
cargo clippy -p graphify-cli --all-targets → 0 warnings
cargo fmt -- --check → OK（兩 workspace）
cargo test -p graphify-cli → 24 passed
```

## e2e 實跑（graphify-cli release 行為，臨時 fixture + 實際 repos 環境）

### 退出碼契約

```
$ graphify handoff doctor                 # 未 init 的 workspace
no relay state file
exit=0                                    # 零寫入（read_dir 計數不變）

$ graphify handoff doctor --scan /tmp/doctor-e2e
[DIRTY] /tmp/doctor-e2e/dirty-repo/relay.json
  - foreign repo "ghost" (path=ghost, no such dir)
exit=1

$ graphify handoff doctor --scan /tmp/doctor-e2e --fix
will delete:
  /tmp/doctor-e2e/dirty-repo/relay.json
  /tmp/doctor-e2e/dirty-repo/.relay
fixed: 2 deleted, 0 failed
exit=0                                    # INFO/CLEAN 檔位元組不變
```

### 實際 repos 環境 --scan（今晚人工清理後現況，唯讀）

```
$ graphify handoff doctor --scan ~/homelab-integration/repos
[INFO] .../AntigravityEnv/relay.json   - legacy global schema (project_context present) ...
[INFO] .../BloggerAgent/relay.json     （同上）
[INFO] .../CLIProxyAPI/relay.json      （同上）
[INFO] .../Draco/relay.json            （同上）
[INFO] .../GraphifySDK/relay.json      （同上）
[INFO] .../LoomCowork/relay.json       （同上）
[INFO] .../OpenDocuments/relay.json    （同上）
[INFO] .../OpenSlide/relay.json        （同上）
[INFO] .../TeletranRoute/relay.json    （同上）
EXIT=0
```

9 個殘留檔全數 INFO（legacy schema 提示升級、`--fix` 不刪）、0 DIRTY — 與人工清理結論
（21 個髒檔已刪、9 個合法保留）完全一致。

### e2e 抓到的誤殺暨修正（spec 修訂紀錄）

首版 cond (c) 把 legacy 自身條目（key == root basename、path 為同名裸名、root 下無同名
子目錄 — 舊碼記法，代表 root 本身）誤判為 foreign DIRTY，8 個合法 workspace 狀態檔差點
被 `--fix` 刪除。修法：spec 檢查項 2(c) 增補自身條目豁免 + 新增 scenario「legacy 自身條
目（stale bare path）不誤殺」+ 剛性測試 `legacy_self_entry_with_stale_bare_path_not_killed`。
修後重掃：EXIT=0、全 INFO，零誤殺。
