## Purpose

Defines the dual-track vector storage mechanism: a managed local Qdrant process (downloaded on first use from official GitHub Releases, verified by SHA-256, spawned with environment overrides) serving as the zero-server default, with automatic upgrade to an external Qdrant server when available and seamless degrade back to local on disconnection. Replaces the RFC-0004 §1.3 `Qdrant::from_path` pseudocode, which does not exist in any qdrant-client release (0.11.1–1.19.0).

## ADDED Requirements

### Requirement: Managed local Qdrant process

When local fallback mode is enabled and no healthy external server is configured, the system SHALL provide a local vector store by managing a Qdrant standalone process as a child process:

- The binary SHALL be downloaded on first use from the official `qdrant/qdrant` GitHub Releases (x86_64-unknown-linux-gnu, or the platform-appropriate asset).
- The download SHALL be verified against the SHA-256 digest published by the GitHub Releases API before the binary is used; a digest mismatch SHALL abort local mode with an explicit error.
- The process SHALL be started with environment overrides (`QDRANT__SERVICE__HTTP_PORT`, `QDRANT__SERVICE__GRPC_PORT`, `QDRANT__STORAGE__STORAGE_PATH`, `QDRANT__TELEMETRY_DISABLED`) rather than a config file, with storage written under the configured local storage directory (XDG data dir by default).
- Readiness SHALL be confirmed by polling the REST `/healthz` endpoint with a bounded timeout before the store reports available.
- The child process SHALL be terminated gracefully when the owning store is dropped or the CLI exits.

#### Scenario: First use downloads and verifies the binary
- **WHEN** local mode is triggered and no binary exists at the configured bin dir
- **THEN** the system downloads the release asset, verifies its SHA-256 digest against the GitHub API value, and spawns it; a digest mismatch fails with an explicit error and never runs an unverified binary

#### Scenario: Local process readiness
- **WHEN** the local process has been spawned
- **THEN** the store polls `/healthz` until ready within a bounded timeout, and only then reports the memory store as available

#### Scenario: Process shutdown
- **WHEN** the store is dropped or the process exits (CLI end)
- **THEN** the child process is terminated gracefully and no orphan process remains

### Requirement: Dual-track initialization with fallback

The memory store SHALL expose `init_with_fallback(server_url, local_config)` that:

- Probes the external server health with a bounded timeout; on success selects `StorageMode::ServerUrl`.
- On probe failure (or when no server is configured), falls back to the managed local process and selects `StorageMode::LocalProcess` (connecting over `Qdrant::from_url` to the local endpoint — the storage client interface is identical in both modes).
- Never fails construction solely due to server unavailability: if local fallback is enabled, construction succeeds in local mode.

#### Scenario: Server healthy selects server mode
- **WHEN** the external server answers the health probe
- **THEN** the store uses `StorageMode::ServerUrl` and all queries/upserts target the external server

#### Scenario: Server down degrades to local
- **WHEN** the external server is unreachable and local fallback is enabled
- **THEN** the store uses `StorageMode::LocalProcess`, all queries/upserts target the local process, and the caller observes a working memory store rather than a hard failure

#### Scenario: Local fallback disabled
- **WHEN** local fallback is disabled and the server is unreachable
- **THEN** the store reports the memory store as `Unavailable` (existing `MemoryStatus::Unavailable` semantics) and does not spawn any process

### Requirement: Storage client compatibility

Both modes SHALL use the same storage client interface (`Qdrant::from_url` + REST operations) already used by `QdrantMemoryStore`; callers SHALL NOT be able to observe which mode is active through the storage API. The active mode MAY be exposed read-only for diagnostics (e.g., TUI status line).

#### Scenario: Mode switch is transparent to callers
- **WHEN** the store degrades from `ServerUrl` to `LocalProcess` (or upgrades back)
- **THEN** existing query/upsert callers keep working unchanged, because both modes present the identical `Qdrant::from_url` client interface; only diagnostics observe the mode
