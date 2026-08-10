# ROCm Embedding Sidecar Research

## Status

- Category: long-term architecture research
- Implementation status: not implemented
- OpenSpec status: no approved change proposal
- Scope: container examples, swappable embedding configuration, and optional compute sidecars

This document records the research direction discussed for expanding Graphify's
embedding compute options. It is not an implementation specification and does
not authorize code changes.

## Current State

Graphify currently supports two embedding paths:

1. `ollama`: sends embedding requests to the configured HTTP endpoint. The
   default model is `bge-m3`, with 1,024-dimensional vectors.
2. `fastembed`: runs embeddings in process through FastEmbed and ONNX Runtime.

Embedding configuration is currently owned by
`graphify-memory/src/config.rs::EmbeddingConfig`:

```toml
[memory.long_term.embedding]
provider = "ollama"
endpoint = "http://localhost:11434"
model = "bge-m3"
vector_size = 1024
```

The configured Qdrant service stores vectors; it does not decide whether CPU,
CUDA, or ROCm performs embedding inference.

## Target Direction

Keep Graphify independent of GPU vendor runtimes. A compute service should
expose a stable embedding interface, while its deployment selects CPU, CUDA,
ROCm, or another accelerator.

```text
Graphify -> embedding endpoint -> CPU / CUDA / ROCm runtime
         -> Qdrant endpoint   -> vector storage
```

This allows different machines to provide compute without linking ROCm into
the Graphify binary or increasing the default image size.

## Container Examples

The proposed examples are:

- A minimal Graphify `Dockerfile` that contains no GPU runtime.
- A CPU embedding service profile.
- A ROCm embedding sidecar profile with AMD GPU devices passed through.
- A Qdrant service shared by either profile.

ROCm containers generally require access to `/dev/kfd` and `/dev/dri` on a
compatible Linux host. Exact permissions and security options depend on the
host ROCm and container runtime versions.

Illustrative Compose shape:

```yaml
services:
  graphify:
    image: graphify
    environment:
      GRAPHIFY_CONFIG_PATH: /config/config.toml
    depends_on:
      qdrant:
        condition: service_healthy

  embedding-cpu:
    profiles: ["cpu"]
    image: example/embedding-cpu

  embedding-rocm:
    profiles: ["rocm"]
    image: example/embedding-rocm
    devices:
      - /dev/kfd
      - /dev/dri

  qdrant:
    image: qdrant/qdrant
```

The image names and service protocol above are placeholders, not committed
dependencies.

## Swappable Configuration Boundary

The existing `provider`, `endpoint`, `model`, and `vector_size` fields already
cover a remote sidecar at the configuration level. The smallest compatible
extension is to define a stable remote embedding protocol rather than create
GPU-specific fields in Graphify.

Potential configuration:

```toml
[memory.long_term.embedding]
provider = "http"
endpoint = "http://embedding-rocm:8000"
model = "bge-m3"
vector_size = 1024
```

The implementation boundary should accept text batches and return ordered
vectors with explicit dimensions and structured errors. Hardware selection
remains a deployment concern.

## Optional Sidecar Functions

Additional compute sidecars may become useful when they provide a concrete
benefit:

- embedding inference and batching;
- reranking search candidates;
- model warm-up and readiness reporting;
- GPU capability and runtime health reporting;
- request queueing or load distribution across compute hosts.

Graphify should not add these functions until a measured workload requires
them. Qdrant remains storage and retrieval infrastructure, not the embedding
compute scheduler.

## Open Questions

- [待討論] Choose the stable sidecar protocol: Ollama-compatible API,
  OpenAI-compatible embeddings API, or a small Graphify-specific contract.
- [待討論] Decide whether multiple endpoints require client-side failover or
  an external load balancer.
- [待討論] Select and pin tested ROCm, host kernel, image, and model-runtime
  versions.
- [待討論] Decide whether the ROCm example uses Ollama, ONNX Runtime, PyTorch,
  or another server.
- [待討論] Define health checks, batching limits, timeouts, and retry behavior.
- [待討論] Establish benchmark thresholds that justify GPU or distributed
  execution over the current Ollama path.

## References

- AMD ROCm container documentation:
  <https://rocm.docs.amd.com/projects/install-on-linux/en/latest/how-to/docker.html>
- AMD ROCm Docker images: <https://hub.docker.com/u/rocm>
- Docker Compose profiles:
  <https://docs.docker.com/compose/how-tos/profiles/>
- Docker Compose startup ordering and health checks:
  <https://docs.docker.com/compose/how-tos/startup-order/>
