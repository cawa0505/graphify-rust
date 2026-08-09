## Purpose

Expose the LLM provider pipeline in `graphify-llm` as a reusable gateway
contract so native plugins can call a shared, configured LLM service without
reimplementing key rotation, provider failover, or HTTP handling — while
remaining free to bring their own dedicated models.

## ADDED Requirements

### Requirement: CoreLlmProvider gateway trait

`graphify-llm` MUST define a `CoreLlmProvider` trait with a `complete` method
for single-prompt completion and a `chat` method for message-based chat, both
returning the generated text or an `LlmError`.

`AutoRotatePipeline` MUST implement `CoreLlmProvider` as the default gateway
implementation, reusing its existing auto-rotating key selection and provider
failover behavior.

#### Scenario: Plugin calls the shared gateway

- **WHEN** a native plugin holds a `CoreLlmProvider` and invokes `complete`
- **THEN** the call routes through the configured provider pipeline with the
  same rotation and failover guarantees as the CLI pipeline

#### Scenario: Pipeline exposes chat

- **WHEN** a caller invokes `chat` with a message list
- **THEN** the pipeline produces a completion using the same provider and key
  selection logic as `complete`

### Requirement: PluginContext skeleton

`graphify-llm` MUST define a `PluginContext` struct carrying the memory engine,
the default LLM gateway, and the `workspace_key` routing key, matching the
v2.0-alpha supplement's service-context design.

The skeleton MUST NOT include multi-model routing or plugin-specialized model
handling in this change; those are Phase 7 plugin work.

#### Scenario: Context carries the shared services

- **WHEN** a native plugin is bound to a workspace
- **THEN** it receives a `PluginContext` with the shared memory engine, the
  default `CoreLlmProvider`, and the `workspace_key`

### Requirement: Plugin model autonomy preserved

`graphify-llm` MUST NOT force plugins to use the shared gateway: the trait and
context are provided as defaults, and a plugin MAY use its own dedicated model
client instead.

#### Scenario: Plugin uses a dedicated model

- **WHEN** `graphify-plugin-review` is configured with a specialized safety
  model (e.g. Shieldstral-3B)
- **THEN** it uses its own client for that model and falls back to the shared
  gateway otherwise
