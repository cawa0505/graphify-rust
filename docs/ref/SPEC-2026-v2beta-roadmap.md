# Graphify v2.0-beta Feature Specification & Architecture Roadmap

> **Docs/ref 快照聲明**：以下為用戶提供的 v2.0-beta 規格原文，verbatim 保留（docs/ref 規則 #3127）。本文件是**未來願景文件**（Target Release: v2.0-beta），不代表目前已實作之功能。與現況的差異分析見各 OpenSpec change 與 roadmap 文件。

📄 文件概覽 (Document Overview)

Document ID: SPEC-2026-V2BETA
Status: Proposed / Target Scope
Target Release: Graphify v2.0-beta
Prerequisites: Graphify v2.0-alpha Core Base Complete (Petgraph 16ms AST + SQLite Global Registry + Qdrant Dual-Track Engine)

🎯 戰略定位 (Strategic Objective)

Graphify v2.0-alpha 成功奠定了 「Neuro-Symbolic 三層架構（SQLite + Petgraph + Qdrant Dual-Track）」 的鋼鐵底座與極致效能。v2.0-beta 的核心使命為 「生態開放與系統防禦（Ecosystem Openness & System Governance）」：

- 開放生態：推出官方 Python SDK (graphify-sdk)，降低 AI/LLM 開發者擴充 Graphify 生態的門檻。
- 防禦邊界：建立 Plugin Health & Admission Protocol，利用沙盒、健康探針與熔斷機制，確保不論第三方 Plugin 如何異常，Graphify Rust Core 均能維持 100% 高可用性與穩定度。

🧩 核心功能規格 (Core Feature Specifications)

## 1. Python SDK (graphify-sdk)

為 Python 生態系提供輕量、強型別且開箱即用的開發套件，使第三方開發者能快速打造特化 Plugin，無縫接收 Graphify 的.toon AST 拓撲並存取領域記憶。

### 1.1 核心特性

- Zero Heavy Dependencies：維持套件極致輕量，內部封裝與 Rust Core 的 IPC/gRPC 握手與資料序列化。
- 強型別 Contract：提供 GraphifyPlugin 基礎類別與 PluginMemoryEnvelope 裝飾器，強制符合 Core 的 Data Standard。
- 標準生命週期 Hook：內建 on_health_check() 與 inspect_toon() 回呼介面。

### 1.2 範例程式碼 (Developer Interface)

```python
from graphify_sdk import GraphifyPlugin, PluginMemoryEnvelope, Context

class SecurityAuditPlugin(GraphifyPlugin):
    Name = "custom-security-auditor"
    Version = "0.1.0"

    def on_health_check(self) -> bool:
        # 回報本地模型/資源狀態給 Rust Core Health Probe
        return self.check_local_model_ready()

    def inspect_toon(self, ctx: Context, toon_subgraph: str) -> list[PluginMemoryEnvelope]:
        # 接收 Rust Core 剪枝出的 16ms.toon AST 拓撲
        findings = self.analyze_vulnerabilities(toon_subgraph)

        # 回傳標準 Envelope，由 Core 寫入該 Plugin 專屬的隔離 Collection
        return [
            PluginMemoryEnvelope(
                workspace_key=ctx.workspace_key,
                payload={"issue": f.title, "severity": f.severity}
            ) for f in findings
        ]

if __name__ == "__main__":
    SecurityAuditPlugin().run()
```

## 2. Plugin Health & Admission Protocol (准入與健康度協議)

為了防範第三方/Python Plugin 產生死鎖、記憶體洩漏或格式錯誤，Core 實作三道動態防線：

