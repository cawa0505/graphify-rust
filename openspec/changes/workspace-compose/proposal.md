# Proposal: workspace-compose

## Why

Graphify 目前每個 workspace 各自產生獨立的 `graphify-out/graph.toon`（AST graph），但 workspace 之間的語意關聯（uses / audits / executes_via / depends_on…）只能手寫在 AGENTS.md 等文件裡——無結構、無法驗證、無法渲染、AI 也讀不到權威版本。我們需要一份機器可讀的 Assembly Manifest（YAML）作為跨 workspace 關聯的 SSoT，並讓 AI 透過 MCP tool 直接撰寫/維護關聯，取代手寫文件。

## What Changes

- 新增 **Assembly Manifest**（YAML）格式：宣告 workspace 清單（id + path）與跨 workspace relations（from/to/type + 可選節點粒度 `ws::node_id`）。同一批 workspace 可有多份 manifest（不同組裝視角），manifest 是「組裝說明書」而非系統唯一真相。
- 新增 CLI 子指令 `graphify compose`：
  - `compose validate <manifest>` — 解析 manifest、驗證 workspace 路徑與節點引用存在於各 workspace 的 `graphify-out/graph.toon`（不存在的引用必須報錯，不得靜默跳過）。
  - `compose graph <manifest>` — 合併各 workspace .toon graph + manifest relations 成單一 unified graph（workspace 為 composition boundary，跨域 relation 為 composition edge）。
  - `compose render <manifest>` — 投影 unified graph 為 box-of-rain JSON schema，shell out 到 `npx box-of-rain`（stdin JSON）輸出 ASCII/SVG。renderer 與 graph model 分離，layout engine 不進 Core。
- 新增 MCP tools（graphify-mcp 內建 tool，沿用現有命名規範）：
  - `graphify_compose_read` — 讀取指定 manifest 的內容（workspaces + relations）。
  - `graphify_compose_write` — 寫入/更新 manifest relations（AI 語意關聯撰寫的入口；寫入前執行 validate 語意，引用不存在即拒絕寫入）。
  - `graphify_compose_render` — 觸發 render，回傳 ASCII 圖文字。
- 不修改 Graphify Core 的 graph 模型：workspace 內部 AST 照舊由現有 pipeline 產生；compose 層只做合併與投影。

## Capabilities

### New Capabilities

- `workspace-compose`: Assembly Manifest 格式、`graphify compose` CLI 子指令（validate/graph/render）、unified graph 合併語意、box-of-rain 投影規則。

### Modified Capabilities

- `mcp-server`: 新增三個內建 MCP tools（`graphify_compose_read` / `graphify_compose_write` / `graphify_compose_render`），擴充現有唯讀查詢工具集；寫入工具需遵守 manifest 語意驗證。

## Impact

- **graphify-cli**: 新增 `compose` 子指令（`Commands` enum + 對應 run 函式）。
- **graphify-mcp**: 新增三個 compose tools（註冊於現有內建 tool 集合，與 `graphify_memory_query`、`graphify_notify_plugins` 同層）。
- **新依賴**：graphify-cli 需 YAML 解析（serde_yaml 或同等），用於 manifest 讀寫。box-of-rain 為外部 npm 工具，透過 `npx` shell-out 呼叫，不進 Cargo 依賴樹；npx 不存在時 render 必須回報明確錯誤（不 mock）。
- **既有行為不變**：`graphify graph` / run_index / .toon schema 均不受影響；compose 是純新增的消費層。
- **參考文件**：`docs/ref/graphify-hierarchical-graph-assembly-concept.md`、`docs/ref/graphify-assembly-manifest-concept.md`、`docs/ref/box-of-rain-readme.md`。
