## Purpose

Enables high-performance vector ingestion and RAG querying via Qdrant optimization features (HNSW indexing toggle and payload pre-filtering) and incremental codebase synchronization (SHA256 file hashing).

## ADDED Requirements

### Requirement: Qdrant Indexing Threshold Management
The indexing process SHALL allow disabling HNSW vector index construction during massive batch uploads by temporarily setting `optimizers_config.indexing_threshold` to `0`, and then restoring it on completion.

#### Scenario: Deferring HNSW index creation during ingestion
- **WHEN** the `index` operation starts processing a batch of nodes
- **THEN** the system SHALL send a PUT/PATCH request to Qdrant disabling vector indexing
- **AND** on batch completion, the system SHALL restore the configured indexing threshold

### Requirement: Optional gRPC Transport Mode
The long-term memory store SHALL optionally connect to Qdrant via the binary gRPC protocol using port `6334` for high-throughput uploading and low-latency payload serialization.

#### Scenario: Connecting to Qdrant via gRPC
- **WHEN** `long_term.qdrant.grpc` is set to `true` in the configuration
- **THEN** the client SHALL initialize a gRPC connection to the server on port `6334`
- **AND** perform all ingestion and point search operations over gRPC instead of HTTP

### Requirement: Incremental Indexing and Change Tracking
The system SHALL compute SHA256 hashes of all source files to identify and index only the files modified since the last successful indexing run.

#### Scenario: Re-indexing on unmodified codebase
- **WHEN** the `index` subcommand is executed without changes to file contents
- **THEN** the system SHALL skip AST extraction and embedding generation for unmodified files
- **AND** only keep or refresh metadata in the local graph representation

### Requirement: Payload Metadata Pre-filtering and Indexing
The system SHALL write structural node attributes (`source_file`, `kind`, `language`) into the Qdrant payload and apply strict Keyword/Integer pre-filtering during RAG queries.

#### Scenario: Performing filtered vector search
- **WHEN** a user queries the codebase for a specific kind of node (e.g., class or struct)
- **THEN** the search request SHALL include a Qdrant payload pre-filter on the `kind` field
- **AND** Qdrant SHALL return matching nodes with 100% filter precision
