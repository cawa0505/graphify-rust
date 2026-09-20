# Graphify CLI Manual

`graphify` (formerly `graphify-cli`) is a high-performance terminal command-line tool written in Rust. It provides static AST analysis, topological querying, interactive graphical visualization, and RAG indexing.

---

## Subcommands Overview

The command-line interface exposes 13 subcommands:

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

#### TUI Demonstration Walkthrough
![Graphify TUI Demo](graphify-tui-demo.gif)

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
- Left Mouse Drag: Grab and pan the 2D visual canvas topology.
- Mouse Scroll Wheel: Zoom in / zoom out of the canvas.
- Right Mouse Click: Click any node to select it and instantly pop open its BFS Trace Modal.
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

### 7. `init`
Initializes a project with graphify infrastructure: creates `.opencode/state.json`, updates `.gitignore`, and builds the initial AST graph.
```bash
# Initialize project at current directory
graphify init

# Initialize at a specific path
graphify init ./my-project
```

### 8. `handoff`
Relay 狀態接力工具（內嵌 graphify-plugin-handoff）。管理跨 session 的開發狀態交接，包含 `save`、`close`、`switch`、`resume`、`status`、`init`、`add` 等子命令。其中 `skill install` 子命令用於安裝 Code Relay skill 到本地 AI 代理生態系統：

```bash
# 安裝 Code Relay skill 到所有偵測到的 agent
graphify handoff skill install

# 只安裝到 opencode，使用者範圍
graphify handoff skill install --agent opencode --scope user

# 查看接力狀態
graphify handoff status
```

### 9. `plugin`
Manage bound plugins and trigger graph-update events (`indexed`, `extracted`, `manual`).
```bash
# List bound plugins
graphify plugin list

# Trigger graph-update event
graphify plugin notify --kind indexed
```

### 10. `workspace`
Manage graphify workspaces: list, switch, and check status.
```bash
graphify workspace list
graphify workspace switch <name>
graphify workspace status
```

### 11. `opendoc`
文件↔程式碼追蹤與 drift 偵測（內嵌 graphify-plugin-opendoc）。索引 markdown 文件中的 spec↔symbol 連結，並偵測文件與程式碼是否漂移。
```bash
# Index all spec docs in the workspace
graphify opendoc index

# Audit drift between docs and code
graphify opendoc audit
```

### 12. `review`
code-review-graph 橋接（內嵌 graphify-plugin-review）。將 CRG 的變更風險分析結果匯入 graphify 的 review bindings。
```bash
# Search CRG for changed functions and bind as review points
graphify review search --base HEAD~1
```

### 13. `coverage`
測試覆蓋率橋接（內嵌 graphify-plugin-test-coverage）。匯入 LCOV 或 cobertura 格式的覆蓋率資料，並綁定到 AST 符號。
```bash
# Import LCOV coverage data
graphify coverage ingest --format lcov --data coverage.lcov
```

### 14. `compose`
以 Assembly Manifest（YAML）組裝多個 workspace 成統一圖，並投影為 ASCII/SVG 架構圖（經 npx box-of-rain）。節點引用語法 `ws-id::node_id`。
```bash
# 驗證 manifest：workspace 路徑、.toon 存在、節點引用全部檢查，錯誤一次列完
graphify compose validate ./ecommerce.yaml

# 建立統一圖並輸出 JSON 摘要（workspaces / total_nodes / total_edges / cross_edges）
graphify compose graph ./ecommerce.yaml

# 渲染 ASCII 架構圖（加 --svg 輸出 SVG）
graphify compose render ./ecommerce.yaml
```
manifest 範例（workspace 路徑相對於 manifest 檔所在目錄）：
```yaml
workspaces:
  - id: agentshop
    path: ./AgentShop
  - id: nexusledger
    path: ./NexusLedger
relations:
  - from: agentshop
    type: uses
    to: nexusledger
  - from: agentshop::src/gateway/mod.rs
    type: calls_into
    to: nexusledger::src/ledger/mod.rs
```

---

## Global Options & Env Overrides

- `-h, --help`: Displays help information.
- `-V, --version`: Prints version.
- `GRAPHIFY_CONFIG_PATH`: Override default XDG config file path.
