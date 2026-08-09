# TOON Plugin Data Specification

## Purpose

Reserve a versioned, namespaced `.toon` metadata container so plugins can attach
optional context without changing the meaning or shape of core graph data.

## Requirements

### Requirement: Plugin metadata uses the reserved container

Plugin metadata MUST be stored only inside the reserved `plugin_data` container
under `.toon` metadata. Plugins MUST NOT add arbitrary top-level fields or change
the meaning of core nodes and edges.

#### Scenario: Review adds metadata

- **WHEN** the Review plugin enriches a `.toon` document
- **THEN** its data appears under `metadata.plugin_data.review`

#### Scenario: Core reads a plugin-enriched document

- **WHEN** core reads a `.toon` document containing unknown plugin data
- **THEN** core preserves compatibility by tolerating absent or unknown plugin
  entries

### Requirement: Plugin metadata is namespaced and versioned

Each `plugin_data` entry MUST be keyed by the registered `plugin_id` and MUST
carry plugin-owned version information when its payload has a schema.

#### Scenario: Plugin IDs do not collide

- **WHEN** OpenDoc and Review both enrich the same `.toon` document
- **THEN** their payloads remain in separate plugin-id entries

### Requirement: Plugin enrichment is optional

Core graph validity MUST NOT depend on any plugin being installed or any
`plugin_data` entry being present.

#### Scenario: `.toon` without plugins

- **WHEN** core emits `.toon` with no plugin enrichment
- **THEN** the document remains valid without a `plugin_data` entry