```
┌────────────────────────────────────────────────────────────────────────┐
│ Plugin Admission Pipeline                                             │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│ 1. Manifest & Capability Handshake (靜態宣告與權限對齊)               │
│    - 檢查 API Version, Reserved Name, Collection Namespace             │
└───────────────────────────┬────────────────────────────────────────────┘
                            │ Pass
                            ▼
┌────────────────────────────────────────────────────────────────────────┐
│ 2. Runtime Passive Health Probe (10ms 被動動態健康度探針)              │
│    - CLI / TUI 啟動時 Ping Endpoint / Model Readiness                  │
└───────────────────────────┬────────────────────────────────────────────┘
                            │ Healthy
                            ▼
┌────────────────────────────────────────────────────────────────────────┐
│ 3. Execution Timeout & Circuit Breaker (執行熔斷與沙盒機制)            │
│    - Hard Timeout (500ms)                                              │
│    - Envelope Schema Strict Validation                                 │
│    - 連續失敗 > 3 次 ──> 自動觸發 Quarantine 隔離                      │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 准入機制 (Admission Handshake)

- Manifest 簽名與相容性檢查：Plugin 註冊時審查 api_version，不匹配者拒絕載入。
- Namespace 實體隔離：自動於 SQLite 註冊並配發 Qdrant 隔離空間 `graphify_plugin_<plugin_id>`，嚴禁跨 Namespace 寫入。

### 2.2 被動健康度探針 (Passive Health Probe)

- 拒絕常駐 Thread 背景 Polling。觸發時機：僅在 CLI 下達指令或 TUI 啟動時，發起 <10ms 超輕量 Ping。
- 三態回報 (Status State Machine)：
  - HEALTHY（綠燈）：完全正常，納入執行管道。
  - DEGRADED（黃燈）：核心可用但局部資源缺失（例如依賴的模型離線），降級執行。
  - UNAVAILABLE（紅燈）：服務中斷，本次執行直接 Bypass（跳過）。

### 2.3 熔斷與沙盒機制 (Circuit Breaker & Execution Timeout)

- 500ms Hard Timeout：Plugin 執行超過 500ms 強制中斷，避免卡住主線程。
- Strict Schema Filter：回傳 Payload 若不符合 PluginMemoryEnvelope 標準格式直接捨棄並記錄 Warning。
- Auto-Quarantine (自動隔離)：連續失敗達 3 次自動升級為 QUARANTINED 狀態，阻止其後續調用，直到使用者手動重置。

## 3. TUI Health Console & One-Key Recovery ([F5])

在 v2.0-alpha 的 TUI Stage 2 基礎上，擴充 Plugin 健康度儀表板與一鍵復原操作：

```
┌─ Active Plugins & Health Governance ───────────────────────────────────┐
│ [✓] graphify-plugin-opendoc (v1.2): HEALTHY (Vector DB Ready)          │
│ [!] graphify-plugin-review (v2.0): DEGRADED (Shieldstral-3B Offline)   │
│ [x] custom-security-auditor (v0.1): QUARANTINED (3x Timeout)           │
└────────────────────────────────────────────────────────────────────────┘
 [F1] Re-index AST   [F2] Sync Memory   [F5] Reset Quarantine & Probes
```

- 狀態透明化：即時顯示各 Plugin 的健康狀態與降級/隔離原因。
- [F5] 一鍵修復：清除隔離狀態（Un-quarantine），重新發起健康度探針與連線握手。

## 📊 v2.0-alpha vs v2.0-beta 比較表

| 構面 | v2.0-alpha (Core Base) | v2.0-beta (Ecosystem & Governance) |
|------|------------------------|------------------------------------|
| 主要目標 | 打通單機三層架構與極致效能 | 開放第三方生態與系統防禦熔斷 |
| Plugin 語言支援 | Native Rust Only | Native Rust + Python SDK (graphify-sdk) |
| Plugin 載入 | 受控的原生硬編碼與內建模組 | Manifest 准入握手與動態沙盒 |
| 健康檢查 | 基礎連線 Check | Passive 10ms Probe (Healthy/Degraded/Unavailable) |
| 容錯機制 | 手動 Error Handling | 500ms Timeout + 3x 失敗 Auto-Quarantine 熔斷 |
| TUI 功能 | Workspace 切換器 + 記憶體容量用量 | Plugin 治理面板 + [F5] 重置修復 |

## 🗓️ 交付里程碑 (Milestones)

- Beta-M1: 完成 graphify-sdk Python 套件原型與跨語言 IPC/gRPC Gateway 介面。
- Beta-M2: 實作 Rust Core 端的 500ms Execution Timeout、Schema Filter 與 Circuit Breaker。
- Beta-M3: 整合 Passive Health Probe 與 SQLite 狀態更新，並解鎖 TUI [F5] 修復功能。
- Beta-M4: 端到端 Integration Test（驗證 Python 異常 Plugin 被自動隔離且不影響 Core AST 與.toon 導出）。
