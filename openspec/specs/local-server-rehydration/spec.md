## Purpose

Defines the one-way Local-to-Server Delta Rehydration (RFC-0004 §1.3.1): when the memory engine reconnects to an external Qdrant server after having run in local mode, pending plugin-memory envelope points written while offline are pushed to the server idempotently, the SQLite `last_synced_at` checkpoint advances, local deltas are drained/marked, and the store switches to `StorageMode::ServerUrl`. This is the concrete body of the `SyncJob` trait defined in P2 (sqlite-global-registry), which deliberately deferred the implementation to this change.

## ADDED Requirements

### Requirement: One-off rehydration event on server recovery

When the memory engine detects that the external Qdrant server is healthy (via `init_with_fallback` at a CLI command or TUI startup boundary, per the no-daemon policy), it SHALL execute a one-off blocking rehydration event if local mode held pending deltas:

- The event SHALL NOT run as a background daemon or periodic timer; it runs at the startup boundary that detected recovery.
- After a successful event, the store SHALL switch to `StorageMode::ServerUrl`; no dual-write persists.

#### Scenario: Server returns with pending local deltas
- **WHEN** the external server is healthy at startup and local mode has unsynced deltas
- **THEN** the system runs the rehydration event once, then operates in `ServerUrl` mode

#### Scenario: Server returns with nothing pending
- **WHEN** the external server is healthy at startup and no local deltas exist
- **THEN** the system switches to `ServerUrl` mode without a migration pass

### Requirement: Scan and push pending envelope points

The rehydration event SHALL identify pending `PluginMemoryEnvelope` points as those whose `created_at` exceeds the registration's `last_synced_at` checkpoint (per workspace/plugin, read from the SQLite registry `plugin_registrations.last_synced_at`), and SHALL batch-push them to the external server.

#### Scenario: Pending points are scoped to the checkpoint
- **WHEN** local points exist with `created_at > last_synced_at` for a registration
- **THEN** only those points are pushed; already-synced points are skipped

#### Scenario: Batch push targets the plugin collection
- **WHEN** pending points are pushed
- **THEN** they land in the server's corresponding `graphify_plugin_<id>` collection for the registration

### Requirement: Idempotent upsert by record_id

The push SHALL use the `record_id` (deterministic hash) as the point identity so a re-run of the event naturally overwrites rather than duplicates. A failed event SHALL preserve all pending records and leave `last_synced_at` unchanged.

#### Scenario: Re-run after partial failure
- **WHEN** an event fails partway and a later startup retries
- **THEN** the full pending backlog is re-scanned and re-pushed; overlapping points are overwritten idempotently, and no record is lost or duplicated

#### Scenario: Failure preserves data
- **WHEN** the event fails partway
- **THEN** no local records are deleted, `last_synced_at` is not advanced, and the registry status stays `Unavailable`

### Requirement: Atomic checkpoint advance and local drain

On success, the event SHALL advance `plugin_registrations.last_synced_at` to the latest `created_at` synced, and SHALL mark or clear the local delta so the next startup sees nothing pending. The registry update SHALL be atomic (single SQLite transaction per registration).

#### Scenario: Successful event updates checkpoint
- **WHEN** the event completes successfully
- **THEN** `last_synced_at` advances to the newest synced `created_at`, local deltas are drained/marked, and the registration flips to `Ready` (per P2 status transitions)
