# workspace-compose 變更 — Delta Spec

## Purpose

定義跨 workspace 組裝（Assembly）能力：以 YAML Assembly Manifest 描述多個 workspace 之間的語意關聯，將各 workspace 既有的 AST graph（`graphify-out/graph.toon`）合併為 unified graph，並投影為 box-of-rain ASCII/SVG 圖。Manifest 是「組裝說明書」而非系統唯一真相；同一批 workspace 可存在多份 manifest 代表不同視角。

## ADDED Requirements

### Requirement: Assembly Manifest 格式
系統 SHALL 支援 YAML 格式的 Assembly Manifest，包含兩個頂層區塊：`workspaces`（清單，每項含 `id` 與 `path`）與 `relations`（清單，每項含 `from`、`type`、`to`，以及可選的節點粒度引用 `ws-id::node_id`）。同一批 workspace SHALL 允許存在多份 manifest 檔，互不衝突。

#### Scenario: 解析最小 manifest
- GIVEN 一份只含 `workspaces`（兩個 workspace）與 `relations`（一條 `uses` 關聯）的 YAML 檔
- WHEN 執行 `graphify compose validate <manifest>`
- THEN 解析成功且回報 manifest 合法（workspaces 數量、relations 數量）

#### Scenario: 多份 manifest 共存
- GIVEN 同一工作目錄下有 `ecommerce.yaml` 與 `observability.yaml` 兩份 manifest，引用部分相同的 workspace
- WHEN 分別 validate 兩份 manifest
- THEN 兩者各自獨立合法，互不影響

### Requirement: Manifest 引用驗證
`compose validate` SHALL 驗證：(1) 每個 `workspaces[].path` 存在且含 `graphify-out/graph.toon`（或 `graph.json` fallback）；(2) 每條 relation 的 `from`/`to` workspace id 存在於 manifest 的 workspaces 清單；(3) 節點粒度引用（`ws-id::node_id`）的 node_id 存在於該 workspace 的 graph。任何驗證失敗 SHALL 回報明確錯誤（含失敗項目與原因），不得靜默跳過。

#### Scenario: workspace 路徑不存在
- GIVEN manifest 中某 workspace 的 `path` 指向不存在的目錄
- WHEN 執行 validate
- THEN 回報錯誤並指出該 workspace id 與路徑

#### Scenario: 節點引用不存在
- GIVEN relation 引用 `agentshop::fn_main`，但 `agentshop` 的 graph 中無此節點
- WHEN 執行 validate
- THEN 回報錯誤並指出完整的節點引用字串

#### Scenario: relation 引用未宣告的 workspace
- GIVEN relation 的 `to` 欄位填了 manifest workspaces 清單中不存在的 id
- WHEN 執行 validate
- THEN 回報錯誤並指出該 relation 與未知 id

### Requirement: Unified Graph 合併
`compose graph` SHALL 載入 manifest 所列每個 workspace 的 graph（優先 `graph.toon`，fallback `graph.json`，沿用既有 legacy schema 遷移語意），加上 manifest 的跨 workspace relations，組成單一 unified graph。Workspace SHALL 作為 composition boundary：workspace 內部 AST 節點/邊保持原樣，跨 workspace relation 為 composition edge。節點 id SHALL 加上 workspace 前綴（`ws-id::node_id`）以避免跨 workspace 撞名。

#### Scenario: 合併兩個 workspace
- GIVEN manifest 宣告 workspace A 與 B，各含自己的 .toon graph，以及一條 A→B 的 relation
- WHEN 執行 `compose graph`
- THEN 產生的 unified graph 同時含 A、B 的全部節點與內部邊，以及該條跨域 composition edge
- AND 跨域 edge 的 relation 欄位值為 manifest 中宣告的 `type`

#### Scenario: 跨 workspace 節點撞名
- GIVEN workspace A 與 B 各自有一個 id 相同的節點（例如 `src/main.rs`）
- WHEN 執行 `compose graph`
- THEN unified graph 中兩者為不同節點（id 分別為 `a::src/main.rs` 與 `b::src/main.rs`），不發生合併或覆蓋

### Requirement: Diagram 投影（box-of-rain）
`compose render` SHALL 將 unified graph 投影為 box-of-rain JSON schema（遞迴 `children` 樹 + `connections`），並透過 `npx box-of-rain`（stdin JSON）產生 ASCII 輸出（`--svg` 選項產生 SVG）。每個 workspace SHALL 對應一個容器 box（title 為 workspace id），容器內為其節點 boxes；跨 workspace relation SHALL 對應 connection（label 為 relation type）。投影與 layout SHALL 與 graph model 分離：layout engine 不進入 Rust 依賴樹。當 `npx` 不可用或 box-of-rain 回報 schema 錯誤時，SHALL 回傳明確錯誤訊息（含 stderr 摘要），不得產生假輸出。

#### Scenario: 渲染 ASCII 圖
- GIVEN 一份合法 manifest 與各 workspace 的 .toon graph
- WHEN 執行 `compose render <manifest>`
- THEN 輸出 ASCII 圖，含每個 workspace 的容器 box 與跨域箭頭（label 為 relation type）

#### Scenario: npx 不存在
- GIVEN 執行環境無 `npx` 指令
- WHEN 執行 `compose render`
- THEN 回報明確錯誤（npx 不可用），不輸出任何圖形

#### Scenario: SVG 輸出
- GIVEN 合法 manifest
- WHEN 執行 `compose render --svg`
- THEN 輸出 SVG 字串而非 ASCII 文字

### Requirement: MCP compose 工具（authoring 介面）
graphify-mcp SHALL 新增三個內建 tools：
- `graphify_compose_read`：讀取指定 manifest，回傳 workspaces 與 relations 結構化內容。
- `graphify_compose_write`：寫入/更新指定 manifest 的 relations（AI 語意關聯撰寫入口）。寫入前 SHALL 執行與 `compose validate` 相同的引用驗證；驗證失敗 SHALL 拒絕寫入並回傳失敗原因，manifest 檔保持原狀（不部分寫入）。
- `graphify_compose_render`：對指定 manifest 執行 render，回傳 ASCII（或 SVG）文字。

#### Scenario: AI 撰寫新關聯
- GIVEN agent 透過 MCP 呼叫 `graphify_compose_write`，payload 含一條新 relation（兩端節點皆存在）
- WHEN 寫入執行
- THEN manifest 檔新增該 relation，且回傳寫入後的 relations 摘要

#### Scenario: AI 撰寫無效關聯被拒
- GIVEN agent 呼叫 `graphify_compose_write`，relation 引用不存在的節點
- WHEN 寫入執行
- THEN 回傳驗證錯誤（指出節點引用字串），manifest 檔內容不變

#### Scenario: 讀取 manifest
- GIVEN 已存在的 manifest 檔
- WHEN 呼叫 `graphify_compose_read`
- THEN 回傳 workspaces 清單與 relations 清單的結構化內容

#### Scenario: MCP 觸發渲染
- GIVEN 合法 manifest
- WHEN 呼叫 `graphify_compose_render`
- THEN 回傳 box-of-rain 產生的 ASCII 圖文字
