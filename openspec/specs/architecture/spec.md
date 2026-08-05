# GraphifyRust Architecture & Integration Specification

## Purpose

Define the complete system architecture, module boundaries, local SLM strategies, and integration roadmap for the GraphifyRust high-performance rewrite, ensuring 100% backward compatibility with Python GraphifyOpt.

## Requirements

### Requirement: Zero-LLM Deterministic AST Parsing
The parser module (src/parser/) SHALL perform pure static AST extraction using tree-sitter without consuming any LLM tokens.

#### Scenario: Running AST extraction on Rust file
- GIVEN a valid Rust source file `src/main.rs`
- WHEN the static extractor is executed on this file
- THEN it SHALL extract all structs, functions, modules, and use statements in microseconds
- AND the memory consumption for parsing a large repository SHALL remain under 10MB

### Requirement: 100% Python Backward Compatibility
The exported `graph.json` or graphml schema produced by the petgraph-based graph engine SHALL be fully identical to the legacy Python output.

#### Scenario: Exporting extraction result
- GIVEN a petgraph memory representation of a codebase
- WHEN the graph engine serializes the graph to JSON
- THEN the output format SHALL use `nodes` and `edges` arrays
- AND each node SHALL contain `id` (deterministic fully-qualified name), `label`, `file_type`, `kind`, and `source_file`
- AND each edge SHALL contain `source`, `target`, `relation`, and `confidence` (EXTRACTED, INFERRED, AMBIGUOUS)

### Requirement: Auto-Rotate LLM Pipeline with Resilience
The LLM integration module (src/provider/) SHALL support automatic fallback, load balancing, and rate limit resilience using tokio and async-trait.

#### Scenario: Handling 429 Rate Limit
- GIVEN a configured list of providers (Local SLM -> Gemini Flash -> Backup APIs)
- WHEN a call to the primary provider returns a 429 (Too Many Requests) or timeout error
- THEN the rotation pipeline SHALL instantly rotate to the next active provider in priority order
- AND continue processing the semantic extraction without failing the overall indexing task

### Requirement: Local SLM GBNF Grammar Enforcement
The local semantic pipeline SHALL strictly enforce JSON Schema formats using GBNF (GGML/GGUF BNF) grammar constraints on Ollama or llama.cpp.

#### Scenario: Requesting node description from Qwen2.5-Coder
- GIVEN a local 1.5B/7B model processing a node summary task
- WHEN the API request is dispatched with JSON Schema GBNF constraints
- THEN the generated tokens SHALL be strictly bound to valid JSON structures
- AND the validation success rate SHALL be 100%

### Requirement: Parallel AST Extraction with Thread-Limit Control
The static extraction module SHALL support multi-threaded parallel AST parsing of files using Rayon, while allowing the user to configure the maximum thread count via both the configuration file and CLI argument overrides to maintain optimal host performance.

#### Scenario: Restricting AST parsing to 4 cores
- GIVEN a codebase with 100 source files
- WHEN AST extraction is executed with a concurrency limit of 4
- THEN Rayon's thread pool SHALL be configured with exactly 4 active threads
- AND all files SHALL be parsed in parallel using this constrained thread pool.

### Requirement: Petgraph Memory Allocation Optimizations
The graph construction engine SHALL pre-allocate memory for both nodes and edges using `Graph::with_capacity` to prevent multiple heap-reallocation copies and memory fragmentation during large codebase processing.

## [待討論]

- 是否保留 `hyperedges`（超超關係）的支援？（Python 版中用於表示一個文件對多個節點的共同引用）
- 遠端備援 API 是否要設定預設的限額（Quota）以防 Token 溢出費用過高？
- 本地小模型 GBNF 規則檔是否需要根據不同語言的 Parser 分開訂製？
