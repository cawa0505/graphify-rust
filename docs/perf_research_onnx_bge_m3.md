# Performance Research: Ollama vs. ONNX (BGE-M3)

## Latency & Layer Overhead

- **Ollama HTTP Layer Cost**: Benchmark data shows that Ollama's HTTP network layer adds significant overhead. For a small model (`all-MiniLM-L6-v2`), direct in-process inference via ONNX (`ort`) takes `~1.28 ms` whereas Ollama HTTP API takes `~11.05 ms` (~8.6× slower). For larger models like `BGE-M3` (568M parameters), the in-process execution remains significantly faster per query compared to the HTTP network boundary.
- **In-Process Inference**: Operating `fastembed-rs` (with CPU int8 quantization) or raw ONNX Runtime (`ort`) runs entirely in-process, bypassing TCP sockets and serialization/deserialization boundaries entirely.

## Model Capability Limits

- **Ollama Limit**: Ollama's `/api/embed` endpoint only yields dense 1024-dimensional vectors. It does not expose BGE-M3's built-in sparse (lexical weight) or ColBERT (token-level multi-vector) outputs.
- **Fastembed/ONNX Capability**: Rust-native `fastembed-rs` using `Bgem3Embedding` computes dense, sparse (indices/weights), and ColBERT vectors simultaneously in a single forward pass, unlocking hybrid multi-stage retrieval.

## Implementation Path

```rust
// ponytail: Bgem3Embedding initialization with CPU-quantized int8
use fastembed::{Bgem3Embedding, Bgem3InitOptions, Bgem3Model};

let model = Bgem3Embedding::try_new(
    Bgem3InitOptions::new(Bgem3Model::BGEM3Q)
        .with_max_length(1024)
        .with_intra_threads(4),
)?;
```
