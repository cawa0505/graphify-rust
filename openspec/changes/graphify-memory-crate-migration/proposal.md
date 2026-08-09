## Why

`graphify-llm` currently mixes two distinct responsibilities in one crate:
the long-term memory subsystem (Qdrant-backed vector memory, embeddings,
`plugin_memory.rs` domain storage) and the LLM provider pipeline (auto-rotating
API keys, provider failover, GBNF, semantic-link extraction). The
memory-plugin-integration-v1 change already stabilized the memory API boundary
(`MemorySearcher`, `MemoryQueryService`, `PluginDomainMemory`,
`plugin_collection_name`). The next step is to give the memory subsystem its
own crate so the two responsibilities can evolve independently, and to expose
the LLM pipeline as a reusable gateway contract for native plugins.

The v2.0-alpha architecture supplement
(`docs/ref/graphify-v2-alpha-architecture-supplement.md`) confirms this
direction: `graphify-memory` is the pure embedding/vector-store infrastructure,
`graphify-llm` survives as the general LLM gateway, and plugins receive a
`PluginContext` with optional access to a `CoreLlmProvider` while remaining
free to bring their own dedicated models.

## What Changes

- Split `graphify-llm` into two crates:
  - `graphify-memory` (new): `memory.rs`, `plugin_memory.rs`, and the memory
    config subset (`QdrantConfig`, `EmbeddingConfig`, `LongTermMemoryConfig`,
    `MemoryConfig`).
  - `graphify-llm` (kept): `pipeline.rs` (`AutoRotatePipeline`), `gbnf.rs`,
    provider types, `ExtractionConfig`, `ShortTermMemoryConfig`, and the new
    `CoreLlmProvider` gateway contract.
- Decouple `MemoryConfig` from `LLMConfig` in place first so memory no longer
  depends on chat/extraction config, then move the memory modules.
- Add the minimal gateway skeleton in `graphify-llm`:
  - `CoreLlmProvider` trait (`complete` / `chat`) with `AutoRotatePipeline` as
    the default implementation.
  - `PluginContext { memory, llm, workspace_key }` skeleton used by native
    plugins. No multi-model routing and no plugin-specialized model clients in
    this change (dedicated models such as Shieldstral-3B remain Phase 7 work).
- Update `graphify-mcp` and `graphify-cli` dependencies and imports to the new
  crate layout.
- Preserve all existing behavior: the move is mechanical; tests, config
  compatibility, and CLI/MCP surfaces stay byte-identical.

## Capabilities

- memory-crate-extraction: extract the memory subsystem into `graphify-memory`.
- llm-gateway-contract: `CoreLlmProvider` trait + `PluginContext` skeleton in
  `graphify-llm`.

## Non-Goals

- No multi-model routing, no plugin-specialized model clients.
- No plugin write access to core memory (the read-only boundary from
  memory-plugin-integration-v1 stays).
- No SQLite global registry, no Qdrant local-embedded dual-track fallback
  (RFC-0004 scope, own OpenSpec changes).
- No change to the `GraphifyPlugin` trait in `graphify-core`.
