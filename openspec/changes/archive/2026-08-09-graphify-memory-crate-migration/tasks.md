# Tasks — graphify-memory crate migration

## 1. Config decoupling

- [x] 1.1 Extract memory-scoped configuration (`QdrantConfig`, `EmbeddingConfig`,
  `LongTermMemoryConfig`, `MemoryConfig`) so memory constructors no longer take
  the full `LLMConfig`.
- [x] 1.2 Update `QdrantMemoryStore::new` and related constructors to accept
  memory-scoped config.
- [x] 1.3 Update callers (`graphify-mcp/src/memory_query.rs`, graphify-cli
  memory paths) to derive and pass memory-scoped config.
- [x] 1.4 Verify green: `cargo test --workspace`, `cargo clippy --workspace -D
  warnings`, `cargo fmt --check` before any module move.

## 2. Crate extraction

- [x] 2.1 Create `graphify-memory` crate (workspace member) with Cargo.toml
  dependencies matching the moved modules.
- [x] 2.2 Move `memory.rs` and `plugin_memory.rs` into `graphify-memory`,
  adjusting intra-crate imports.
- [x] 2.3 Move memory config types into `graphify-memory` and re-export from
  `graphify-llm` if any public path requires compatibility.
- [x] 2.4 Update `graphify-mcp` and `graphify-cli` Cargo.toml dependencies and
  `use` imports to the new crate layout.
- [x] 2.5 Verify: workspace build, tests, clippy `-D warnings`, fmt.

## 3. LLM gateway contract

- [x] 3.1 Define `CoreLlmProvider` trait (`complete`, `chat`) and `LlmError` in
  `graphify-llm`, keeping the trait object-safe.
- [x] 3.2 Implement `CoreLlmProvider` for `AutoRotatePipeline` reusing existing
  rotation/failover logic.
- [x] 3.3 Add `PluginContext { memory, llm, workspace_key }` skeleton.
- [x] 3.4 Add unit tests: gateway routes through the pipeline, context carries
  the services and workspace key, trait remains usable as `dyn` reference.

## 4. Verification and archive

- [x] 4.1 Full workspace verification: build, `cargo test --workspace`, clippy
  `-D warnings`, fmt — zero warnings.
- [x] 4.2 Regression check: memory query API, MCP tool, plugin-domain memory,
  `.toon plugin_data` tests all pass unchanged.
- [x] 4.3 Sync docs (`docs/core.md`, `docs/plugin_system.md`,
  `docs/architecture-memory-plugin.md`, crate lists) to the new layout.
- [x] 4.4 Record verification evidence and `openspec validate
  graphify-memory-crate-migration --strict`.

### Verification evidence (2026-08-09)

- `cargo test --workspace`: 70/70 passed (graphify-core 17, graphify-memory 15,
  graphify-llm 13 + 1 ignored homelab integration, graphify-mcp 20,
  graphify-cli 4, doc-tests 1).
- `cargo clippy --workspace --all-targets -- -D warnings`: zero warnings.
- `cargo fmt --all`: clean.
- `openspec validate graphify-memory-crate-migration --strict`: valid.
- Regression: memory query (`MemorySearcher`/`graphify_memory_query`),
  plugin-domain memory (JSONL namespaces), `.toon plugin_data` tests all pass
  unchanged after the crate move.
