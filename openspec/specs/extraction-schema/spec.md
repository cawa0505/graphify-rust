# Extraction Schema Specification

## 1. 目的 (Purpose)
定義 `graph.json` 輸出的圖譜 JSON Schema，做為 GraphifyRust 與舊版 Python 工具（如 HTML 視覺化與分析器）之間 100% 強相容的物理契約。

## 2. 規格要求 (Requirements)

### 2.1 節點結構要求 (Node Structure Requirement)
產出的每個節點（Node）物件必須包含且僅能包含以下物理欄位：
- `id` (String): 具備確定性（Deterministic）與唯一性的全限定標識符。
- `label` (String): 節點顯示名稱。
- `file_type` (String): 代表節點的物理/邏輯分類。
  - 列舉值：`"code"`, `"document"`, `"paper"`, `"image"`, `"rationale"`, `"concept"`。
  - AST 解析器提取之代碼實體預設為 `"code"`。
- `kind` (String): 節點內部類型（小寫蛇形命名，如 `"struct"`, `"function"` 等，允許自由擴充，不做嚴格限制，以利效能最大化與彈性）。
- `language` (String): 原始語言（如 `"rust"`, `"python"`, `"go"`, `"javascript"`, `"c"`, `"cpp"`, `"php"`）。
- `source_file` (String): 節點定義所在的源檔案相對路徑。
- `start_line` (u32): 定義起始行號（1-indexed）。
- `end_line` (u32): 定義結束行號（1-indexed）。
- 可選欄位：`doc_comment` (String)。
- `metadata` (Map<String, Value>): 視為不透明物件（Opaque Map）透傳，核心層不做強制結構校驗以確保極致的解析效能。

#### Scenario: Python 函數節點生成
- GIVEN 解析 Python 檔案 `utils.py` 中的 `def get_user():`
- WHEN 執行提取時
- THEN 節點物件輸出必須包含：
  ```json
  {
    "id": "python:utils::get_user",
    "label": "get_user",
    "file_type": "code",
    "kind": "function",
    "language": "python",
    "source_file": "utils.py",
    "start_line": 12,
    "end_line": 15
  }
  ```

### 2.2 邊結構要求 (Edge Structure Requirement)
產出的每個邊（Edge）物件必須包含且僅能包含以下物理欄位（嚴格禁止使用 `kind` 代替關係欄位）：
- `source` (String): 起始節點 ID。
- `target` (String): 目標節點 ID。
- `relation` (String): 代表邊的關係種類（如 `"calls"`, `"imports"`, `"inherits"`, `"contains"`）。
- `confidence` (String): 可信度列舉，必須為以下三者之一：
  - `"EXTRACTED"`: 靜態 AST 確定性解析。
  - `"INFERRED"`: 經由 LLM 語意推導或啟發式分析。
  - `"AMBIGUOUS"`: 具備歧義或不確定性。
- `source_file` (String): 此關聯發生的源檔案相對路徑（對齊 Python 版格式）。
- `source_location` (String): 發生的精準行號標示，格式為 `file_path:line_number`。

#### Scenario: 函數調用邊生成
- GIVEN 靜態解析器在 `src/main.rs` 第 42 行偵測到 `run_server()` 調用 `init_db()`
- WHEN 輸出邊關係時
- THEN 邊物件輸出必須包含：
  ```json
  {
    "source": "rust:main::run_server",
    "target": "rust:db::init_db",
    "relation": "calls",
    "confidence": "EXTRACTED",
    "source_file": "src/main.rs",
    "source_location": "src/main.rs:42"
  }
  ```

### 2.3 虛擬序列化超邊聚合規範 (Virtual Hyperedge Aggregation Constraint)
為了兼顧記憶體內圖譜遍歷的最優 CPU 效能，以及磁碟儲存與大語言模型（LLM）對話時的極致 Token 節省，系統採取「記憶體內標準二元有向邊，序列化/傳輸時虛擬超邊聚合」的混血架構：
- **記憶體內 (In-Memory)**：保持 `petgraph::graph::DiGraph` 標準二元有向邊的拓撲結構，確保最短路徑與廣度優先搜尋（BFS）等核心算法不受超圖複雜度干擾，維持毫秒級的物理運算效能。
- **序列化輸出 (Serialization Output)**：在寫出到 `.json` 或 `.toon` 時，編碼器必須**主動將「同一個 `source`、`relation` 與 `confidence`」的邊進行關係聚合**，將多條一對一扁平邊壓縮為一對多的「虛擬超邊（Virtual Hyperedge）」結構。

#### 1. `.toon` 格式中的超邊聚合
在 `.toon` 格式中，原本的 `target` 欄位升級為複數的 `targets` 欄位。多個目標節點 ID 之間使用 **`|` (Pipe)** 字元進行緊湊分隔：
```text
edges[1,]{source,targets,relation,confidence,source_file,source_location}
src/app.rs,src/db.rs|src/cache.rs|src/config.rs,imports,EXTRACTED,src/app.rs,src/app.rs:5
```

#### 2. `.json` (graph.json) 格式中的超邊聚合
為了保持與 Python 舊版 HTML 視覺化渲染器的高度相容，在導出 `.json` 格式時，可支援將 `target` 字段在序列化層面表示為 `targets` 陣列（若環境不支援，則解碼器在載入時必須具備能同時解讀 `target: String` 與 `targets: Array<String>` 的防禦性容錯能力）：
```json
{
  "source": "rust:main::run_server",
  "targets": [
    "rust:db::init_db",
    "rust:cache::get_redis"
  ],
  "relation": "calls",
  "confidence": "EXTRACTED",
  "source_file": "src/main.rs",
  "source_location": "src/main.rs:42"
}
```

#### 3. 反序列化還原 (Deserialization Expansion)
在讀取 `.json` 或 `.toon` 圖譜檔案載入回記憶體時，解碼器（Decoder）必須自動將聚合的 `targets`（以 `|` 分割的字串或 JSON 陣列）**展開（Flatten）**回標準的一對一扁平二元有向邊，無縫裝載進 Petgraph 數據結構中。此展開過程對上層圖運算 API 100% 隱蔽透明，完全保障向後相容。
