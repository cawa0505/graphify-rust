補充 v2.0 alpha 的架構

🎯 核心原則：區分「基礎設施 (Infra)」與「業務邏輯 (Domain)」
Graphify-memory（Core 層）：

職責：專注做「向量記憶與 Embedding 基礎設施」。

LLM / Embedding 角色：它封裝的是 System-Level Embedding Provider（例如將 Content 轉為 Vector 的模型，如 Text-Embedding-3-Small / Ollama Embeddings），以及基礎的 Vector DB 讀寫。

提供給 Plugin 的 API：提供統一的 GraphifyMemoryEngine Trait。Plugin 不需要知道背後是用哪個 Embedding 模型，只需傳入 content，由 Core/Memory 自動負責 Vectorize 並存存。

Core 提供「共用 LLM Service Gateway」（給一般 Plugin 降級與調用）：

職責：在 Core（或專屬的 graphify-llm / graphify-provider crate）提供一個可全域設定的通用 LLM 呼叫 Client（例如預設連到使用者的 Local Ollama、Claude API 或 OpenAI API）。

好處：普通 Plugin（例如只想要摘要一段文字、或是做簡單 Code Explanation 的 Plugin）不用自己再寫一遍 API Key 解析、HTTP 連線池、Retry 機制與 Token 計數，直接調用 Core 提供的方法即可。

允許 Plugin「自行開發 / 引入專用 Model」（極致特化與自由度）：

職責：當 Plugin 有極度特化的 AI 需求時，完全允許 Plugin 自行引入專用模型（Dedicated / Specialized Models）。

經典案例（以你剛才提的 Shieldstral-3B 為例）：

Graphify-plugin-review 如果想要用 Shieldstral-3B 做本地 Guardrail/Security 審查，它不應該強制套用 Core 的通用 LLM 設定。

Review Plugin 可以選擇：

Option A（預設）：沒有特別指定時，Fallback 調用 Core 提供的通用 LLM Service。

Option B（特化）：Plugin 自行在 Config 宣告 model: "mistralai/shieldstral-1.0-3b"，自行管理該微型模型的 Prompt 與 Special Tokens 推論邏輯！

🏗️ 建議的架構職責分工圖 (Responsibility Matrix)
Plaintext
┌────────────────────────────────────────────────────────────────────────┐
│ Plugin Layer │
│ │
│ ┌───────────────────────────┐ ┌──────────────────────────────┐ │
│ │ graphify-plugin-opendoc │ │ graphify-plugin-review │ │
│ │ │ │ │ │
│ │ (使用 Core 通用 LLM/Memory)│ │ (可自備專用 Shieldstral-3B) │ │
│ └─────────────┬─────────────┘ └──────────────┬───────────────┘ │
└───────────────┼──────────────────────────────────────┼─────────────────┘
 │ │
 │ (標準 Service API 呼叫) │ (必要時自建專用 Client)
 ▼ ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Core Layer │
│ │
│ ┌───────────────────────────┐ ┌──────────────────────────────┐ │
│ │ graphify-memory │ │ graphify-llm (Gateway) │ │
│ │ │ │ │ │
│ │ - 負責 Embedding Provider │ │ - 負責共用 General LLM Client │ │
│ │ - 負責 Vector Stores │ │ - 負責 API Keys, Retry, Tool │ │
│ └───────────────────────────┘ └──────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────┘
💻 在 Rust 上的 Trait 與 API 設計模式
在 Core 劃分出 LlmGateway 介面，允許 Plugin 既能「輕鬆呼叫 Core」，又能「自訂覆蓋」：

Rust
// 1. Core 提供標準的 Service Context 給 Plugin
Pub struct PluginContext {
 Pub memory: Arc<dyn GraphifyMemoryEngine>, // 來自原本改名後的 graphify-memory
 Pub llm: Arc<dyn CoreLlmProvider>, // Core 提供的預設 LLM Gateway
 Pub workspace_key: String,
}

// 2. Core 提供的通用 LLM Gateway Trait
#[async_trait]
Pub trait CoreLlmProvider: Send + Sync {
 Async fn complete(&self, prompt: &str) -> Result<String, LlmError>;
 Async fn chat(&self, messages: Vec<ChatMessage>) -> Result<String, LlmError>;
}

// 3. Plugin 內部的彈性設計範例 (以 Review Plugin 為例)
Pub struct ReviewPlugin {
 // 若有自訂特化模型 (如 Shieldstral)，則使用自己的 Client；否則為 None，退回使用 Core
 Custom_safety_model: Option<DedicatedShieldstralClient>,
}

