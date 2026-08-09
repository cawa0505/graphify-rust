## Purpose

Defines the passive-triggered synchronization policy for the embedding provider: no background daemon polling, with provider health checked only at CLI command or TUI startup boundaries.

## ADDED Requirements

### Requirement: No daemon polling
The system SHALL NOT run a background polling daemon or periodic timer to monitor embedding provider health. Provider recovery is only detected at the next user-initiated command or TUI startup.

#### Scenario: Provider outage with no user activity
- **WHEN** the embedding provider is down and the user runs no commands
- **THEN** no background process attempts reconnection and no resources are consumed

### Requirement: Startup ping with 10ms timeout
The system SHALL probe the embedding provider with a bounded 10ms ping at CLI command and TUI startup when memory is enabled and status is not `Ready`. If the probe fails, execution SHALL continue with an explicit warning and the registry status stays `Unavailable`.

#### Scenario: Provider still unavailable
- **WHEN** the 10ms ping fails at startup
- **THEN** the system prints a warning, keeps `status = 'Unavailable'`, and continues the topology/indexing work without memory queries

#### Scenario: Provider recovered
- **WHEN** the 10ms ping succeeds at startup
- **THEN** the system triggers the MemorySyncJob, and on completion sets `status = 'Ready'` and resumes memory-backed operations

### Requirement: MemorySyncJob execution scope
The MemorySyncJob SHALL re-synchronize the pending embedding backlog (records written while `Unavailable`) into the recovered provider by:
- Querying SQLite for `handoff_registry` entries where `created_at > last_synced_at` (per workspace/plugin),
- Performing a batch upsert via the Qdrant REST API (using the existing `upsert_rest` path) for those entries,
- On success, updating `last_synced_at` to the latest `created_at` synced.
A failed sync SHALL leave `status = 'Unavailable'` and preserve all pending records.

#### Scenario: Sync failure preserves data
- **WHEN** the MemorySyncJob fails partway through
- **THEN** no records are deleted, `status` remains `Unavailable`, and a subsequent startup retries the full backlog

#### Scenario: Sync completion updates timestamps
- **WHEN** the MemorySyncJob finishes successfully
- **THEN** `last_synced_at` is advanced and the registry reflects the new state atomically
