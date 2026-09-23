## Purpose

Upgrade the existing TUI graph inspector (currently bound to the current working directory's `graphify-out/graph.toon`) into a workspace-aware monitor: a workspace switcher driven by the SQLite Global Registry, a plugin health panel fed by `plugin_registrations`, and a `[F5]` one-key recovery action (SPEC-2026-v2beta §3). The change must preserve the existing inspector's simplicity — no global architectural rewrite of the TUI.

## ADDED Requirements

### Requirement: Workspace selection at TUI startup

The TUI MUST present a workspace selector on startup listing all registered workspaces from the SQLite Global Registry (id, root path, last indexed time). The user MUST be able to pick a workspace, and the TUI MUST load that workspace's `.toon` graph instead of the cwd-bound default.

When no workspace is registered (fresh setup), the TUI MUST fall back to the current behavior (load `graphify-out/graph.toon` from the cwd) and remain usable.

The workspace list MUST load in under 100 ms from a cold start (SQLite registry read, per RFC-0004).

#### Scenario: User picks a registered workspace
- **WHEN** the TUI starts and the user selects a registered workspace from the list
- **THEN** the TUI loads that workspace's `.toon` and the inspector operates on it

#### Scenario: No registered workspace falls back to cwd
- **WHEN** the TUI starts with an empty registry
- **THEN** the TUI loads `graphify-out/graph.toon` from the current directory exactly as today, with no error

#### Scenario: Workspace `.toon` missing shows clear error
- **WHEN** the user selects a workspace whose `.toon` is missing or stale
- **THEN** the TUI shows a clear error naming the workspace and the missing file, and returns to the selector

### Requirement: Plugin health panel

The TUI MUST display a plugin health panel listing every plugin registration for the active workspace with its current status (`Healthy`/`Degraded`/`Unavailable`/`Quarantined`), read live from `plugin_registrations`. The panel MUST NOT interrupt or change the existing inspector interactions (keyboard/mouse navigation of the graph remains exactly as today).

The panel MUST be reachable with a single key press (e.g. `p` for Plugins) and MUST NOT open by default — the default view stays the current graph inspector, preserving the existing minimal UI.

#### Scenario: Plugin panel shows live statuses
- **WHEN** the user presses the plugins key in the TUI on a workspace with registered plugins
- **THEN** the panel lists each plugin with its persisted status and the user can scroll it

#### Scenario: Default view unchanged
- **WHEN** the TUI starts
- **THEN** the graph inspector renders exactly as today, with no plugin panel visible until the user opens it

### Requirement: [F5] one-key quarantine reset

In the plugin health panel, pressing `[F5]` MUST reset all `Quarantined` plugins for the active workspace: it clears the quarantine state (via plugin-health-status reset), re-runs the passive health probe for each reset plugin, and refreshes the panel with the resulting statuses.

#### Scenario: F5 unquarantines and re-probes
- **WHEN** the user presses `[F5]` while a plugin is `Quarantined`
- **THEN** the plugin's quarantine clears, a probe runs, and the panel shows the probe result

#### Scenario: F5 with no quarantined plugins is a no-op
- **WHEN** the user presses `[F5]` and no plugin is `Quarantined`
- **THEN** the panel refreshes and no status changes
