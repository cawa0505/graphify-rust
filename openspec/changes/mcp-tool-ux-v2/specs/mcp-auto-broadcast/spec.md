## Purpose

Eliminate the manual `notify_plugins` step by automatically broadcasting graph update events after every index, extract, or reindex operation completes successfully.

## ADDED Requirements

### Requirement: Auto-broadcast after index

The server SHALL automatically emit a `graph_updated` event with trigger kind `indexed` after every successful `graphify index` run.

#### Scenario: Index completes without manual notify

- **WHEN** a `graphify index` run completes successfully
- **THEN** the system SHALL broadcast the `graph_updated` event to all bound plugins
- **AND** no manual `notify_plugins` call SHALL be required for the event to reach plugins

### Requirement: Auto-broadcast after extract

The server SHALL automatically emit a `graph_updated` event with trigger kind `extracted` after every successful `graphify extract` run.

#### Scenario: Extract completes without manual notify

- **WHEN** a `graphify extract` run completes successfully
- **THEN** the system SHALL broadcast the `graph_updated` event to all bound plugins
- **AND** no manual `notify_plugins` call SHALL be required for the event to reach plugins

### Requirement: Auto-broadcast after reindex

The server SHALL automatically emit a `graph_updated` event with trigger kind `manual` after every successful `graph_reindex` MCP tool invocation.

#### Scenario: Reindex triggers broadcast

- **WHEN** an agent invokes the `graph_reindex` tool and it succeeds
- **THEN** the system SHALL broadcast the `graph_updated` event to all bound plugins automatically

### Requirement: Manual notify retained for backward compatibility

The `graphify notify_plugins` tool/command SHALL remain available as a manual override, but its necessity SHALL be eliminated for normal workflows.

#### Scenario: Manual notify still works

- **WHEN** a user explicitly invokes `notify_plugins`
- **THEN** the system SHALL broadcast a `graph_updated` event with trigger kind `manual` as before