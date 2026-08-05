# Graphify LLM & Memory Crate

`graphify-llm` manages multi-provider LLM pipelines, atomic key modulo rotation, local/homelab Ollama embedding queries, and Qdrant memory synchronization.

---

## Auto-Rotate Pipeline

The LLM connector features a thread-safe, lock-free key rotation loop:
- **Lock-Free Key Selection**: Employs `AtomicUsize` modulo operations (`index % keys.len()`) to dynamically rotate API keys without blocking threads.
- **Zero-Sleep 429 Retries**: Immediately advances the rotation index on rate limit (HTTP 429) errors, avoiding latency-inducing wait loops.
- **Adaptive Provider Failover**: Automatically downgrades requests to local fallback models (like Ollama `qwen2.5-coder`) once primary cloud API keys (Gemini, OpenRouter) are exhausted.

---

## Qdrant Long-Term Memory (LTM)

The `QdrantMemoryStore` coordinates semantic indexing and retrieval:
- **Deterministic Point IDs**: Node UUIDs/identifiers are hashed into deterministic u64 IDs, ensuring that re-indexing operations are perfectly idempotent.
- **Automatic Collection Lifecycle**: Collections are auto-created on first use with dimensions (1024 for `bge-m3`) and Cosine distance pulled directly from configuration.
- **Ollama Integration**: Employs `/api/embeddings` sequentially for zero-dependency local homelab setups.
