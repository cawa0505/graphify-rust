## Context

Currently, the vector database ingestion in `graphify-llm/src/memory.rs` is purely REST-based, non-batched, lacks HNSW optimization during heavy bulk uploading, and has no incremental ingestion logic (which causes redundant extraction and embedding generation on unmodified files). Additionally, we want to optionally support the gRPC protocol using port `6334` via `qdrant-client` to bypass heavy REST serialization costs.

## Goals / Non-Goals

**Goals:**
- Provide a configurable setting to switch between REST (`6333`) and gRPC (`6334`) protocols for Qdrant operations.
- Temporarily set `optimizers_config.indexing_threshold` to `0` during bulk indexing, and restore it back to the configured default on completion (background HNSW build).
- Implement a SHA256-based file change detection layer to prevent re-indexing unmodified files.
- Enable automatic index creation on metadata fields (`source_file`, `kind`, `language`) in Qdrant and leverage payload filters during search queries.
- Explicitly preserve all `[待討論]` architecture markers.

**Non-Goals:**
- Supporting process-level environment variable persistence for credentials (dynamic, memory/file-bound settings remain strictly enforced).
- Deleting or degrading existing Ollama and FastEmbed vectorization capabilities.

## Decisions

### 1. Optional gRPC Transport Integration
- **Decision**: Introduce `qdrant-client` as an optional dependency (conditional compilation feature or a fallback client) or natively wrap it. Given the user's explicit open port on `6334`, we will configure the client to support connecting via gRPC when specified.
- **Alternative Considered**: Writing raw gRPC protobuf code using Tonic. This was rejected due to massive engineering and maintenance overhead. Wrapping `qdrant-client` is the standard library approach.

### 2. High-Performance Bulk HNSW Deferral
- **Decision**:
  - Prior to upserting points, call `PUT/PATCH /collections/{name}` with `optimizers_config: { indexing_threshold: 0 }`.
  - Perform the batch uploads (points chunked into packages of 64–256).
  - Post-upsert, patch `indexing_threshold` back to the default config value (e.g. `20000`).
- **Rationale**: Reduces client-side ingestion blocking and eliminates redundant CPU/GPU HNSW graph building on the server during point loading.

### 3. SHA256-Based Incremental Syncing
- **Decision**: Keep an in-memory or dynamic file-hash snapshot index. When executing `index`, skip files whose hash matches the active cache database. If a file is modified, delete existing points belonging to that `source_file` in Qdrant prior to upserting the new ones.
- **Alternative Considered**: Standard timestamp (mtime) checks. Rejected because file timestamps are easily modified by Git checkouts and do not guarantee actual content differences.

## Risks / Trade-offs

- **[Risk]**: gRPC binary dependency increases the final binary size and compilation time.
  - **Mitigation**: We can hide `qdrant-client` under a Cargo feature flag if required, keeping the core workspace lightweight.
- **[Risk]**: REST to gRPC connection port mismatch.
  - **Mitigation**: If `grpc` is set to `true`, automatically fall back to REST `6333` with an explicit terminal notice if port `6334` is unreachable.

## [待討論]

- **快照快取位置與機制**：增量更新的檔案雜湊快照，應記錄在本地 `.graphify_cache` / `.toon` 檔案中，還是由各圖獨立的 metadata 結構中處理？
- **HNSW 重建的非同步等待行為**：上傳完成後發送 PATCH 還原 threshold，Homelab 預設是否應設為 `wait=false`（背景進行），以防 CLI 連線逾時（Timeout）？
