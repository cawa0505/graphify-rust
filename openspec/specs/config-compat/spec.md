# Configuration Compatibility & Migration Specification

## Purpose
定義系統如何以符合 XDG Base Directory 規範（TOML 格式）為首選，同時優雅、無痛地相容、讀取、並單向自動升級舊有 Python 版在 `~/.graphify/config.json` 的環境配置。

## Requirements

### 2.1 配置載入層級規範 (Config Loader Hierarchy Requirement)
啟動時，載入器 MUST 依照以下優先順序讀取配置，禁止中途崩潰：

#### Scenario: XDG TOML 為首選
- **GIVEN** `~/.config/graphify/config.toml` 存在
- **WHEN** 系統啟動載入配置
- **THEN** 系統 SHALL 讀取該 TOML 配置
- **AND** SHALL NOT 讀取舊版 JSON

#### Scenario: 環境變數覆寫優先
- **GIVEN** 環境變數 `GRAPHIFY_CONFIG_PATH` 指向任意路徑
- **WHEN** 系統啟動載入配置
- **THEN** 系統 SHALL 讀取環境變數指定的路徑

#### Scenario: 舊版 JSON 無縫遷移
- **GIVEN** XDG 與環境變數路徑皆不存在，且 `~/.graphify/config.json` 存在
- **WHEN** 系統啟動載入配置
- **THEN** 系統 SHALL 無縫載入該 JSON 並在記憶體中轉換
- **AND** SHALL 寫入 `~/.config/graphify/config.toml` 持久化
- **AND** SHALL NOT 修改或刪除原始 `~/.graphify/config.json`
1.  **XDG 標準路徑**：`~/.config/graphify/config.toml` (TOML 格式)。
2.  **環境變數覆寫**：`GRAPHIFY_CONFIG_PATH` 指定之任意路徑。
3.  **舊版相容與自動升級**：若 1 與 2 皆不存在，但本機存在 `~/.graphify/config.json`：
    - 系統必須無縫載入該 JSON 配置。
    - 在記憶體中進行結構轉換（詳見下述）。
    - 建立 `~/.config/graphify/` 目錄，將轉換後的配置以 TOML 格式持久化寫入 `~/.config/graphify/config.toml`。
    - 於 `stderr` 提示：`[graphify] Migrated legacy JSON config to ~/.config/graphify/config.toml`。
    - **嚴格限制**：原 `~/.graphify/config.json` 必須保持完好，不得做任何修改或刪除。

### 2.2 欄位映射與轉換規則 (Field Translation Requirements)
載入舊版 JSON 時，系統 SHALL 依以下規則轉換欄位；未知欄位 SHALL 直接丟棄（YAGNI），且 SHALL NOT 產生任何遷移副作用寫回原始 JSON。

#### Scenario: backend 優先權映射
- **GIVEN** 舊 JSON 含 `"backend": "gemini"`
- **WHEN** 進行配置轉換
- **THEN** 對應 Provider 在新 TOML 的 `priority` SHALL 為 `1`

#### Scenario: 多金鑰展平
- **GIVEN** 舊 JSON 的 `providers.api_key` 為 `["KEY_1", "KEY_2"]`
- **WHEN** 進行配置轉換
- **THEN** 新配置 SHALL 展平為支援輪轉的多金鑰形式

#### Scenario: 未知欄位直接丟棄
- **GIVEN** 舊 JSON 含系統不認識的欄位
- **WHEN** 進行配置轉換
- **THEN** 未知欄位 SHALL 直接丟棄
- **AND** SHALL NOT 寫入新 TOML，防止配置膨脹

<!-- 原始欄位映射規則詳述 -->
載入 JSON 時，記憶體對應與轉換規則如下：
- `"backend": "gemini"` 字串：對應之 Provider 在新 TOML 的 `priority` 必須設為最優先（`priority = 1`）。
- `"providers"` 的 `"api_key"` 陣列：
  - 若 `api_key` 為 `["KEY_1", "KEY_2"]`，在記憶體與新 TOML 中必須展平為支援輪轉的多金鑰（自帶 `AtomicUsize` 計數器，優先權自動遞增）。
- `"extraction"` 屬性：其內置 `chunk_size`、`max_concurrency` 必須直接對應至 `LLMConfig.extraction` 子節點中，由核心提取調度器直接讀取。
- **未知欄位處理規則 (YAGNI & Safety Constraint)**：
  - 舊 JSON 若含有未知欄位，自動升級時不予理會、直接丟棄，排除任何冗餘配置、降低維護複雜度並防範配置膨脹（Container Image Bloat & Memory Efficiency）。
