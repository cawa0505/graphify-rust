# Tasks

## 1. Feasibility spike（先驗證外部契約）

- [x] 1.1 box-of-rain spike：建立範例 JSON（兩個容器 box + 跨容器 connection），`npx box-of-rain` stdin 驗證 ASCII 與 `--svg` 輸出契約、Zod 錯誤輸出格式。產出：spike 筆記附加於本 change（驗證：terminal 實跑截輸出）。
  - 結論（2026-09-21）：ASCII/SVG 契約驗證通過；所有錯誤均 exit 0（含 malformed JSON），部分靜默空盒——對策記錄於 design.md D4。
- [x] 1.2 確認 YAML crate 選擇：檢查 `serde_yaml`（archived?）vs `serde_yml` 維護狀態與 API 相容性，定案並記錄於 design.md D6（驗證：`cargo add --dry-run` + docs.rs 紀錄）。
  - 定案：`serde_yaml_ng` 0.10（serde_yaml 0.9 API 相容 fork；serde_yaml 已 archived，serde_yml 版本號躁進），記錄於 design.md D6。

## 2. Manifest 型別與解析（graphify-core + graphify-cli）

- [x] 2.1 `graphify-core` 新增 manifest 資料型別（`AssemblyManifest` / `WorkspaceEntry` / `RelationEntry`，serde derive，無解析器依賴），含 workspace id 不得含 `::` 的格式約束（驗證：單元測試 `test_manifest_types_roundtrip`）。
- [x] 2.2 `graphify-cli/src/compose/manifest.rs`：YAML 解析 + validate 語意（路徑存在、.toon/.json fallback、relation 端點、節點引用存在性；錯誤逐一列出，不靜默跳過）（驗證：單元測試涵蓋 spec 三個 validate scenarios，`cargo test -p graphify-cli compose::manifest`）。

## 3. Unified graph 合併與 CLI

- [x] 3.1 `graphify-cli/src/compose/merge.rs`：載入各 workspace graph（.toon 優先、.json fallback + legacy schema 遷移）、節點 id 冠 `ws::` 前綴、跨域 relation 加為 composition edge（驗證：單元測試 `test_merge_two_workspaces`、`test_merge_dedupes_by_prefix`）。
- [x] 3.2 CLI `compose` 子指令：`validate` / `graph` / `render [--svg]` 分派接線進 `Commands` enum（驗證：`graphify compose validate <fixture>` 實跑 + `--help` 顯示）。

## 4. box-of-rain 投影渲染

- [x] 4.1 `graphify-cli/src/compose/render.rs`：unified graph → box-of-rain JSON（workspace=容器 box、節點=葉 box、跨域 edge=connection+label）、stdin JSON → `npx box-of-rain`、spawn 失敗/非零退出回報明確錯誤（驗證：投影 JSON 單元測試 + spike fixture 端到端實跑出 ASCII 圖）。

## 5. MCP authoring tools

- [x] 5.1 graphify-mcp 註冊 `graphify_compose_read` / `graphify_compose_render`（唯讀，與 CLI 共用 compose 模組邏輯）（驗證：MCP tool 呼叫回傳 manifest 結構/ASCII 圖）。
- [x] 5.2 `graphify_compose_write`：驗證先行 + 暫存檔原子寫入（rename），驗證失敗時 manifest 位元組不變（驗證：單元測試 spec 兩個 write scenarios，含失敗不可變性檢查）。

## 6. 整合驗證與收尾

- [x] 6.1 端到端驗收 narrative：多 workspace fixture（≥2 個 workspace、≥1 條 relation）跑 validate → graph → render 全鏈，輸出保留為驗證證據（驗證：terminal 輸出記錄至 change 目錄 `verification.md`）。
- [x] 6.2 `cargo fmt` + `cargo clippy`（all+pedantic）+ 全測試綠、零警告（驗證：`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace`）。
