# Proposed File Support & Extension Roadmap

This document captures the future roadmap for extended file and parser support to match the Python Graphify capabilities.

## Future Scope (To be Discussed / 待討論)

### Code (36 tree-sitter grammars)
- **Extensions**: `.py`, `.ts`, `.mts`, `.cts`, `.js`, `.jsx`, `.tsx`, `.mjs`, `.go`, `.rs`, `.java`, `.c`, `.cpp`, `.cc`, `.cxx`, `.h`, `.hpp`, `.cu`, `.cuh`, `.metal`, `.rb`, `.cs`, `.kt`, `.kts`, `.scala`, `.php`, `.swift`, `.lua`, `.luau`, `.toc`, `.zig`, `.ps1`, `.psm1`, `.psd1`, `.ex`, `.exs`, `.m`, `.mm`, `.jl`, `.vue`, `.svelte`, `.astro`, `.groovy`, `.gradle`, `.dart`, `.v`, `.sv`, `.svh`, `.sql`, `.f`, `.f90`, `.f95`, `.f03`, `.f08`, `.pas`, `.pp`, `.dpr`, `.dpk`, `.lpr`, `.inc`, `.dfm`, `.lfm`, `.lpk`, `.sh`, `.bash`, `.json`, `.dm`, `.dme`, `.dmi`, `.dmm`, `.dmf`, `.sln`, `.slnx`, `.csproj`, `.fsproj`, `.vbproj`, `.xaml`, `.razor`, `.cshtml`
- *Notes*: `.mts/.cts` reuse the TypeScript grammar. `.cc/.cxx` and CUDA `.cu/.cuh` and Metal `.metal` reuse the C++ grammar.

### Salesforce Apex
- **Extensions**: `.cls`, `.trigger` (regex-based; classes, interfaces, enums, methods, triggers, SOQL/DML edges).

### Terraform / HCL
- **Extensions**: `.tf`, `.tfvars`, `.hcl`.

### MCP Configs
- **Files**: `.mcp.json`, `mcp.json`, `mcp_servers.json`, `claude_desktop_config.json` — extracts server nodes, package refs, env var requirements.

### Package Manifests
- **Files**: `apm.yml`, `pyproject.toml`, `go.mod`, `pom.xml` — one canonical package node per package (by name) plus `depends_on` edges, so a package referenced from many manifests is a single hub.

### Docs
- **Extensions**: `.md`, `.mdx`, `.qmd`, `.html`, `.txt`, `.rst`, `.yaml`, `.yml` (markdown `[text](./other.md)` links and `[[wikilinks]]` become reference edges between docs).

### Office
- **Extensions**: `.docx`, `.xlsx`.

### Google Workspace
- **Extensions**: `.gdoc`, `.gsheet`, `.gslides` (opt-in).

### PDFs & Media
- **PDFs**: `.pdf`
- **Images**: `.png`, `.jpg`, `.webp`, `.gif`
- **Video / Audio**: `.mp4`, `.mov`, `.mp3`, `.wav` and more
- **YouTube / URLs**: any video URL
