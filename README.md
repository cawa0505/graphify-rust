# GraphifyRust

基於 Rust 與 Tree-sitter 實作的高效能、低延遲靜態程式碼 AST 語意圖譜建構工具。整合第一類（First-class）`.toon` 自研超省 Token 序列化格式、多金鑰執行緒安全原子輪轉（Auto-Rotate）容災管線、Qdrant 長短期語意記憶體配置，以及 MCP (Model Context Protocol) 伺服器，專為 LLM 助理提供極低成本、極致精準的程式碼拓撲感知能力。

---

## 核心技術優勢 (Key Capabilities)

### 1. 緊湊型 `.toon` 圖譜格式 (First-Class `.toon` Format)
- **解耦 JSON 冗餘**：傳統圖譜使用大體積 JSON 格式會因重複的屬性鍵名（Keys）造成嚴重的 Token 浪費與快取失效。
- **Tabular Array 串流序列化**：`.toon` 格式採用類似表格式 CSV 的緊湊列結構（如 `nodes[103,]{id,label,file_type...}`），僅在 header 定義一次屬性名稱，資料列全數以精簡 CSV 形式緊密排列，縮減體積達 **60% 以上**。
- **無縫向下相容**：`graphify-cli` 與 `graphify-mcp` 自動偵測 `.toon` 與 `.json` 副檔名，並提供透明的互轉及並存支援。

### 2. Auto-Rotate 雙層執行緒安全容災管線
- **Atomic 輪轉**：多執行緒環境下採用 `AtomicUsize` 進行無鎖模除（Modulo）輪轉，完美承載多執行緒並發提取。
- **零延遲 429 容災**：遭遇 Rate Limit (HTTP 429) 時，不進行睡眠等待，立即推進原子計數器，切換至下一個 API Key。
- **動態 Provider 降級**：當主線 Cloud API (Gemini, OpenRouter) 所有的金鑰均告罄，自動、無感降級至 Homelab 本地 SLM (Ollama, e.g. Qwen2.5-Coder)。

### 3. 長短期語意記憶（Qdrant & STM）配置
- **短期對話視窗 (STM)**：設定 `max_messages` 上限，超過時自動啟動語意 Compartment 壓縮，精簡歷史。
- **長期語意記憶 (LTM)**：對接 **Qdrant 向量引擎**（預設位址為 `http://localhost:6333`，採用 `Cosine` 餘弦相似度演算法），將結構化的架構決策與專案規則進行持久化與跨 Session 語意檢索。
- **防禦性相容**：設定讀取模組具備預設值防禦，舊版不含記憶體欄位的設定檔在載入時會自動套用預設值，並在自動遷移（Migrate）至 XDG 目錄時回寫完整的現代 TOML 格式。

### 4. 毫秒級多語言 AST 提取
- 基於原生 Tree-sitter，支援 `Rust`, `Python`, `Go`, `JavaScript`, `C`, `C++`, `PHP` 七大語言靜態代碼精確解析。
- 提取模組、結構體、函數、介面、類別等核心符號，並建立 `contains`、`calls`、`imports` 等實體關聯。

---

## 專案結構 (Crate Layout)

```text
GraphifyRust/
├── graphify-core/   # AST 靜態解析器、有向圖引擎、.toon 序列化與圖譜導出 (Pure Sync, WASM ready)
├── graphify-llm/    # 多 Provider 金鑰原子輪轉管線、429 容災、長短期記憶與 Qdrant 配置
├── graphify-mcp/    # MCP 伺服器，將圖譜工具 (Summary, Reindex, Trace) 暴露給 AI 終端 (Async via Tokio)
├── graphify-cli/    # 命令列管理工具，支援靜態提取、BFS 圖譜查詢與格式互轉
└── openspec/        # OpenSpec 技術規格書，作為專案功能的單一事實來源 (SSOT)
```

---

## 快速開始 (Quick Start)

### 1. 編譯專案
```bash
cargo build --release
```

### 2. 靜態提取代碼圖譜 (支援 `.toon` 與 `.json`)
```bash
# 預設提取當前工作區並輸出至 graphify-out/graph.toon (極度推薦)
cargo run --release -p graphify-cli -- extract .

# 亦可手動指定輸出路徑與格式
cargo run --release -p graphify-cli -- extract . --output graphify-out/graph.json
```

