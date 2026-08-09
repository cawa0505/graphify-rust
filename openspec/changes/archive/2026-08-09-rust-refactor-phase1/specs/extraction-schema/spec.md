# Delta for Extraction Schema

## ADDED Requirements

### Requirement: Rust Workspace Structure
The system SHALL consist of three crates: `graphify-core` (extraction + graph, sync, zero LLM deps), `graphify-mcp` (MCP server, async), `graphify-cli` (CLI binary).

#### Scenario: Workspace builds
- GIVEN a fresh clone of GraphifyRust
- WHEN `cargo build` is run
- THEN all three crates compile without errors

### Requirement: Tree-sitter Multi-language Extraction
The system SHALL extract code structure from Python, Rust, Go, and JavaScript/TypeScript files using tree-sitter.

#### Scenario: Python extraction
- GIVEN a Python file with functions, classes, and imports
- WHEN `graphify extract <path>` is run
- THEN nodes are produced for each function, class, and module with correct `kind` and `language`

#### Scenario: Rust extraction
- GIVEN a Rust file with structs, traits, impls, and functions
- WHEN `graphify extract <path>` is run
- THEN nodes are produced with correct `kind` values (struct, trait, impl, function)

### Requirement: petgraph Graph Building
The system SHALL build a petgraph graph from extraction results and output graph.json.

#### Scenario: Graph construction
- GIVEN extraction output with nodes and edges
- WHEN `graphify extract <path>` completes
- THEN `graphify-out/graph.json` contains valid JSON matching the extraction-schema spec

### Requirement: MCP Server Skeleton
The system SHALL provide an MCP server with `graphify_query` (BFS traversal) and `graphify_path` (shortest path) tools.

#### Scenario: MCP query
- GIVEN a built graph
- WHEN `graphify-mcp` is started and `graphify_query` is called
- THEN a BFS traversal result is returned

### Requirement: CLI Interface
The system SHALL provide a CLI with `extract` and `query` subcommands.

#### Scenario: Extract command
- GIVEN a directory with code files
- WHEN `graphify extract <path>` is run
- THEN `graphify-out/graph.json` is created

## MODIFIED Requirements

(none — this is a new project)

## REMOVED Requirements

(none — this is a new project)
