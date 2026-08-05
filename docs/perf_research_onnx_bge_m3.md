# Performance Research: Ollama vs. ONNX (BGE-M3)

## Latency & Layer Overhead

- **Ollama HTTP Layer Cost**: Benchmark data shows that Ollama's HTTP network layer adds significant overhead. For a small model (`all-MiniLM-L6-v2`), direct in-process inference via ONNX (`ort`) takes `~1.28 ms` whereas Ollama HTTP API takes `~11.05 ms` (~8.6× slower). For larger models like `BGE-M3` (568M parameters), the in-process execution remains significantly faster per query compared to the HTTP network boundary.
- **In-Process Inference**: Operating `fastembed-rs` (with CPU int8 quantization) or raw ONNX Runtime (`ort`) runs entirely in-process, bypassing TCP sockets and serialization/deserialization boundaries entirely.

## Model Capability Limits

- **Ollama Limit**: Ollama's `/api/embed` endpoint only yields dense 1024-dimensional vectors. It does not expose BGE-M3's built-in sparse (lexical weight) or ColBERT (token-level multi-vector) outputs.
- **Fastembed/ONNX Capability**: Rust-native `fastembed-rs` using `Bgem3Embedding` computes dense, sparse (indices/weights), and ColBERT vectors simultaneously in a single forward pass, unlocking hybrid multi-stage retrieval.

## Implementation Path & Realized Optimization

The system has been successfully upgraded to use Rust-native `fastembed-rs` with CPU-quantized int8 weights (`BGEM3Q`).

### 1. Directory Hygiene
To prevent contaminating the active workspace directory, the downloads are directed to standard user cache path:
- **XDG path**: `~/.cache/fastembed/`
- **Fallback**: `./.fastembed_cache` (if home directory is missing/unresolvable)
- Directories are automatically verified and created recursively on runtime initialization.

### 2. User Experience Logging
Loading the 544MB `BGEM3Q` ONNX model and initializing the thread pool dynamically takes 2-5 seconds. We have added immediate standard out feedback to keep the interface responsive:
```bash
[graphify] Initializing local ONNX Runtime & loading BGE-M3 (int8)...
# (2-5s hardware execution)
[graphify] ONNX Runtime initialized & BGE-M3 model loaded successfully!
```

### 3. Execution Integration
```rust
// ponytail: Bgem3Embedding initialization with CPU-quantized int8
use fastembed::{Bgem3Embedding, Bgem3InitOptions, Bgem3Model};

let cache_dir = dirs::home_dir().map_or_else(
    || std::path::PathBuf::from(".fastembed_cache"),
    |h| h.join(".cache").join("fastembed"),
);
let _ = std::fs::create_dir_all(&cache_dir);

let mut opts = Bgem3InitOptions::new(Bgem3Model::BGEM3Q)
    .with_max_length(1024)
    .with_cache_dir(cache_dir);

if let Some(concurrency) = self.config.extraction.concurrency {
    opts = opts.with_intra_threads(concurrency);
} else {
    opts = opts.with_intra_threads(4);
}
let model = Bgem3Embedding::try_new(opts)?;
```