### 3. 命令列 BFS 關係鏈查詢
```bash
# 查詢 config.rs 中的 MemoryConfig 結構體與其所屬 module 關係
cargo run --release -p graphify-cli -- query "./graphify-llm/src/config.rs:struct:MemoryConfig"
```

### 4. 啟動 MCP 互動式伺服器
```bash
cargo run --release --bin graphify-mcp
```
> **提示**：當偵測到工作區存在 `graphify-out/graph.toon` 時，MCP 會自動載入高效 toon 格式。在呼叫增量重新索引工具（`graphify_graph_reindex`）時，系統亦會秒級更新並回寫至 `.toon` 檔案，全程零污染（配合預設 `.gitignore` 隔離產出檔）。

---

## 系統配置規格 (Configuration Schema)

設定檔優先自環境變數 `GRAPHIFY_CONFIG_PATH` 讀取，若無則依據 XDG 規範存取 `~/.config/graphify/config.toml`。

```toml
# XDG 標準：~/.config/graphify/config.toml

[[providers]]
name = "gemini-primary"
type = "gemini"
endpoint = ""                  # 自訂 proxy/mirror 端點，空白則使用官方預設
model = "gemini-1.5-pro"
api_key = "AIzaSy..."          # 支援逗號分隔多金鑰: "KEY_A,KEY_B,KEY_C"
priority = 1                   # Priority 越低優先級越高

[[providers]]
name = "ollama-backup"
type = "ollama"
endpoint = "http://localhost:11434"
model = "qwen2.5-coder:7b"
priority = 10                  # 作為備用 Provider

[extraction]
chunk_size = 1024
max_concurrency = 1

[memory]
[memory.short_term]
max_messages = 20

[memory.long_term]
enabled = false
provider = "qdrant"

[memory.long_term.qdrant]
url = "http://localhost:6333"
collection = "graphify_memory"
distance = "Cosine"
```

---

## 研發規範與 OpenSpec 流程

本專案實施嚴格的 **OpenSpec 變更管理流程**：
1. **規格優先 (Spec-First)**：任何新功能的加入或現有行為的修改，必須先於 `openspec/specs/<feature>/spec.md` 建立或修改規格。
2. **警告全清 (Warning-Free)**：所有 Rust 代碼提交前必須通過 `cargo test` 與 `cargo clippy --all-targets -- -D warnings`，全專案不允許存在任何編譯警告或未處理的 Clippy 檢測。
3. **無 Mock 承諾 (No-Mock)**：不允許實作靜態死資料或假 mock 行為。若功能尚未實作，一律回傳 `Result::Err`，確保功能真實、可驗證。

---

## ⚡ 效能基準測試與對齊 (Performance Benchmark & Parity)

### 1. Parity Check (向後相容與對齊驗證)
我們對同一個包含 `Rust`, `Python`, `Go` 以及 `JavaScript` 的多語言測試專案（110 個源檔案，422 條邊）進行了提取比對：
- **Python 舊版產出**：110 Nodes, 422 Edges
- **Rust 新版產出**：110 Nodes, 422 Edges
- **對齊結果**：**100% 物理對齊**。Rust 版本的 AST 提取與圖譜拓撲邏輯完全向下相容，輸出的 `.toon` 與 `.json` 節點/邊資訊與 Python 版完全一致，保證了舊版 Python 可以**無痛、完美退休（EOL）**。

### 2. Performance Comparison (效能壓倒性時刻)
在同一個 Homelab 實體開發環境下，對 110 檔案的專案進行完整 AST 提取與有向圖建構的耗時比對：

| 評測維度 | Python 舊版 | Rust 新版 (110 檔案) | 效能提升倍數 |
| :--- | :---: | :---: | :---: |
| **AST 提取 + 建圖時間** | ~420 ms | **16 ms** (0.016s) | **26.25 倍** ⚡ |
| **多核心並行擴充性** | 不支援 (單執行緒) | **支援 Rayon (-j N)** | 隨執行緒數線性增長 🚀 |
| **記憶體分配策略** | 動態擴容拷貝 | **Petgraph Arena 預分配** | 0 垃圾記憶體碎片 |
| **圖譜檔案體積 (.toon)** | 185 KB (JSON) | **74 KB** (.toon) | **節省 60% 體積** (Token 效率提速) |

---

## 📄 開源授權 (License)

本專案採用 **MIT License** 授權，詳見 [LICENSE](LICENSE) 檔案。
