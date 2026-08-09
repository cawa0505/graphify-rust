# memory-crate-extraction

## Purpose

Extract the long-term memory subsystem out of `graphify-llm` into a dedicated
`graphify-memory` crate so memory infrastructure (embeddings, vector stores,
plugin-domain storage) evolves independently of the LLM provider pipeline.

## Requirements

### Requirement: Memory subsystem extraction

The workspace MUST contain a `graphify-memory` crate that owns the long-term
memory modules previously living in `graphify-llm`: the Qdrant-backed memory
store (`memory.rs`), the plugin-domain memory storage (`plugin_memory.rs`), and
the memory configuration subset (`QdrantConfig`, `EmbeddingConfig`,
`LongTermMemoryConfig`, `MemoryConfig`).

The extraction MUST NOT change observable behavior: existing tests, config
compatibility, and the CLI/MCP surfaces remain identical.

#### Scenario: Workspace builds after the split

- **WHEN** the workspace is built with the new crate layout
- **THEN** `graphify-memory` compiles with the moved memory modules, and
  `graphify-llm` keeps only the provider pipeline and config it owns

#### Scenario: Memory no longer reads chat config

- **WHEN** the memory store is constructed after the config decoupling
- **THEN** it requires only memory-scoped configuration, not the full
  `LLMConfig` chat/extraction struct

### Requirement: Config decoupling before extraction

The memory configuration MUST be decoupled from `LLMConfig` before any module
move: `QdrantMemoryStore::new` and related constructors MUST take memory-scoped
configuration only, and `LLMConfig` MUST stop owning the memory fields.

The decoupling MUST land green independently (compile, tests, clippy) before
the crate move begins.

#### Scenario: Callers construct the memory store

- **WHEN** `graphify-mcp` / `graphify-cli` construct the memory store
- **THEN** they pass memory-scoped configuration derived from the loaded config,
  and no caller depends on the memory fields inside `LLMConfig`

### Requirement: Dependency and import updates

`graphify-mcp` and `graphify-cli` MUST depend on `graphify-memory` for memory
functionality and on `graphify-llm` only for the provider pipeline, with all
imports updated accordingly.

#### Scenario: Imports resolve to the new crates

- **WHEN** `graphify-mcp` uses `MemoryQueryService` and `graphify-cli` uses the
  memory store
- **THEN** those symbols resolve from `graphify-memory`, and no remaining code
  imports memory types from `graphify-llm`
