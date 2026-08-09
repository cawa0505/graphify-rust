# TUI 的規格變更計劃

> Note (2026-08-09): 本文為原始構想之 verbatim 快照。文中「workspace_uuid」已於 plugin-events-v1 更名為「workspace_key」（= canonical workspace root path 之穩定 hash，見 #3086）；全文屬設計提案，尚未定稿。

當系統從「單機純檔案（Stateless CLI）」走向「多工作區（Multi-Workspace）、多 Agent 協同、Plugin Domain Memory 託管」時，「系統中到底有哪些 Workspace？」 這個問題就再也無法只靠 cwd（當前目錄）來回答了。你感覺需要一個 DB 是完全正確的，因為現在出現了全域中台中繼資料（Global System Metadata） 的管理需求：

- 列出所有 Workspace（TUI 下拉選單、Agent 切換視角、CLI 清單命令）。
- 紀錄 Workspace 的 Indexing 狀態與 Health（最後 Re-index 時間、AST 節點數、Embedding 狀態）。
- Workspace Path 到 workspace_key 的動態映射（例如：/Users/dev/project_a $\rightarrow$ ws_hash_9921）。
- 追蹤全域的 Domain Memory & Handoff Snapshots 歸屬。

💡 輕量級解法：嵌入式 SQLite (SQLite/Rusqlite)

在 Rust 架構中，完全不需要架設一套額外的 PostgreSQL 或外掛服務。最佳且最極致的解法是在 Graphify 的全域 Home 目錄（例如 `~/.graphify/graphify.db`）嵌入一個 SQLite (使用 rusqlite 或 sqlx crate)。

**為什麼 SQLite 是最佳選擇？**

- 零維運、純本地 (Zero-Config & Self-Contained)：一個單一.db 檔，不需要啟動 Daemon，開箱即用，完全符合 Graphify 離線、輕量、快速的基因。
- 100% 毫秒級讀取：SQLite 讀取 workspace_key 清單的延遲小於 0.1ms，極度適合 TUI 啟動時讀取。
- 鎖定與並發安全：Rust + SQLite (WAL Mode) 能輕鬆處理多個 Agent/Plugin 同時讀寫全域 Workspace 清單的場景。

🗄️ 全域 Core Metadata DB Schema 設計

這個 SQLite DB (`~/.graphify/graphify.db`) 只負責儲存「全域註冊與狀態中繼資料 (Global Registry & State)」，而不儲存龐大的 AST 拓撲（依然在 Petgraph/.toon）或向量（依然在 Qdrant）：

```sql
-- 1. 全域 Workspace 註冊表
CREATE TABLE IF NOT EXISTS workspaces (
 Workspace_key TEXT PRIMARY KEY, -- 確定性識別碼 (如 SHA256 of absolute path)
 Workspace_name TEXT NOT NULL, -- 專案顯示名稱 (如 "backend-core")
 Root_path TEXT NOT NULL UNIQUE, -- 本地絕對路徑 (如 "/Users/dev/backend-core")
 Created_at INTEGER NOT NULL, -- 建立時間 (Unix Timestamp)
 Last_indexed_at INTEGER NOT NULL, -- 上次 AST Re-index 時間
 Ast_node_count INTEGER DEFAULT 0, -- AST 節點數量 (統計用)
 Is_active BOOLEAN DEFAULT 1 -- 是否仍存在/啟用
);

-- 2. Plugin 註冊與 Collection 映射表
CREATE TABLE IF NOT EXISTS plugin_registrations (
 Plugin_id TEXT NOT NULL,
 Workspace_key TEXT NOT NULL,
 Qdrant_collection_name TEXT NOT NULL, -- 如 "graphify_plugin_opendoc_ws9921"
 Last_synced_at INTEGER,
 Status TEXT NOT NULL, -- "Ready", "Unavailable", "Pending"
 PRIMARY KEY (plugin_id, workspace_key),
 FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
);

-- 3. Handoff Snapshots 全域索引表 (方便 TUI 與 Agent 全域搜尋可用的 Handoff)
CREATE TABLE IF NOT EXISTS handoff_registry (
 Snapshot_id TEXT PRIMARY KEY,
 Workspace_key TEXT NOT NULL,
 Session_id TEXT NOT NULL,
 Task_goal TEXT NOT NULL,
 Created_at INTEGER NOT NULL,
 Expires_at INTEGER NOT NULL,
 FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
);
```

