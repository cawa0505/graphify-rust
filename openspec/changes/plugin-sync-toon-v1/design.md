# plugin-sync-toon-v1 — Design

## Context

`sync_toon` 簽名已於 plugin-trait-v1 定案（`Option<Vec<u8>> -> Vec<u8>`，見 proposal.md - Why）。graphify 以 .toon 為 Single Source of Truth（#2522/#2524），且 `graphify-core` 有零新增依賴約束（#3088）。本設計僅凍結封包格式，不動 trait 簽名。

## Goals / Non-Goals

**Goals**
- 定義 sync_toon 封包的版本承載（metadata 內 `format_version`）。
- 讓 .toon 文件同時承載路由鍵（`workspace_key`）與可選負載（symbol_nodes / graph_topology）。
- 純文件變更，無代碼、無新依賴。

**Non-Goals**
- 不引入封包 envelope / framing（如 magic bytes、length prefix）——in-process 呼叫無需傳輸層封裝；外部 SDK 的 framing 屬於 plugin-host-mcp 範疇（D1 協議）。
- 不定義 .toon 序列化格式本身（既有 TOON 規格負責）。
- 不實作版本協商邏輯——v1 僅定義解析端拒絕行為。

## Decisions

**D1: 封包 = .toon 文件本體，而非 envelope**
- 選項 A（採用）：payload 直接是 .toon 序列化，版本與路由鍵放 metadata。
- 選項 B（捨棄）：自訂 envelope（magic + version + length + payload）。
- 理由：in-process 呼叫無 framing 需求；用 .toon 既有 metadata 承載版本，零新結構；外部 SDK 需 framing 時在 plugin-host-mcp 層加，不污染核心契約。

**D2: 版本欄位放 metadata 內 `format_version`，而非 .toon 格式版本**
- 選項 A（採用）：`format_version` 描述封包契約版本（本規格 v1 = "1.0.0"）。
- 選項 B（捨棄）：直接拿 .toon 序列化格式版本當契約版本。
- 理由：.toon 格式版本描述序列化語法，sync_toon 契約版本描述承載語意；兩者演進節奏不同，分開較清晰。

**D3: 錯誤以 `error` metadata 表達，不用 Result / panic**
- 選項 A（採用）：無法產出有效輸出時回傳含 `error` metadata 的 .toon。
- 選項 B（捨棄）：改 trait 簽名為 `Result<Vec<u8>>`。
- 理由：簽名已凍結（plugin-trait-v1），改簽名即破壞性變更；`error` metadata 不影響既有簽名且對 MCP 視圖友善。

**D4: optional 負載對齊 §3.2 Standard Plugin Communication Protocol**
- 採用：`symbol_nodes` / `graph_topology` 沿用 docs/plugin_system.md §3.2 的既有視圖欄位名，避免同一資料兩種命名。
- 理由：文件已定義此視圖，命名一致降低插件作者認知負擔。

## Risks / Trade-offs

- [無 framing，外部 SDK 需自行補傳輸層] → 已在 Non-Goals 明示；plugin-host-mcp（D1）承接 framing 與 JSON-RPC，封包語意不變。
- [`error` metadata 無標準欄位結構（字串？結構？）] → v1 以字串描述；若未來需要結構化錯誤，升 minor 版本補充，不破壞 v1 消費端。
- [MAJOR 不符時「可拒絕」為 MAY 而非 MUST] → 保留彈性給插件自訂降級策略；核心解析端文件明示建議拒絕。

## Migration Plan

- 純文件變更，無部署步驟、無回滾需求。
- `docs/plugin_system.md` §3.2 與 `docs/core.md` 同步補上封包契約說明，維持文件與規格一致。

## Open Questions

- [待討論] 是否需要在 graphify-core 提供一個 `parse_sync_toon_packet(&[u8])` helper 供插件共用（解析 metadata + 驗證 MUST 欄位）？v1 純規格不實作，留待第一個實際消費插件（handoff）出現時再定。
