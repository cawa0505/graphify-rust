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

### 2.3 相容性退化原則 (YAGNI Constraint)
- 廢除 Python 版的 `hyperedges`（多對多超邊關係），以保持 Rust 版極簡且高效的單向圖譜拓撲，所有對齊工具一律透過基本單邊（`edges`）進行相容。