🖥️ 賦能 TUI：workspace_key 下拉選單與面板

有了這個 SQLite 註冊表後，TUI 就能直接解鎖極致的 Multi-Workspace 管理體驗：

```
┌─ Graphify TUI ────────────────────────────────────────────────────────────────────────┐
│ Active Workspace: [ 1. Backend-core (/Users/dev/backend) ▼ ] Mode: [Live Tracking] │
│ │ 1. Backend-core (/Users/dev/backend) │ │
│ │ 2. Frontend-ui (/Users/dev/frontend) │ │
│ │ 3. Opendoc-sdk (/Users/dev/opendoc) │ │
│ └─────────────────────────────────────────┘ │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ [Workspaces Monitor] │
│ KEY NAME LAST INDEXED NODES PLUGINS STATUS │
│ ws_backend_9921 backend-core 2 mins ago 12,400 opendoc(OK) review(OK) │
│ ws_frontend_3312 frontend-ui 1 hour ago 8,100 opendoc(OK) review(UNAVAIL)│
│ ws_opendoc_8812 opendoc-sdk 3 days ago 2,100 handoff(OK) │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

**TUI 獲得的升級能力：**

- 即時清單列出 (`SELECT * FROM workspaces`)：TUI 啟動時讀取 SQLite，一秒列出本機上所有曾經被 Graphify 索引過或開啟過的專案。
- 動態切換視角 (Ctrl + W)：在 TUI 裡切換選中的 Workspace，系統自動去載入該專案對應的.toon 拓撲與 Qdrant 獨立 Collection，完全不用重新重開 TUI。
- 無效路徑清理 (Auto Clean-up)：若某個專案目錄已被刪除，SQLite 可以自動標註 is_active = false 並提示開發者是否清理對應的 Qdrant Collection。

🚀 結論：三大元件各司其職

加入 SQLite 後，整體 Graphify 系統的三層儲存架構完全告成：

- SQLite (`~/.graphify/graphify.db`) → 全域中繼資料 (Global Metadata)：負責列出/管理所有 workspace_key、Plugin 註冊狀態、Handoff 索引與專案路徑映射。
- Petgraph (.toon) → 空間拓撲 (Symbolic AST)：負責每個 Workspace 記憶體內 16ms 毫秒級極速代碼圖譜與衝擊計算。
- Qdrant (Vector DB) → 神經記憶 (Neural Vector Store)：負責儲存各 Plugin 隔離的文檔 Chunks、歷史 Review 經驗與語意 Vector。

---

原本 TUI 的設計假設極度簡單：單一單機／單一目錄（Single Workspace）。當 TUI 啟動時，它直接去當前 Working Directory 的本地 `.graphify/` 下面開 Local Storage、讀那個目錄產出的.toon。

但現在，一旦導入了：

- Core / Plugin Memory 的 workspace_key / workspace_uuid 多租戶隔離
- HandoffSnapshot 的跨 Session / 跨 Workspace 接力與恢復
- Qdrant 集中式託管的多工作區 Collection

TUI 就不能再「死綁定目前 Shell 所在目錄的單一.toon」了，否則就會發生：「當前開在 A 專案，但我想要看被 Handoff 過來的 B 專案快照，或是查詢跨 Workspace 的 OpenDoc 知識庫時，TUI 畫面一片空白或撈錯資料」。

💡 如何解決 TUI Workspace 綁定問題？

為了讓 TUI 能優雅支援這個全新的架構，TUI 需要進行 「Workspace 綁定模型 (Workspace Binding Model)」 的升級：

**1. 引進 Active Workspace Selector (工作區切換器)**

TUI 預設依然可以吃 cwd（目前目錄）作為 Current Active Workspace，但在 TUI 頂部 Header 或面板內加入一個快捷切換器（例如 Ctrl+W）：

- 預設 (Auto)：自動載入目前 Working Directory 的 workspace_key 與.toon 拓撲。
- 切換 (Select/Inspect)：允許開發者切換視角至特定的 workspace_key（例如 Qdrant 裡註冊過的其他專案），或是載入某個 HandoffSnapshot 所附帶的獨立 workspace_key。

```
┌─ Graphify TUI ─ Active WS: [ my-project-core (Auto) ▼ ] ────── [Plugins & Memory] ─┐
```

**2. TUI 改為向 Graphify Engine Gateway 索取資料，而非直接讀本地死檔**

- 舊作法：TUI → 直接讀取當前目錄 `./.graphify/graph.toon`
- 新作法：TUI → 帶入 active_workspace_key → 向 GraphifyCoreService 查詢：
  - AST 靜態圖譜：若切換到當前專案，讀當前 Petgraph；若切換到 Handoff Snapshot，直接繪製 Snapshot 內帶有的 focused_subgraph_toon！
  - Plugin Memory：帶入 active_workspace_key 與 plugin_id 查詢 Qdrant，完全不會越界或撈到別的專案的髒資料。

**3. 專屬的 [Handoff Inspector] 預覽模式**

當 TUI 接收到一個來自別處的 Handoff 時，TUI 可以自動開啟 "Snapshot Inspection Mode"：

- 頂部標籤顯示：`WS: my-project-core (Inspecting Handoff #402)`
- 畫面上的.toon 拓撲圖直接渲染該 Handoff 快照內封裝的 focused_subgraph_toon，就算本地代碼庫目前不在那個 Git Commit，工程師也能在 TUI 上完美還原當時 Agent 的「心智地圖」與周邊 AST 衝擊半徑！

🚀 調整後的 TUI 資料流

```
┌──────────────────────────────────────────────┐
 │ TUI Master Controller │
 │ Active Workspace: "ws_backend_service_123" │
 └──────────────────────┬───────────────────────┘
 │
 ▼
 ┌──────────────────────────────────────────────┐
 │ Graphify Memory Gateway │
 │ (Applies workspace_key & plugin_id filtering)│
 └──────────────────────┬───────────────────────┘
 │
 ┌──────────────────────────────┴──────────────────────────────┐
 ▼ ▼
┌─────────────────────────────────────────────┐ ┌─────────────────────────────────────────────┐
│ Core AST /.toon │ │ Plugin Domain Memory │
│ (Reads Active WS Petgraph or Handoff.toon) │ │ (Queries Qdrant with active_workspace) │
└─────────────────────────────────────────────┘ └─────────────────────────────────────────────┘
```

這樣調整之後，原本「開在哪個 workspace 就只能抓哪個.toon」的硬傷就被完全破解了！TUI 不僅能預設服務好當前目錄的開發，還升級成能夠跨 Workspace 檢索、動態預覽 Handoff 快照、檢查特定專案 Plugin Memory 的強大多專案儀表板！

---

既然 Graphify 已經從單純的 AST 分析器升級為 「Neuro-Symbolic（神經+符號）雙模矩陣」，TUI（Terminal User Interface）就不再只是用來開關 Toggle，而是能扮演整個系統的 「拓撲與神經控制中心 (Command & Control Dashboard)」。

加入一個專屬 Tab（例如名為 `[Plugins & Memory]` 或 `[Matrix Monitor]`），能讓開發者或除錯中的工程師第一眼看到全系統的健康度與運作狀態。

以下是針對這個 TUI Tab 的設計構想與區塊規劃：

🖥️ TUI Layout 規劃構想：[Plugins & Memory] Tab

我們可以把它劃分為 四個核心監控面板 (Panels)：

```
┌─ Graphify Control Center ─────────────────────────────────────────── [Plugins & Memory] ─┐
│ │
│ ┌─ 1. Core & Neural Health ─────────────┐ ┌─ 2. Domain Memory Storage ──────────────────┐ │
│ │ Petgraph AST Engine: ACTIVE (16ms) │ │ [opendoc] 2,410 chunks | Qdrant (3072d) │ │
│ │ Qdrant Vector DB: CONNECTED │ │ [review] 182 findings | Qdrant (1536d) │ │
│ │ Embedding Provider: READY (Ollama) │ │ [handoff] 5 snapshots| Qdrant (1536d) │ │
│ └───────────────────────────────────────┘ └────────────────────────────────────────────┘ │
│ │
│ ┌─ 3. Active Plugins & Safety ──────────┐ ┌─ 4. Live Relay & Trace Monitor ────────────┐ │
│ │ [✓] graphify-plugin-opendoc (v1.2) │ │ [10:42:01] Review trace: 3-step Blast Radius │ │
│ │ [✓] graphify-plugin-review (v2.0) │ │ [10:42:03] Handoff exported (1.4 KB.toon) │ │
│ │ [✓] graphify-plugin-handoff (v1.0) │ │ [10:42:15] OpenDoc: Ref 'MemoryConfig' linked │ │
│ └───────────────────────────────────────┘ └────────────────────────────────────────────┘ │
│ │
│ [F1] Re-index AST [F2] Sync Memory [F3] Clear Handoffs [F5] Purge Plugin Cache │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

💡 四大核心區塊與互動功能設計

**1. 系統健康與降級狀態 (Core & Neural Health Status)**

監控重點：即時顯示上一題我們討論的 優雅降級 (Graceful Degradation) 狀態。

畫面元素：

- Petgraph AST Status：永遠顯示 ACTIVE（綠字），並附帶上次 Re-index 耗時（如 16ms）。
- Embedding Provider Status：若本地 Ollama 沒開或網路斷線，這裡會直觀顯示 UNAVAILABLE (Fallback to Pure Topology)（黃字/紅字警示），讓開發者一眼就知道「為什麼現在搜尋不到歷史 Memory，但.toon 拓撲仍可運作」。
- 快捷按鍵：[F2] 一鍵重新觸發 Memory Sync。

**2. Domain Memory 儲存與容量監控 (Domain Memory Inspector)**

監控重點：監控各 Plugin 獨立 Collection 的數據量與佔用情形（落實 Collection-per-Plugin 隔離原則）。

畫面元素：

- 列表呈現：`graphify_plugin_opendoc` (Vector 筆數、維度)、`graphify_plugin_review` (歷史 Review 數量)、`graphify_plugin_handoff` (快照數量)。
- 快捷操作：工程師可以在這裡選擇指定的 Plugin，直接 `[D] Clear Cache` 或是 `[R] Re-index Collection`（例如重解析 OpenDoc 檔案），完全不影響其他 Plugin 或 Core Memory！

**3. 插件啟用與設定 (Plugin Controls & Toggles)**

設定功能：

- 開關特定 Native Plugin（例如在純寫 Code 時暫時關閉 opendoc 的自動關聯，節省背景運算）。
- 設定 review 插件的衝擊半徑預設階數（Max Depth: 2 階 vs 3 階）。
- 設定 handoff 快照的 TTL 保留期限（如：自動清理超過 7 天的過期快照）。

**4. 即時 Event & Handoff 追蹤器 (Live Event Feed)**

除錯利器：

- 顯示即時的 Plugin 運作 Log。當 AI Agent 在背景執行接力棒時，TUI 會即時印出：

```
[Handoff Hydrated] Session #402 restored -> Pinned 3 nodes (MemoryConfig, flush_cache) via 1.2KB.toon.
```

- 這讓開發者能像看 tail -f 一樣，觀察 Agent 之間傳遞.toon 快照與查詢 Qdrant 的真實過程，非常有科幻感與掌控感！

🚀 對整體開發體驗 (DX) 的加分之處

- 可視化的「黑盒開箱」：AI Agent 在背景做代碼分析與 Handoff 時，以前對工程師來說完全是黑盒；有了這個 TUI Tab，工程師能清楚看到 AI 拿到了哪些.toon 拓撲與歷史 Memory。
- 極致的開發除錯工具：當寫 Plugin 寫到一半發現記憶查不到，按 Tab 切過來 0.1 秒就能確定是「Qdrant 斷線」、「Embedding 降級」還是「Collection 被清空」。

這樣規劃讓 TUI 從單純的觀看器，升級為這套 Neuro-Symbolic AI Engine 最硬核且美觀的控制台！
