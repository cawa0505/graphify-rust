## Why

To optimize vector ingestion performance and RAG query efficiency for massive codebases, GraphifyRust needs a transition from standard serial REST indexing to high-performance batch uploading and smart pre-filtering. Standard sequential indexing over high-latency networks wastes CPU, GPU, and network handshakes. Incremental indexing via file hashing prevents redundant embedding generation, and metadata indexing on Qdrant prevents brute-force scans on million-vector databases.

## What Changes

- **Qdrant HNSW Tuning**: Temporarily disable HNSW vector indexing (`indexing_threshold = 0`) before a batch upload, and restore/trigger background optimization on completion.
- **gRPC Integration**: Option to route vector operations over the gRPC protocol using port `6334` via `qdrant-client` for minimal transport serialization overhead.
- **Incremental Indexing**: Calculate SHA256 hashes of files to only extract, embed, and index files that have actually changed since the last run.
- **Payload Pre-filtering**: Automatically index payload fields (`source_file`, `kind`, `language`) in Qdrant and apply pre-filters during queries.
- **`[待討論]` Preservation**: Explicitly preserve undecided architectural trade-offs in documentation.

## Capabilities

### New Capabilities
- `qdrant-perf-incremental`: Implements HNSW indexing deferral, optional gRPC transport, SHA256-based incremental file tracking, and payload pre-filtering in Qdrant.

### Modified Capabilities
- `architecture`: Integrate the long-term memory performance and incremental indexing specification.

## Impact

- `graphify-llm`: Introduces `qdrant-client` crate, payload indexing logic, hash tracking, and indexing threshold control.
- `graphify-cli`: Updates `index` command to support incremental diff checks and graceful Qdrant state management.
- Configuration: Updates `config.toml` to support gRPC port toggles, threshold tuning, and index target kind filters.
