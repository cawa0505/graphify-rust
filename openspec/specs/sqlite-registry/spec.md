## Purpose

Provides the SQLite Global Registry (`graphify.db`) that tracks workspaces, plugin registrations, and handoff snapshots across the Graphify ecosystem, serving as the routing and rehydration authority for workspace-scoped memory.

## ADDED Requirements

### Requirement: Registry database location
The system SHALL persist the global registry at a platform-standard data directory (`~/.local/share/graphify/graphify.db` on Linux) and SHALL create it automatically on first use if missing.

#### Scenario: First-run initialization
- **WHEN** any CLI command or TUI startup requires registry access and no database exists
- **THEN** the system creates `graphify.db` with the full three-table schema (`workspaces`, `plugin_registrations`, `handoff_registry`)

#### Scenario: Corrupt or invalid database
- **WHEN** the database file exists but fails schema validation
- **THEN** the system returns an explicit error and does not silently overwrite or recreate user data

### Requirement: Workspace tracking
The system SHALL record each workspace indexed by Graphify as a row in `workspaces` keyed by `workspace_key` (stable hash of the canonical root path), and SHALL support marking exactly one workspace active.

#### Scenario: Workspace registration
- **WHEN** a workspace is indexed for the first time
- **THEN** a `workspaces` row is upserted with the derived `workspace_key` and the workspace becomes active

#### Scenario: Active workspace switching
- **WHEN** a user switches active workspace via CLI or TUI
- **THEN** exactly one row is marked `is_active` and all others are cleared

### Requirement: Plugin registration with rehydration timestamp
The system SHALL register each plugin per workspace in `plugin_registrations` keyed by (`plugin_id`, `workspace_key`), storing `qdrant_collection_name`, `last_synced_at` (integer epoch, default 0), and `status` (`Ready` or `Unavailable`).

#### Scenario: Plugin registration on first scan
- **WHEN** a plugin is discovered for a workspace and no registration exists
- **THEN** the system inserts a row with `last_synced_at = 0` and `status = 'Unavailable'`

#### Scenario: Rehydration point derivation
- **WHEN** Local-to-Server rehydration runs (RFC-0004 §1.3.1)
- **THEN** the system selects envelope records with `created_at > last_synced_at`, upserts them to the server collection, and updates `last_synced_at` to the current timestamp in the same transaction

#### Scenario: Workspace cascade deletion
- **WHEN** a workspace is removed
- **THEN** its `plugin_registrations` and `handoff_registry` rows are deleted via cascade

### Requirement: Registry status two-state model
The system SHALL store plugin registration status as exactly two values: `Ready` and `Unavailable`. Pending-sync state SHALL NOT be a stored status; it is derived from `last_synced_at` lagging behind the newest envelope `created_at`.

#### Scenario: Provider failure does not create a third state
- **WHEN** the embedding provider is unreachable
- **THEN** `status` is set to `Unavailable` and no `SyncPending` value is stored anywhere in the schema

#### Scenario: Recovery restores readiness
- **WHEN** a passive ping succeeds and the MemorySyncJob completes
- **THEN** `status` is set back to `Ready`
