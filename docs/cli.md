# Graphify CLI Manual

`graphify` (formerly `graphify-cli`) is a high-performance terminal command-line tool written in Rust. It provides static AST analysis, topological querying, interactive graphical visualization, and RAG indexing.

---

## Subcommands Overview

The command-line interface exposes 6 subcommands:

### 1. `extract`
Extracts structural AST definitions and dependencies from a codebase statically using tree-sitter, serializing the output into `.toon` or `.json`.
```bash
# Extract current directory using Rayon multi-threading to the default .toon format
graphify extract .

# Explicitly specify output path and JSON format
graphify extract . --output graphify-out/graph.json

# Limit thread concurrency for low-resource environments
graphify extract . --concurrency 4
```

### 2. `query`
Queries structural nodes in the compiled graph using BFS (Breadth-First Search) traversal.
```bash
# Query a specific node using its identifier or label
graphify query "./graphify-llm/src/config.rs:struct:MemoryConfig" --depth 2
```

### 3. `path`
Finds the shortest path between two symbols or nodes in the extracted graph.
```bash
# Find shortest path from source function/struct to target function/struct
graphify path "./graphify-llm/src/pipeline.rs:struct:AutoRotatePipeline" "./graphify-llm/src/config.rs:struct:LLMConfig"
```

### 4. `install-skill`
Installs the Graphify Skill directive and rules globally or locally for various AI assistants (including `OpenCode`, Cline, Cursor, and Roo Code).
```bash
# Interactive setup prompting for target assistants and directory level
graphify install-skill

# Install to global directory paths
graphify install-skill --global
```

### 5. `tui`
Launches the zero-dependency interactive terminal dashboard for full-screen structural visualization.
```bash
# Launch interactive TUI
graphify tui
```
**Controls**:
- `Tab` or `1`/`2`: Switch between `Explorer` and `Visual Graph` tabs.
- `j`/`k` / Up/Down: Navigate node list.
- `/`: Focus search bar.
- `g`: Launch default editor and jump to precise code line number.
- `t`/`T`: Trigger Breadth-First Search (BFS) trace path modal overlay.
- `h`/`j`/`k`/`l` / Arrow Keys: Pan canvas viewport in Visual Graph.
- `+`/`-`: Zoom in/out on the canvas.
- `r`/`R`: Reset canvas camera (pan and zoom).
- Left Mouse Click: Select/switch tabs, click node on canvas to jump-select.
- `Esc` / `q`: Dismiss modal, exit search, or quit TUI.

### 6. `index`
Indexes a codebase or previously compiled `.toon`/`.json` file directly into the local/homelab Qdrant vector store using Ollama embeddings.
```bash
# Parse codebase and index into Qdrant store
graphify index .

# Re-index an existing compiled graph file
graphify index graphify-out/graph.toon

# Delete existing Qdrant collection first to force-recreate
graphify index . --force
```

---

## Global Options & Env Overrides

- `-h, --help`: Displays help information.
- `-V, --version`: Prints version.
- `GRAPHIFY_CONFIG_PATH`: Override default XDG config file path.
