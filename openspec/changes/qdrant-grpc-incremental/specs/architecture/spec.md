## MODIFIED Requirements

### Requirement: Local Embeddings & Qdrant Vector Store Architecture
To support thread-safe long-term memory (LTM) without cloud token overhead, the system SHALL support local semantic embedding generation and indexing with Qdrant.

#### Scenario: Generating embeddings using Ollama bge-m3
- GIVEN a structured `.toon` node data payload
- WHEN a vector embedding request is dispatched to a local Ollama instance configured with `bge-m3`
- THEN the embedding vector of exactly `1024` dimensions SHALL be generated under the 8192-token context window limit
- AND indexing of the vector into the Qdrant `graphify_memory` collection using `Cosine` distance distance metric SHALL succeed with 100% precision.

#### Scenario: Embedded embedding fallback using ONNX (ort)
- GIVEN an environment without Ollama installed or running
- WHEN the embedded embedding client is initialized
- THEN it SHALL load a local ONNX model `BAAI/bge-m3` via CPU-bound matrix bindings
- AND output dense vectors of exactly `1024` dimensions for local indexing without network requests.

## [待討論]

- 是否保留 `hyperedges`（超超關係）的支援？（Python 版中用於表示一個文件對多個節點的共同引用）
- 遠端備援 API 是否要設定預設的限額（Quota）以防 Token 溢出費用過高？
- 本地小模型 GBNF 規則檔是否需要根據不同語言的 Parser 分開訂製？
- **Phase 2 Qdrant LTM 實作時程**：待 Python 舊版完全退休後，優先啟動基於 Ollama (bge-m3) 的 HTTP 嵌入管線實作，並將嵌入式 ONNX (ort) 動態編譯作為 Phase 3 社群開源包裝的可行性方案。
- **快照快取位置與機制**：增量更新的檔案雜湊快照，應記錄在本地 `.graphify_cache` / `.toon` 檔案中，還是由各圖獨立的 metadata 結構中處理？
- **HNSW 重建的非同步等待行為**：上傳完成後發送 PATCH 還原 threshold，Homelab 預設是否應設為 `wait=false`（背景進行），以防 CLI 連線逾時（Timeout）？
