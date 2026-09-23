## Purpose

Define the execution protections that keep the Graphify Rust Core stable regardless of plugin behavior: a hard execution timeout, a strict envelope schema filter, and an auto-quarantine circuit breaker that suspends repeatedly failing plugins (SPEC-2026-v2beta §2.3). These protections live at the real plugin invocation boundary: `graphify-mcp`'s subprocess host (`PluginHost::call_tool` / `broadcast_graph_updated`, driving `PluginProcess` JSON-RPC subprocesses). The CLI's in-process `PluginHost` (`graphify-cli/src/plugin_host.rs`) never registers plugins in production — the MCP subprocess host is where third-party plugin execution actually happens.

> **Adaptation note**: the roadmap's "Hard Timeout (500 ms)" was specced for the CLI's synchronous in-process hook model. At the subprocess boundary the structural isolation is stronger: tool calls are bounded by an existing non-configurable `recv_timeout` ceiling, and notifications are fire-and-forget (a slow plugin cannot stall the gateway). See Requirement: Hard execution timeout below.

## ADDED Requirements

### Requirement: Hard execution timeout

Every plugin tool invocation at the MCP subprocess boundary MUST be subject to a hard timeout: the existing `PLUGIN_TIMEOUT` ceiling enforced via `recv_timeout` in `PluginProcess::await_response`. A timed-out or transport-failed invocation MUST be recorded as a failure, the process marked `Failed`, and the gateway continues serving other plugins.

The timeout MUST NOT be configurable to zero/disabled (a non-zero fixed ceiling per SPEC-2026-v2beta §2.3).

Notification delivery (`notifications/graph_updated`) is fire-and-forget — a write to the plugin's stdin with no response wait — so a slow plugin can never stall the broadcast path; delivery failure (broken pipe / dead process) is recorded as a failure.

#### Scenario: Slow plugin tool call is aborted
- **WHEN** a plugin tool call exceeds the hard timeout
- **THEN** the invocation is aborted, recorded as a failure, the plugin process marked `Failed`, and other plugins keep working

#### Scenario: Fast plugin tool call completes normally
- **WHEN** a plugin tool call returns within the timeout
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

The MCP host MUST count consecutive invocation failures per plugin id within the gateway process (the gateway is workspace-bound — its workspace key is derived from cwd, same as the existing broadcast path) and, on the 3rd consecutive failure, transition the plugin status to `Quarantined` (persisted via plugin-health-status in the registry).

A failure is: a tool-call timeout, a transport/framing error, a dead process, or a schema-rejected payload. A single successful invocation MUST reset the consecutive-failure counter to zero.

Once quarantined, the plugin MUST be bypassed for all subsequent invocations in the process (tool calls rejected, broadcast skipped) until a manual reset (plugin-health-status reset). On host startup, plugins already `Quarantined` in the registry MUST be bypassed immediately.

#### Scenario: Three consecutive failures quarantine the plugin
- **WHEN** a plugin fails 3 times consecutively (any mix of timeout/transport/schema rejection)
- **THEN** the plugin status becomes `Quarantined` (registry) and the plugin is bypassed for all later invocations

#### Scenario: Success resets the failure counter
- **WHEN** a plugin fails twice and then succeeds once
- **THEN** the failure counter returns to zero and the plugin remains in its normal state

#### Scenario: Quarantined plugin is bypassed
- **WHEN** an event is broadcast (or a tool call arrives) while a plugin is quarantined
- **THEN** the plugin is not invoked and the operation completes for the remaining plugins
