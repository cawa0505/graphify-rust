# Design — graphify-memory crate migration

## Overview

Split `graphify-llm` along the responsibility boundary confirmed by the
v2.0-alpha architecture supplement: memory infrastructure becomes its own
crate, the LLM provider pipeline stays as the general gateway. The move is
mechanical; behavior must not change.

## Target crate layout

| Crate | Owns | Moved from |
| ----- | ---- | ---------- |
| `graphify-memory` (new) | `memory.rs` (QdrantMemoryStore, MemorySearcher, MemoryNode, sync), `plugin_memory.rs` (PluginDomainMemory, plugin_collection_name), memory config subset | `graphify-llm/src/` |
| `graphify-llm` (kept) | `pipeline.rs` (AutoRotatePipeline), `gbnf.rs`, Provider/ProviderType, ExtractionConfig, ShortTermMemoryConfig, new `CoreLlmProvider` + `PluginContext` | stays |
| `graphify-core` | `GraphifyPlugin` trait, `PluginMemoryEnvelope<T>` (canonical, re-exported), workspace_key types | unchanged, zero deps |
| Native plugins (Phase 7) | opendoc / review / handoff | depend on core + graphify-memory only |

The envelope-in-core / storage-in-memory split from memory-plugin-integration-v1
already exists and stays. `PluginMemoryEnvelope` does not move to the memory
crate.

## Migration order

1. **Config decoupling (land green independently)** — highest-risk step.
   Extract `MemoryConfig` ownership: `QdrantMemoryStore::new` and constructors
   take memory-scoped config only; `LLMConfig` stops owning memory fields.
   Update callers (`graphify-mcp/src/memory_query.rs`, cli memory paths) in the
   same step. Compile + tests + clippy green before moving code.
2. **Crate extraction (mechanical, reviewable)** — create `graphify-memory`,
   move `memory.rs` + `plugin_memory.rs` + memory config. Update
   `graphify-mcp` / `graphify-cli` Cargo.toml and imports.
3. **Gateway skeleton** — add `CoreLlmProvider` trait (`complete` / `chat`,
   `LlmError`), implement it for `AutoRotatePipeline`, add `PluginContext
   { memory, llm, workspace_key }`. No routing, no plugin-specialized models.
4. **Verify + archive** — full workspace build/test/clippy, docs sync, archive
   the change.

## Key decisions

- **Extract, don't rename**: `graphify-llm` survives as the gateway; the change
  is the extraction of `graphify-memory`. Lower churn, keeps the gateway name
  meaningful.
- **Gateway trait lives in graphify-llm**: plugins that need the shared LLM
  depend on `graphify-llm`'s contract; plugins that don't (opendoc, handoff)
  stay core + memory only. Trait stays object-safe (no async fn in trait;
  existing sync facade pattern in `graphify-mcp/src/memory_query.rs`).
- **`ExtractionConfig` stays in graphify-llm** for now: it feeds the pipeline's
  LLM-assisted extraction. Whether it drifts toward core is flagged for a later
  change, not decided here.
- **`PluginDomainMemory` backend stays file-backed**: RFC-0004 mandates
  per-plugin Qdrant collections, but that is a feature delta; keeping it
  separate prevents the rename from never landing.

## Open items [待討論]

- `GraphifyMemoryEngine` write capability: the v2 supplement shows plugins
  passing content for the memory engine to vectorize/store, but the approved
  memory-plugin-integration-v1 boundary is read-only core memory. Whether
  `GraphifyMemoryEngine` gains a system-managed write path is a separate
  decision, not part of this change.
- `PluginContext.memory` typing: the exact engine trait shape depends on the
  write-capability decision above; this change provides the `llm` + 
  `workspace_key` skeleton and types `memory` as the existing restricted
  searcher.
