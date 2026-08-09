# plugin-events

## Purpose

Provide first-party plugins with a graph-update notification mechanism: when the graph is (re)built by index/extract, or when a user manually triggers hooks, every bound plugin is notified with the set of modified nodes so it can react to code changes.

## Requirements

### Requirement: Workspace key derivation

The workspace identifier carried by graph update events MUST be the `workspace_key`: a stable, reproducible identity for a workspace derived from the workspace root's canonical path.

The derivation MUST use a deterministic hash of the canonicalized workspace root path (std `DefaultHasher`/SipHash), yielding the same key for the same path across processes and machines, and MUST NOT depend on opendoc-mcp's workspace UUID.

The `workspace_key` is graphify's own workspace identity (per architecture decision A); the opendoc-mcp workspace UUID is a separate, plugin-layer concept mapped by opendoc plugins at bind time.

#### Scenario: Same path yields the same key
- **WHEN** the workspace key is derived twice for the same canonical workspace root path
- **THEN** both derivations yield identical keys

#### Scenario: Opendoc workspace UUID is not used
- **WHEN** a workspace key is derived
- **THEN** it is computed from the workspace root path, not from any opendoc-mcp workspace UUID

### Requirement: Graph update event type

The system MUST define a graph update event carrying the workspace identifier, the list of modified node identifiers, and the trigger kind of the update.

The trigger kind MUST be one of: `indexed` (produced by a completed index run), `extracted` (produced by a completed extract run), or `manual` (produced by an explicit user/script trigger).

#### Scenario: Index completion produces an indexed event
- **WHEN** a `graphify index` run completes successfully on a workspace
- **THEN** the system emits a graph update event whose trigger kind is `indexed` and whose modified node list contains the node identifiers affected by that run

#### Scenario: Extract completion produces an extracted event
- **WHEN** a `graphify extract` run completes successfully on a workspace
- **THEN** the system emits a graph update event whose trigger kind is `extracted`

#### Scenario: Manual trigger produces a manual event
- **WHEN** a user invokes the manual plugin-hooks trigger
- **THEN** the system emits a graph update event whose trigger kind is `manual`

### Requirement: Plugin notification hook

The plugin interface MUST expose a graph-update notification hook that every plugin can implement.

The hook MUST accept the graph update event and MUST NOT break existing plugin implementations when they do not implement it (implementations that predate the hook continue to bind and function unchanged).

#### Scenario: Bound plugin receives update notification
- **WHEN** a graph update event is emitted while a plugin is bound to the workspace
- **THEN** the plugin's graph-update notification hook is invoked with that event

#### Scenario: Plugin without the hook remains compatible
- **WHEN** a plugin that does not implement the graph-update notification hook is bound
- **THEN** the plugin binds successfully and the system emits updates without erroring

### Requirement: Update broadcast after graph rebuild

The system MUST broadcast a graph update event to all bound plugins after a successful index or extract run completes.

#### Scenario: All bound plugins notified after index
- **WHEN** an index run completes successfully and multiple plugins are bound
- **THEN** every bound plugin receives the graph update event

#### Scenario: Failed run emits no event
- **WHEN** an index or extract run fails
- **THEN** the system emits no graph update event for that run

### Requirement: Manual hook trigger command

The system MUST provide a CLI command that manually triggers the plugin hooks with a `manual` event.

The command MUST succeed (exit code 0) when no plugins are bound, emitting no events, and MUST propagate a clear error if hook execution fails.

#### Scenario: Manual trigger with bound plugins
- **WHEN** a user runs the manual hook trigger command while plugins are bound
- **THEN** each bound plugin receives a `manual` graph update event

#### Scenario: Manual trigger with no plugins
- **WHEN** a user runs the manual hook trigger command with no plugins bound
- **THEN** the command exits successfully and no events are emitted

#### Scenario: Manual trigger hook failure
- **WHEN** a bound plugin's hook fails during a manual trigger
- **THEN** the command reports an error identifying the failure
