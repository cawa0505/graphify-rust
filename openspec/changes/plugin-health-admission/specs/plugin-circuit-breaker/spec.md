## Purpose

Define the execution protections that keep the Graphify Rust Core stable regardless of plugin behavior: a hard execution timeout, a strict envelope schema filter, and an auto-quarantine circuit breaker that suspends repeatedly failing plugins (SPEC-2026-v2beta §2.3). These protections live at the plugin invocation boundary in `graphify-cli` (`PluginHost::broadcast` and hook invocations), so no single plugin can stall or corrupt the pipeline.

## ADDED Requirements

### Requirement: Hard execution timeout

Every plugin hook invocation MUST be subject to a 500 ms hard timeout. If a hook does not return within the timeout, the invocation is aborted, the attempt is recorded as a failure, and the pipeline continues with the next plugin.

The timeout MUST NOT be configurable to zero/disabled (a non-zero fixed ceiling per SPEC-2026-v2beta §2.3).

#### Scenario: Slow plugin hook is aborted
- **WHEN** a plugin hook blocks for more than 500 ms
- **THEN** the invocation is aborted and recorded as a failure, and other plugins still receive the event

#### Scenario: Fast plugin hook completes normally
- **WHEN** a plugin hook returns within 500 ms
- **THEN** the invocation completes normally and is not recorded as a failure

### Requirement: Envelope schema strict validation

Any plugin result payload returned to Core MUST be validated against the `PluginMemoryEnvelope` schema (record_id, workspace_key, plugin_id, payload, created_at). A payload that fails validation MUST be discarded, a warning MUST be logged with the plugin id and reason, and the discarded payload MUST NOT be written to any Qdrant collection.

#### Scenario: Invalid payload is dropped with warning
- **WHEN** a plugin returns an envelope missing a required field (e.g., no `workspace_key`)
- **THEN** the payload is discarded, a warning names the plugin and the missing field, and nothing is persisted

#### Scenario: Valid payload is persisted
- **WHEN** a plugin returns an envelope conforming to the schema
- **THEN** the payload is accepted and written normally

### Requirement: Circuit breaker with auto-quarantine

The system MUST count consecutive hook failures per `(plugin_id, workspace_key)` across invocations within a process and, on the 3rd consecutive failure, transition the plugin status to `Quarantined` (persisted via plugin-health-status).

A failure is: a hook timeout, a hook panic, or a schema-rejected payload. A single successful invocation MUST reset the consecutive-failure counter to zero.

Once quarantined, the plugin MUST be bypassed for all subsequent invocations in the process until a manual reset (plugin-health-status reset).

#### Scenario: Three consecutive failures quarantine the plugin
- **WHEN** a plugin fails 3 times consecutively (any mix of timeout/panic/schema rejection)
- **THEN** the plugin status becomes `Quarantined` and the plugin is bypassed for all later invocations

#### Scenario: Success resets the failure counter
- **WHEN** a plugin fails twice and then succeeds once
- **THEN** the failure counter returns to zero and the plugin remains in its normal state

#### Scenario: Quarantined plugin is bypassed
- **WHEN** an event is broadcast while a plugin is quarantined
- **THEN** the plugin's hook is not invoked and the broadcast completes for the remaining plugins
