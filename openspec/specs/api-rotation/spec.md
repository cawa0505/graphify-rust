# API Key Rotation & Failover Specification

## Purpose
確保 `AutoRotatePipeline` 面臨高併發、大規模語意提取請求時，當單一 Provider 遭遇限流（429 / RESOURCE_EXHAUSTED）時，能夠以「執行緒安全」、「零執行緒睡眠（Zero Sleep）」的方式，立即切換金鑰並重試，實現最高耐受度。

## Requirements

### 2.1 執行緒安全輪轉要求 (Thread-Safe Rotation Requirement)
當金鑰配置為多金鑰陣列時，系統 SHALL 以執行緒安全的原子計數器（`AtomicUsize`）在請求間輪轉 API 金鑰，且計數器遞增 SHALL 保證記憶體順序安全性（`SeqCst`）。

#### Scenario: 多金鑰模除取鍵
- **GIVEN** Provider 配置了多把 API 金鑰（`Vec<String>`）
- **WHEN** 任一 LLM 請求需要選擇金鑰
- **THEN** 系統 SHALL 以原子計數器模除（Modulo）取得當前金鑰
- **AND** 輪轉遞增 SHALL 使用 `SeqCst` 記憶體順序

### 2.2 零睡眠立即重試機制 (Zero-Sleep Immediate Retry Requirement)
當 LLM 請求遭遇限流錯誤時，系統 SHALL 立即以前進一位的金鑰重試，且 SHALL NOT 進行任何執行緒睡眠；重試次數上限 SHALL 等於該 Provider 的金鑰總數。

#### Scenario: 429 立即輪轉重試
- **GIVEN** Concurrent extraction 進行中，其中一個執行緒向 LLM 發出請求
- **WHEN** 該請求返回 `429 Too Many Requests` 或 `RESOURCE_EXHAUSTED` 錯誤
- **THEN** 系統 SHALL 立即調用輪轉計數器前進一位
- **AND** 當次受挫的請求 SHALL NOT 進行執行緒睡眠，立即採用新金鑰發起重試
- **AND** 重試次數上限 SHALL 等於該 Provider 配置的金鑰總數

### 2.3 穩定性退避與冷卻原則 (Stability & Failover Constraint)
當整組金鑰於單一提取週期內全數宣告 429 失敗時，系統 SHALL 判定為 IP 或帳號級限流，立即 Failover 至下一順位備用 Provider；僅備用 Provider 亦失敗時，才 SHALL 引入最大 3 次的指數型退避（含 jitter）作為最終防線。

#### Scenario: 整組金鑰耗盡觸發 Failover
- **GIVEN** 某 Provider 配置了 N 把金鑰
- **WHEN** 一個提取週期內全部 N 把金鑰皆回傳 429
- **THEN** 系統 SHALL 判定為 IP 或帳號級限流
- **AND** 系統 SHALL 立即切換至下一順位的備用 Provider（如 Ollama）或安全退化
- **AND** SHALL NOT 原地盲目重試

#### Scenario: 備用 Provider 失敗的最終退避
- **GIVEN** 主 Provider 整組金鑰已耗盡並已 Failover
- **WHEN** 備用 Provider 亦宣告失敗
- **THEN** 系統 SHALL 施加最多 3 次指數型退避（Exponential Backoff with jitter）
- **AND** 退避期間 SHALL 保證資料寫入安全與程序不中斷

<!-- 舊文保留於下方，作為冷卻設計依據 -->
- **金鑰冷卻（Cool-down Period）**：
  - 由於 429 反應的是帳號或 IP 級別限流，單純在金鑰陣列中打轉若無冷卻，容易造成多個 Key 連續因相同的 IP 級限流而崩潰。
  - 為此，每次金鑰前進（輪轉）時，皆使用最穩定的無腦全局模除法，若整組金鑰在一個提取週期內（Max Retries = 陣列長度）均宣告 429 失敗，系統判定為 IP 或帳號限流。
- **自適應退避與斷然 Failover (Adaptive Failover)**：
  - 當整組金鑰全數失效時，不原地進行盲目重試，系統必須**立刻、斷然**切換至下一順位的備用 Provider（如 Ollama）或直接安全退化。
  - 僅在備用 Provider 亦宣告失敗時，才允許引入最大上限 3 次的指數型退避（Exponential Backoff with jitter）做為最終防線，以保證最安全的資料寫入與程序不中斷。
