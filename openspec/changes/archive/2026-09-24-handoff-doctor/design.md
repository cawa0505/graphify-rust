# Design: handoff-doctor

## Context

relay-multi-repo-isolation（已歸檔）上線後的清理工作產生了實證驗屍準則（見 archived change 的 `cleanup.md` 與當時的一次性 Python script）。本 change 把該準則產品化為 `graphify handoff doctor`。實作位置：graphify-plugin-handoff（relay 生態的單一歸屬地）+ graphify-cli 接線。

## Goals / Non-Goals

- **Goals**：唯讀報告預設、`--fix` opt-in 刪除、判定準則剛性可測、退出碼 CI 可用。
- **Non-Goals**：MCP tool 面（doctor 是人類操作者工具）；schema 就地遷移寫回（升級路徑是重新 init，不是 doctor 改檔）；registry（graphify.db）檢查（DB 層本就按 workspace_key 隔離，無清理需求）。

## Decisions

### D1：模組位置與檔案切分

`graphify-plugin-handoff/src/doctor.rs` 單檔起步；若超過 300 行上限，切 `doctor/mod.rs`（入口 + 報告組裝）+ `doctor/checks.rs`（純判定函式）。判定函式全部設計為純函式（輸入路徑與 JSON 值、輸出判定），測試不需要真實檔案系統的案例走純函式，檔案系統案例走 tempdir。

### D2：判定核心資料流

```
fn check_relay_file(path, root) -> Vec<Finding>
struct Finding { level: Dirty|Info|Clean, reason: String, evidence: Vec<String> }
```

- 輸入 JSON 直接用 `serde_json::Value` 讀（doctor 不依賴 `RelayState` 的強型別——髒檔常常正是 schema 不合的檔，強型別反序列化會在解析層就失敗，失去「指出哪裡髒」的能力）。unparseable 判定即來自此層。
- foreign 判定三條件（bare-name 無本地子目錄 / 絕對路徑出 root / path 不存在）各自獨立檢查、逐一列證據，一檔多髒時全部列出（不短路）。
- `$HOME` 判定用 `std::env::var("HOME")`；env 不可用時跳過 stray-位置檢查（不誤報）。

### D3：--fix 刪除範圍與安全邊界

刪除集合 = DIRTY 檔本身 + 同層 `.relay/` 目錄。**不刪** `.relay/` 之外的任何伴隨檔（如 `RESUME.md`、`next_step.md`——那些是渲染產物，留給使用者）。刪除前先收集完整將刪清單並輸出，再逐一刪除；單檔刪除失敗不中斷其餘刪除，最後彙總失敗清單並回退出碼 1。無 `--fix` 時整條刪除路徑不執行（型別上以 `Option<Vec<PathBuf>>` 區分，非 boolean flag 散落判斷）。

### D4：--scan 掃描策略

`--scan <dir>` 用 `walkdir`（workspace 已有依賴則沿用；若無，手寫兩層 readdir 即可——掃描深度上限 3 層，避免 walkdir 新依賴，遵守「stdlib 優先」）。命中條件：目錄下有 `relay.json` 或 `.relay/`。掃描根本身若就是 relay root（有 relay.json）也納入檢查。排除 `target/`、`node_modules/`、隱藏目錄（`.git`、`.relay` 本身除外——`.relay` 內不遞迴找，只在 `.relay` 存在時記錄為鏡像伴隨）。

### D5：輸出格式

人類可讀為主（與 relay 工具現有輸出風格一致）：

```
[DIRTY] /repos/AiToEarn/relay.json
  - foreign repo "llm-stock-analyzer-integration" (path=llm-stock-analyzer-integration, no such dir)
[INFO]  /repos/Saaslab/relay.json
  - legacy global schema (project_context present) — consider relayInit to upgrade
[CLEAN] /repos/Draco/relay.json
```

無機器可讀輸出（`--json`）——目前無消費者，YAGNI；需要時再加。

### D6：CLI 接線

`HandoffCommand::Doctor { #[arg(long)] fix: bool, #[arg(long)] scan: Option<PathBuf> }` → `run_handoff` 直接呼叫 `doctor::run(DoctorArgs)`。doctor 不需要 `RelayPlugin::bind`（不依賴 relay root 綁定流程，自己解析 workspace root 或用 scan 根），但放在 handoff 子命令下保持生態歸屬。

## Risks / Trade-offs

- **誤殺風險**：判定準則以「存在性」為證據（子目錄存在 = 合法 monorepo），不猜語意。真實清理案例（8 個 flagged、1 個 monorepo 誤殺修正）已校準準則，spec 情境直接取自實證。
- **`--fix` 破壞風險**：opt-in + 前置清單 + 只刪 DIRTY 三層防護；INFO（legacy）永不刪。
- **掃描深度**：上限 3 層是效能與安全折衷（repos 目錄結構平淺）；`[待討論]` 若未來有更深巢狀需求再放寬。

## Migration Plan

純新增工具，無遷移。上線後 `cleanup.md` 的手動準則由 doctor 取代（archived 文件保留歷史，不回改）。

## Open Questions

- 掃描深度上限 3 層是否足夠 → `[待討論]`（預設 3，可先實作再依實際 repos 結構調整）。
