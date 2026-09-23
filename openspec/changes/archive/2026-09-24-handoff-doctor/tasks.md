# Tasks: handoff-doctor

## 1. doctor 核心模組（graphify-plugin-handoff）

- [x] 1.1 `src/doctor.rs`：`Finding`/`DoctorReport` 型別與 `check_relay_file(path, root) -> Vec<Finding>` 純判定函式（serde_json::Value 讀檔；unparseable/foreign 三條件/stray/legacy 四類檢查，多髒全列不短路）（驗證：單元測試涵蓋 spec 五個判定 scenarios；純函式案例不需檔案系統）※實作為 `doctor/checks.rs`（Serde 弱型別純判定）+ `doctor/mod.rs`（Report 組裝）
- [x] 1.2 刪除路徑：`--fix` 將刪清單收集（DIRTY 檔 + 同層 `.relay/`）→ 前置輸出 → 逐一刪除（單檔失敗不中斷、彙總結果）；INFO/CLEAN 永不入清單（驗證：tempdir 測試「fix 只刪 dirty」「fix 刪除前列清單」兩 scenarios；無 fix 時零寫入驗證）※`deletion_targets` + `apply_fix` 回傳失敗清單
- [x] 1.3 `--scan` 掃描（≤3 層、排除 target/node_modules/隱藏目錄）+ workspace root 解析（git toplevel / cwd，沿 `root.rs` 既有 helper）；無狀態檔回報 "no relay state file"（驗證：`--scan` 多 repo scenario 測試；未 init workspace scenario 測試）※stdlib 手寫 stack 掃描不引 walkdir

## 2. CLI 接線（graphify-cli）

- [x] 2.1 `HandoffCommand::Doctor { fix, scan }` 變體 + `run_handoff` 分派；退出碼契約（0/1/2）經 `std::process::exit` 或錯誤型別映射落地（驗證：`graphify handoff doctor --help` 實跑；臨時目錄 fixture 實跑退出碼 0/1/2 三案例）※e2e 實跑：no-state→0、dirty --scan→1、fix 成功→0、非 git repo 內散置 relay.json 由 render 端去重顯示

## 3. 測試與規格綁定

- [x] 3.1 spec 情境全量測試（5 個判定 + 2 個 fix + 1 個 scan + 1 個 no-state + 退出碼 2 案例），名稱對應 scenario 語意（驗證：`cargo test -p graphify-plugin-handoff doctor` 全綠）※14 測試：foreign/monorepo/絕對路徑外/unparseable/$HOME stray/orphan scan/legacy INFO（fresh 空欄位不誤報）/多髒全列/fix 只刪 dirty/fix 前列清單/scan 多 repo/no-state/fix 後退出碼/掃描根不存在
- [x] 3.2 檔案切分檢查：doctor 模組 ≤300 行（必要時切 `doctor/mod.rs` + `doctor/checks.rs`）（驗證:`wc -l`）※checks 179 / mod 132 / tests 264，全部 300 限內

## 4. 收尾

- [x] 4.1 `cargo fmt` + `cargo clippy`（all+pedantic）+ 兩 workspace 全測試綠、零警告；e2e 實跑：以今晚清理後的實際 repos 環境跑 `--scan`，核對與清理後現況一致（驗證：`verification.md`）※e2e 抓到 legacy 自身條目誤殺，spec 修訂 + 剛性測試後重掃：9 檔全 INFO、0 DIRTY、EXIT=0
- [x] 4.2 openspec validate --strict 通過；spec sync 為主 spec `handoff-doctor`（含驗證證據）+ `handoff-relay` MODIFIED（$HOME 硬錯）已同步 + archive
