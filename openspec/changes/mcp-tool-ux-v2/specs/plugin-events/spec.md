## MODIFIED Requirements

### Requirement: Update broadcast after graph rebuild

**FROM:** The system MUST broadcast a graph update event to all bound plugins after a successful index or extract run completes.

**TO:** The system MUST broadcast a graph update event to all bound plugins automatically after every successful index, extract, or reindex run completes. No manual trigger SHALL be required for the broadcast to reach plugins in these normal workflows.

#### Scenario: All bound plugins notified after index (unchanged)
- **WHEN** an index run completes successfully and multiple plugins are bound
- **THEN** every bound plugin receives the graph update event

#### Scenario: Reindex also triggers broadcast
- **WHEN** a reindex (single file) completes successfully
- **THEN** every bound plugin receives the graph update event with trigger kind `manual`

#### Scenario: No manual notify needed after index
- **WHEN** an index run completes successfully
- **THEN** the broadcast SHALL fire automatically
- **AND** no explicit `notify_plugins` call SHALL be required

### Requirement: Manual hook trigger command

**FROM:** The system MUST provide a CLI command that manually triggers the plugin hooks with a `manual` event.

**TO:** The system MUST retain the manual hook trigger command as a manual override for cases where automatic broadcast is insufficient (e.g., after external graph file modifications).

#### Scenario: Manual trigger still works
- **WHEN** a user runs the manual hook trigger command while plugins are bound
- **THEN** each bound plugin receives a `manual` graph update event