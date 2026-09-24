# relay-bare-repo-lookup Tasks

## 1. 實作

- [x] 1.1 `relay_save` 裸名參數插入 layer-0 已註冊回查（helper `registered_dir_for`，fresh 讀盤）
- [x] 1.2 測試：命中已註冊外部路徑（root 下無同名子目錄）、已註冊死路徑回退 fail-loud、未註冊維持既有行為

## 2. 收尾

- [x] 2.1 `cargo test` / `clippy` / `fmt` 全綠（GraphifyPlugins + GraphifyRust）
- [x] 2.2 `openspec validate --strict` → 歸檔 → 主 spec `handoff-relay` sync → 雙 repo commit/push → 雙機部署 + e2e 指紋驗證
