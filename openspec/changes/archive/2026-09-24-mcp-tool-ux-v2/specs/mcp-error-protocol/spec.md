## Purpose

Define a consistent error and empty-result response protocol across all MCP tools, so AI agents can reliably distinguish between "no data", "not configured", and "operation failed".

## ADDED Requirements

### Requirement: Three-state response protocol

Every MCP tool SHALL return one of three response shapes: a successful result, an empty result (operation succeeded but no data matched), or an error result (operation failed).

#### Scenario: Successful result returns data

- **WHEN** a tool invocation succeeds and produces data
- **THEN** the response SHALL include the result data in a structured format

#### Scenario: Empty result returns clear signal

- **WHEN** a tool invocation succeeds but produces no matching data
- **THEN** the response SHALL return a clear empty-result indicator (not an error, not `null` data that could be confused with a misconfiguration)
- **AND** the indicator SHALL include a human-readable message explaining why no data was returned

#### Scenario: Error result returns descriptive message

- **WHEN** a tool invocation fails
- **THEN** the response SHALL return an error with a descriptive message identifying the failure cause
- **AND** the error SHALL clearly distinguish between configuration errors (e.g., "memory not enabled"), connectivity errors (e.g., "Qdrant unreachable"), and processing errors (e.g., "parse failure")

### Requirement: Zero-data vs unconfigured distinction

Tools that depend on optional features (memory, plugins, coverage) SHALL clearly distinguish between "feature is not configured/disabled" and "feature is configured but returned no data".

#### Scenario: Disabled memory returns configured message

- **WHEN** an agent invokes `memory_query` and memory is disabled
- **THEN** the response SHALL explicitly state "memory is not configured/enabled" rather than returning an empty result or generic error