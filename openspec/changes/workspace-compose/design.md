# Design: workspace-compose

## Context

每個 workspace 已由現有 pipeline 產生 `graphify-out/graph.toon`（fallback `graph.json`，legacy schema 遷移語意已存在於 `run_index` / `load_graph_output`，見 specs `sync-toon-packet`）。graphify-mcp 已有內建 tool 註冊模式（`memory_query.rs` 等，唯讀查詢 + `graphify_notify_plugins`）。CLI 以 clap `Commands` enum 組織子指令。構想文件見 `docs/ref/graphify-hierarchical-graph-assembly-concept.md`、`docs/ref/graphify-assembly-manifest-concept.md`；box-of-rain schema 見 `docs/ref/box-of-rain-readme.md`。

## Goals / Non-Goals

**Goals**
- Manifest（YAML）解析 + 引用驗證（workspace 路徑、relation 端點、節點粒度引用）。
- Unified graph 合併：各 workspace graph + manifest relations，workspace 前綴防撞名。
- box-of-rain 投影（stdin JSON → ASCII/SVG），renderer 與 graph model 分離。
- MCP authoring tools：read / write（帶驗證的寫入口）/ render。

**Non-Goals**
- 不改 Core graph 模型、不改 .toon schema、不做 workspace 內部 AST 的手寫描述。
- 不把 layout engine 納入 Rust 依賴樹；不支援 Mermaid 輸入（box-of-rain 自己支援，但我們只投影 JSON）。
- 不做 TUI 整合（未來議題）；不做 multi-tenant / 遠端 manifest。

## Decisions

### D1: Manifest 格式與位置
YAML，頂層 `workspaces`（id + path）+ `relations`（from/type/to + 可選節點引用 `ws::node_id`）。檔案由使用者指定路徑（慣例放 workspace 外層目錄，如 `compose/ecommerce.yaml`）。多份 manifest 共存，無全域註冊。
- 替代方案：JSON（人寫太吵）、TOML（list-of-tables 寫起來囉唆）。選 YAML：構想文件即 YAML，AI authoring 也最友善。

### D2: 合併策略 — 投影層獨立，petgraph 合併
`compose graph` 在 CLI 端載入各 .toon → petgraph，節點 id 冠前綴 `ws-id::`（UUID 內部型別沿用既有 `NodeId` 包裝，字串層加前綴），跨域 relation 直接加邊。此合併圖是暫態（每次 compose 重建），不落盤為第三種 SSoT —— .toon 仍是各 workspace 的 SSoT，manifest 是組裝層 SSoT。
- 替代方案：把合併圖寫回 `graphify-out/`（拒絕：污染既有輸出，且組裝視角多元，不宜單一落盤）。
- **[待討論]** 是否需要把 unified graph 快取落盤（大 workspace 時重複載入成本）。先不做。

### D3: 節點引用語法 `ws-id::node_id`
單一分隔符 `::`。node_id 為該 workspace .toon 內的原始節點 id 字串。驗證時逐 workspace 載入其 graph 並檢查 id 存在。workspace id 不得含 `::`（validate 檢查）。
- 替代方案：自然鍵（file + line，太脆）；label 搜尋（歧義）。`::` 對既有 `graphify-out` 節點 id 命名（`{file_path}:{line}` 風格）衝突風險低，validate 會直接報 not-found。

### D4: box-of-rain 呼叫 — shell-out `npx`，stdin JSON
投影函式輸出 box-of-rain JSON（`children` 樹 + `connections`），經 stdin 餵 `npx box-of-rain`（`--svg` 可選），串流回傳 stdout/stderr。`npx` 不存在（spawn 失敗）或非零退出 → 明確錯誤 + stderr 摘要。Rust 端零新依賴（`std::process::Command`）。
- 替代方案：Rust 重寫 layout（違反「不重造 layout」原則）；Node 端長駐服務（YAGNI）。
- feasibility spike 先行：手動 `npx box-of-rain` 驗證輸入輸出契約，再寫投影。
- **Spike 結論（2026-09-21，Node v24.18.0 / npx 11.16.0）**：容器 box（`children` 遞迴樹，`title` 欄位不必要，有 id + label 即可）+ 跨容器 connection（`from`/`to`/`label`）渲染正確（labeled arrow `└ uses ─▶`）；`--svg` 輸出 SVG XML。**陷阱：box-of-rain 對所有錯誤（含 malformed JSON、dangling connection 引用不存在節點）一律 exit 0**，部分情境靜默輸出空盒。對策：投影 JSON 僅由驗證過的 unified graph 自建（端點必存在），並在 stdout 偵測 `Error:` 前綴視為失敗；不依賴 exit code。

### D5: MCP tools 放 graphify-mcp 內建層
三個 tools 註冊於 graphify-mcp 現有內建 tool 集合（與 memory_query 同層），命名沿用 `graphify_` 前綴。`graphify_compose_write` 是唯一寫入工具：先驗證後原子寫入（暫存檔 + rename），失敗時 manifest 位元組不變。
- 替代方案：做成 plugin（拒絕：compose 是 core 消費層能力，且 MCP plugin 協議走 Stdio+JSON-RPC 過重）。符合記憶 #3084「embedded plugins 由 core trait 定義；此處為內建 tool 而非 plugin」。

### D6: 新依賴 — serde_yaml_ng 僅限 graphify-cli 與 graphify-mcp
YAML parser 定案 **`serde_yaml_ng` 0.10**（2026-09-21 crates.io 查證）：`serde_yaml` 0.9.34 已 archived/deprecated（dtolnay 官方聲明不再維護）；`serde_yml` 0.0.13 版本號躁進、API 有爭議。`serde_yaml_ng` 是 serde_yaml 0.9 API 相容的活躍 fork（API 同 call sites，改動成本最低）。graphify-core 不加依賴。YAML schema 結構體放 `graphify-core`（純資料型別 + serde derive，無 YAML 解析器依賴），parser 本體（serde_yaml_ng）在 graphify-cli 與 graphify-mcp。

### D7: 檔案切分（300 行上限）
新模組：`graphify-cli/src/compose/mod.rs`（子指令分派）、`manifest.rs`（解析+驗證）、`merge.rs`（unified graph）、`render.rs`（投影+npx）。

## Risks / Trade-offs

- [npx 需要 Node 環境；CI/容器可能沒有] → render 指令明確報錯並提示安裝；validate/graph 不依賴 npx。
- [大 workspace 逐一載入 .toon 慢] → 先不做快取（[待討論] D2）；compose 是人類/AI 觸發的低頻操作。
- [`::` 前綴與 node id 衝突] → 節點 id 由 graphify 產生（file:line 格式），實務不含 `::`；validate 驗證引用存在性可捕捉誤寫。
- [serde_yaml 維護狀態] → 實作前確認 crate 健康度（archived serde_yaml vs serde_yml fork），擇活躍者。
- [MCP write 工具的濫用（AI 亂寫關聯）] → 寫入必過 validate；manifest 在 git 版本控管下可審查、可回滾。不加多餘的權限系統（YAGNI）。

## Migration Plan

純新增能力，無遷移。既有指令行為零改動。回滾 = 刪除 compose 子指令與 MCP tools 註冊（無資料格式變更）。

## Open Questions

- **[待討論]** unified graph 是否需要落盤快取（見 D2）。
- **[待討論]** manifest 慣例路徑（repo 根、`compose/` 子目錄、或 XDG）——先用「使用者傳入路徑」不設慣例，等實際使用經驗。
