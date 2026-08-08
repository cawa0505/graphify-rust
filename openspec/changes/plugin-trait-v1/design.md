## Context

`docs/plugin_system.md` 已定義微核心 Plugin 架構（review / handoff / opendoc）與 `WorkspaceContext` 資料契約（§3.1）。GraphifyCore 目前無任何插件抽象，且維持「零 LLM/HTTP 依賴、同步」的硬性架構原則（AGENTS.md）。v1 只需落地最小 trait 契約，plugin 本體（handoff 等）留待後續 change。

## Goals / Non-Goals

**Goals:**
- 在 `graphify-core` 新增 `GraphifyPlugin` trait（4 方法）與 `WorkspaceContext`。
- 保持 graphify-core 零新依賴（僅 std + serde）。
- 提供 reference 實作（test 或 example）證明外部 crate 可實作。
- 同步更新 `docs/`（trait 規格 + crate 依賴圖）。

**Non-Goals:**
- 不實作任何具體插件（handoff / review / opendoc 本體）。
- 不引入 MCP 協定、動態載入（dylib/dlopen）或 plugin registry。
- 不修改既有 Node/Edge/GraphOutput schema。

## Decisions

1. **trait 放 `graphify-core` 而非獨立 crate**
   因為插件是「內嵌型 crate」（user 主旨明示無獨立 MCP），trait 定義必須放在被依附的核心，避免循環依賴。替代方案（獨立 `graphify-plugin-api` crate）被否決：v1 僅一個消費者，YAGNI。

2. **`sync_toon` 簽名採用 `Option<Vec<u8>>`**
   `Option` 讓插件可被動同步（收到外部 .toon）或被動要求輸出（`None` 時以綁定上下文自產），覆蓋 handoff「匯出子圖」與 opendoc「接收外部載荷」兩種方向。回傳 `Vec<u8>` 保持格式中立（.toon 位元組），不綁定 graphify-core 的 `toon.rs` 型別。

3. **`WorkspaceContext` 直接以欄位 struct 定義**
   與 `docs/plugin_system.md` §3.1 的介面契約逐欄對齊（`workspace_uuid` / `workspace_name` / `root_path` / `timestamp`），`timestamp` 用 `i64`（Unix epoch 秒）避免 chrono 依賴。

4. **reference 實作放在 `graphify-core/src/plugin.rs` 的 `#[cfg(test)]`**
   證明 trait 可實作且通過綁定/同步流程，同時避免在核心加入不必要的範例代碼路徑。測試需回傳 `Result` 並以 `?` 傳播（workspace 禁 `unwrap`/`expect`）。

## Risks / Trade-offs

- [trait 方法簽名過早定型] → v1 僅 4 方法，均對應 `docs/plugin_system.md` 已承諾的契約；日後擴充（如 `on_index_complete`）為 additive，不破壞既有實作。
- [`sync_toon` 回傳 `Vec<u8>` 語意模糊] → 由 spec 的 Scenario 鎖定「處理後輸出」，並在 rustdoc 註明方向語意；具體插件協議留待各插件 change 定義。
- [`get_workspace_uuid` 在未 bind 時回傳何值] → 契約要求 bind 後才有效；未 bind 時回傳空字串並在 rustdoc 標注，避免引入 `Result` 破壞最小簽名。

## Open Questions

無。所有未決項目（插件本體、動態載入、registry）已在 Non-Goals 明確排除。
