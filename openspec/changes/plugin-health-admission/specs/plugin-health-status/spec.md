## Purpose

Define the health-state model for plugins registered in the SQLite Global Registry, replacing the current two-state (`Ready`/`Unavailable`) model with the four-state model required by SPEC-2026-v2beta §2.2/§2.3: `Healthy`, `Degraded`, `Unavailable`, and `Quarantined`. This state machine is the single source of truth that the CLI, TUI panel, and circuit breaker all read and write.

## ADDED Requirements

### Requirement: Four-state plugin status enum

The registry MUST model plugin status with exactly four states:

- `Healthy` — plugin fully operational and included in the execution pipeline.
- `Degraded` — core functional but partial resources missing (e.g., a required model is offline); execution continues in degraded mode.
- `Unavailable` — service interruption; the run bypasses the plugin.
- `Quarantined` — suspended after repeated failures (see plugin-circuit-breaker); all calls are blocked until a manual reset.

The `status` column CHECK constraint on `plugin_registrations` MUST accept exactly the four new values (`'Healthy'`, `'Degraded'`, `'Unavailable'`, `'Quarantined'`).

#### Scenario: Existing rows migrate to the new vocabulary
- **WHEN** the registry database is migrated from schema version 1 to version 2
- **THEN** every existing `Ready` row becomes `Healthy`, every existing `Unavailable` row stays `Unavailable`, and no existing row is dropped

#### Scenario: Invalid status value is rejected
- **WHEN** a caller attempts to write a status value outside the four-state vocabulary
- **THEN** the write fails and the previous status is preserved

### Requirement: Plugin status persistence and retrieval

The registry MUST expose functions to set and read a plugin's status for a given `(plugin_id, workspace_key)` pair, persisting the value in `plugin_registrations`.

The registry MUST expose a query returning all plugin registrations for a workspace with their current status, ordered deterministically by `plugin_id`, for use by the CLI and TUI.

#### Scenario: Status write is persisted
- **WHEN** a plugin's status is set to `Degraded` for a workspace
- **THEN** a subsequent read for the same `(plugin_id, workspace_key)` returns `Degraded`

#### Scenario: Status read for unregistered plugin returns nothing
- **WHEN** a status is requested for a `(plugin_id, workspace_key)` pair that has no registration row
- **THEN** the read returns no status and does not error

### Requirement: Quarantine state is sticky and manual-reset only

A plugin in `Quarantined` state MUST remain quarantined across processes (persisted in SQLite) and MUST NOT be re-admitted automatically. Only an explicit user action — a dedicated CLI command or the TUI `[F5]` action (see tui-workspace-monitor) — MUST reset it.

#### Scenario: Quarantine survives process restart
- **WHEN** a plugin is quarantined and the graphify process exits and restarts
- **THEN** the plugin is still reported as `Quarantined`

#### Scenario: Reset command clears quarantine and re-probes
- **WHEN** the user invokes the quarantine-reset command for a quarantined plugin
- **THEN** the plugin status is cleared to a probing state, the passive health probe runs, and the status becomes the probe result (`Healthy`, `Degraded`, or `Unavailable`)
