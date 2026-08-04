# Extraction Schema

## Purpose

Define the graph.json output schema that all extractors (tree-sitter AST, LLM semantic analysis) MUST produce. This schema is the compatibility guarantee between GraphifyRust and Python GraphifyOpt.

## Requirements

### Requirement: Node Structure
Each node SHALL contain: `id`, `label`, `kind`, `language`, `source_file`, `start_line`, `end_line`. Optional fields: `doc_comment`, `metadata`.

#### Scenario: Python function extraction
- GIVEN a Python file with a function `def hello():`
- WHEN the extractor processes this file
- THEN a node is produced with `kind: "function"`, `language: "python"`, and `id` in format `python:<module>::hello`

#### Scenario: Rust struct extraction
- GIVEN a Rust file with `pub struct Config { ... }`
- WHEN the extractor processes this file
- THEN a node is produced with `kind: "struct"`, `language: "rust"`, and `id` in format `rust:<crate>::Config`

### Requirement: Edge Structure
Each edge SHALL contain: `source` (node id), `target` (node id), `kind`, `confidence`, `source_location`.

#### Scenario: Import extraction
- GIVEN a Python file with `import os`
- WHEN the extractor processes this file
- THEN an edge is produced with `kind: "imports"`, `confidence: "EXTRACTED"`, and `source_location` pointing to the import line

### Requirement: Deterministic IDs
Node IDs MUST be deterministic: the same source file + same fully-qualified name SHALL always produce the same ID, regardless of run order or environment.

#### Scenario: Same file, two runs
- GIVEN a Python file processed twice
- WHEN both extractions complete
- THEN the node IDs from both runs SHALL be identical

### Requirement: Confidence Levels
Each edge MUST be tagged with one of: `EXTRACTED` (AST-deterministic), `INFERRED` (LLM or heuristic), `AMBIGUOUS` (uncertain).

#### Scenario: AST extraction confidence
- GIVEN a function call detected via tree-sitter
- WHEN the edge is created
- THEN its confidence SHALL be `EXTRACTED`

### Requirement: Source Location
Each edge MUST include `source_location` in format `file_path:line_number` for traceability.

### Requirement: Graph Output Format
The output JSON SHALL contain: `nodes` (array), `edges` (array), `metadata` (version, timestamps, counts, token usage).

#### Scenario: Empty extraction
- GIVEN a directory with no code files
- WHEN extraction completes
- THEN the output contains empty `nodes` and `edges` arrays with `metadata.total_nodes: 0`

### Requirement: Forward Compatibility
Adding new node kinds or edge kinds MUST NOT break existing consumers. Consumers SHALL ignore unknown kinds.

#### Scenario: Unknown kind encountered
- GIVEN a graph.json with a node having `kind: "new_concept"`
- WHEN a consumer reads this graph
- THEN the consumer processes it without error, treating the unknown kind as opaque

## [待討論]

- `kind` enum 完整清單？（struct, enum, trait, impl, function, method, module, file, concept, ...）
- `metadata` 結構是否需要 per-language 定義？
- 是否需要 `hyperedges` 支援？（Python 版有）
