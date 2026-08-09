## Purpose

Defines the lifecycle policy for handoff snapshots in the registry: automatic pruning by TTL and per-workspace capacity to prevent unbounded growth of session records.

## ADDED Requirements

### Requirement: TTL expiration
Handoff snapshots SHALL carry an `expires_at` timestamp defaulting to `created_at + 7 days`. Snapshots past `expires_at` SHALL be deleted automatically.

#### Scenario: Default expiration window
- **WHEN** a snapshot is written without an explicit `expires_at`
- **THEN** `expires_at` is set to `created_at` plus 7 days

#### Scenario: Expired snapshot removed
- **WHEN** pruning runs and a snapshot's `expires_at` is in the past
- **THEN** that snapshot row is deleted from `handoff_registry`

### Requirement: Per-workspace capacity cap
The system SHALL keep at most 20 handoff snapshots per workspace. When a write would exceed the cap, the oldest snapshots SHALL be evicted first (FIFO) to make room.

#### Scenario: Cap exceeded on write
- **WHEN** a new snapshot is inserted and the workspace already holds 20 snapshots
- **THEN** the oldest non-expired snapshot is deleted so the workspace holds at most 20

#### Scenario: Expired-then-cap ordering
- **WHEN** pruning runs and both expired and over-cap candidates exist
- **THEN** expired snapshots are removed first, then FIFO eviction applies to any remaining over-cap surplus

### Requirement: Pruning trigger on snapshot write
The system SHALL run pruning (TTL scan plus capacity check) as a side effect of every new handoff snapshot write, so the registry self-maintains without a background job.

#### Scenario: Write triggers maintenance
- **WHEN** a new snapshot is written
- **THEN** TTL and capacity pruning execute in the same transaction as the insert
