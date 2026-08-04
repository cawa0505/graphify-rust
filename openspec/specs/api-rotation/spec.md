# API Key Rotation & Failover Specification

## 1. 目的 (Purpose)
確保 `AutoRotatePipeline` 面臨高併發、大規模語意提取請求時，當單一 Provider 遭遇限流（429 / RESOURCE_EXHAUSTED）時，能夠以「執行緒安全」、「零執行緒睡眠（Zero Sleep）」的方式，立即切換金鑰並重試，實現最高耐受度。

## 2. 規格要求 (Requirements)

### 2.1 執行緒安全輪轉要求 (Thread-Safe Rotation Requirement)
- 當金鑰配置為 `Vec<String>`（多金鑰）時，系統必須維護一組執行緒安全的原子計數器（`AtomicUsize`）。
- 每次 LLM 請求在呼叫時，透過計數器計算模除（Modulo）以取得當前 API 金鑰。
- 發生輪轉時，計數器遞增操作必須保證內存順序安全性（使用 `SeqCst` 獲取最高的一致性保證）。

### 2.2 零睡眠立即重試機制 (Zero-Sleep Immediate Retry Requirement)
- **GIVEN** Concurrent extraction 進行中，其中一個執行緒向 LLM 發出請求
- **WHEN** 該請求返回 `429 Too Many Requests` 或 `RESOURCE_EXHAUSTED` 錯誤
- **THEN** 系統必須立即調用輪轉計數器前進一位
- **AND** 當次受挫的請求不進行執行緒睡眠（No Thread Sleep），立即採用新金鑰發起重試
- **AND** 重試次數上限等於該 Provider 配置的金鑰總數。

### 2.3 穩定性退避與冷卻原則 (Stability & Failover Constraint)
- **金鑰冷卻（Cool-down Period）**：
  - 由於 429 反應的是帳號或 IP 級別限流，單純在金鑰陣列中打轉若無冷卻，容易造成多個 Key 連續因相同的 IP 級限流而崩潰。
  - 為此，每次金鑰前進（輪轉）時，皆使用最穩定的無腦全局模除法，若整組金鑰在一個提取週期內（Max Retries = 陣列長度）均宣告 429 失敗，系統判定為 IP 或帳號限流。
- **自適應退避與斷然 Failover (Adaptive Failover)**：
  - 當整組金鑰全數失效時，不原地進行盲目重試，系統必須**立刻、斷然**切換至下一順位的備用 Provider（如 Ollama）或直接安全退化。
  - 僅在備用 Provider 亦宣告失敗時，才允許引入最大上限 3 次的指數型退避（Exponential Backoff with jitter）做為最終防線，以保證最安全的資料寫入與程序不中斷。