Impl ReviewPlugin {
 Pub async fn execute_review(&self, ctx: &PluginContext, toon_diff: &str) -> Result<ReviewResult, Error> {
 If let Some(safety_model) = &self.custom_safety_model {
 // 模式 A：使用 Review Plugin 自行開發/掛載的 Shieldstral-3B 特化模型
 safety_model.audit_guardrail(toon_diff).await
 } else {
 // 模式 B：退回使用 Core 傳進來的全域通用 LLM Gateway
 ctx.llm.complete(&format!("Review this toon: {}", toon_diff)).await
 }
 }
}
💡 總結這個改動帶來的效益
Graphify-memory 職責單一化：更名後，它就是純粹的「記憶與向量數據庫」，只留 Embedding 與 Vector 操作，乾淨利落。

零重複造輪子 (DX 最優)：90% 的普通 Plugin 開發者不需要自己關心 LLM 怎麼連、API Key 怎麼帶，直接用 ctx.llm 或 ctx.memory 就能開箱即用。

保留頂級擴充彈性 (Extreme Power Users)：像 graphify-plugin-review 這種需要安全稽核模型（如 Shieldstral）、或是需要特殊 Function Calling / Local Quantized SLM 的重度 Plugin，完全可以自帶模組，不受 Core 限縮。

這樣劃分，系統在架構維護性（Maintainability）與 Plugin 自由度（Flexibility）上就達到了最完美的平衡！

關於 graphify-plugin-opendoc
💡 為什麼 OpenDoc 根本不需要（甚至該避免）依存 Core LLM？1. OpenDoc 自帶獨立的解析與向量 pipeline (Self-Contained Ingestion)OpenDoc 處理的是非常特定的領域資料（如 Excel 試算表算式、PDF 規範條文、Word 需求書）。它有自己的 Document Ingestion Engine（解析段落、表格、鏈結）。它有自己的 Chunking & Domain Vector Provider（針對文檔優化的 Embedding，如專門讀表格或長文檔的模型）。它直接將結果寫入專屬的 graphify_plugin_opendoc Collection。2. OpenDoc 的核心是「確定性 Mapping」，而不是「生成 (Generation)」OpenDoc 最強大的地方在於：$$\text{文檔段落 (Doc Chunk)} \xrightarrow{\text{語意向量/關鍵字}} \text{AST 符號 (Symbol)} \xrightarrow{\text{Petgraph 16ms}} \text{衝擊拓撲 (.toon)}$$這中間最關鍵的一步是 「對照 (Mapping & Alignment)」：找到文件中的這段需求對應到哪一個 AST 節點（例如 MemoryConfig）。這一步完全是向量距離計算 + 正則/語意對照，完全不需要呼叫大型語言模型（LLM）去生成內文！🏗️ 這樣調整後的插件 LLM / Provider 依存矩陣把 OpenDoc 的定位釐清為「純對照 (Pure Mapping) 引擎」後，三個原生插件對 LLM / Model 的依賴關係就變得非常漂亮且精準：Plaintext┌───────────────────────────┬───────────────────────────┬───────────────────────────┐
│ graphify-plugin-opendoc │ graphify-plugin-review │ graphify-plugin-handoff │
├───────────────────────────┼───────────────────────────┼───────────────────────────┤
│ 🔹 核心：純粹的對照 (Mapping)│ 🔹 核心：安全與風險稽核 │ 🔹 核心：狀態與拓撲打包 │
│ 🔹 LLM 依賴：零 (None) │ 🔹 LLM 依賴：特化 Safety │ 🔹 LLM 依賴：零 (None) │
│ 自帶文檔 Embedding 與 │ 模型 (如 Shieldstral) │ 純粹將拓撲與記憶 ID │
│ AST 節點指紋對照引擎 │ 或 Fallback 至 Core LLM│ 序列化為.toon 快照 │
└───────────────────────────┴───────────────────────────┴───────────────────────────┘
🎯 這個設計帶來的巨大優勢極致的執行速度與穩定度：opendoc 執行 Trace 與 Mapping 時完全零 LLM 推論開銷（Zero LLM Latency & Cost）。它只需要做向量檢索與 Petgraph 的 16ms 拓撲連通性分析，速度是毫秒級的，而且 100% 確定、絕不幻覺！職責極度乾淨 (High Cohesion)：OpenDoc 專注做 數據解析、特化 Embedding 與符號映射（Mapping）。Handoff 專注做 Context 狀態壓縮與序列化（Serialization）。Review 才真正需要 AI 模型的智慧（使用 Shieldstral 3B 或 Core LLM 做 Security Guardrail & Logical Audit）。
