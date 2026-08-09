# compatibility

## Purpose

Align the GraphifyRust serialization schema to be 100% backward-compatible with the Python `graphify` JSON format to ensure existing tools and viewers can load the output without modifications.

## Requirements

### Requirement: Node Field Compatibility

Every serialized node MUST include the `file_type` field and the `source_file` field.

#### Scenario: Serializing a Node to JSON

- GIVEN a Rust `Node` instance with ID `"rust:main"`, label `"main"`, and source file `"src/main.rs"`
- WHEN the node is serialized to JSON
- THEN the JSON object SHALL contain `"file_type": "code"`
- AND the JSON object SHALL contain `"source_file": "src/main.rs"`
- AND the JSON object SHALL contain `"id"` and `"label"`.

### Requirement: Edge Field Compatibility

Every serialized edge MUST include `relation`, `confidence`, and `source_file` fields.

#### Scenario: Serializing an Edge to JSON

- GIVEN a Rust `Edge` instance from `"rust:a"` to `"rust:b"` with kind `Calls` and source file `"src/main.rs"`
- WHEN the edge is serialized to JSON
- THEN the JSON object SHALL contain `"source": "rust:a"`
- AND the JSON object SHALL contain `"target": "rust:b"`
- AND the JSON object SHALL contain `"relation": "calls"`
- AND the JSON object SHALL contain `"confidence": "EXTRACTED"`
- AND the JSON object SHALL contain `"source_file": "src/main.rs"`.
