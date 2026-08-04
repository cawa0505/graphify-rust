# GraphifyRust — Development Conventions

## File Size Limits

- **Rust source files**: Max 300 lines per `.rs` file. If a file exceeds this, split by responsibility (e.g., separate `extract.rs` into `extract/mod.rs`, `extract/python.rs`, `extract/go.rs`).
- **No god-modules**: One concern per file. If you're writing a `utils.rs` that grows past 100 lines, it's doing too much — split it.

## Code Style

- Follow `rustfmt` defaults. Run `cargo fmt` before committing.
- `clippy::all` + `clippy::pedantic` must pass. No `#[allow]` without a comment explaining why.
- 絕對禁止存在任何編譯警告（Warnings）。所有 Compiler 與 Clippy 警告必須在提交前完全修正，確保編譯 100% 乾淨。
- Prefer `thiserror` for library errors, `anyhow` for binary/CLI errors.
- No `unwrap()` in library code. Use `?` or explicit `expect("reason")`.

## Architecture

- **Workspace crates**: `graphify-core` (extraction + graph), `graphify-llm` (LLM pipeline), `graphify-mcp` (MCP server).
- **No circular deps**: `graphify-core` has zero LLM/HTTP dependencies. `graphify-llm` depends on `graphify-core`. `graphify-mcp` depends on both.
- **Schema-first**: The `graph.json` output schema is defined in `openspec/specs/extraction-schema/spec.md`. All extractors produce this schema. Changes to the schema go through OpenSpec.

## Documentation (OpenSpec)

- Specs live in `openspec/specs/<name>/spec.md`. Status: `draft` → `in-progress` → `implemented`.
- Unresolved scope items get `[待討論]` markers.
- Changes are proposed via `openspec change propose` and tracked in `openspec/changes/`.
- `implemented` specs include verification evidence (test output, benchmark numbers) at the end.
- **Token 節省規範**：在做報告（Report）或回覆時，不用詳細列出過多的原始程式碼，應專注在與 openspec 規格文件的實踐與實施進度。

## Honesty Rules

- **No mock mode.** If a feature isn't implemented, say "not implemented" — don't return fake data or placeholder results.
- **No stub functions** that return `Ok(())` silently. If you can't implement it yet, return `Err("not implemented: <what>")` or `todo!()` with a clear message.
- **No fake benchmarks.** Performance numbers must come from actual `cargo bench` runs on real data.
- **No hidden complexity.** If a function is doing something non-obvious, add a comment. If the comment would be longer than 5 lines, the function is too complex — split it.

## Testing

- One `#[cfg(test)] mod tests` block per module, or a separate `tests/` file for integration tests.
- Test names describe behavior: `test_extract_python_imports` not `test_1`.
- Use the Python版 `tests/fixtures/` as test data where possible (symlink or copy).
- `cargo test` must pass before any commit.

## Dependencies

- Prefer stdlib over crates. Only add a dependency if it saves >50 lines of non-trivial code.
- No async runtime in `graphify-core`. Keep it synchronous — tree-sitter and petgraph are sync.
- `graphify-llm` and `graphify-mcp` can use `tokio`.

## Git

- One logical change per commit. Commit message format: `<type>: <description>` (e.g., `feat: add Python extractor`, `fix: handle empty imports`).
- No commits with `WIP` or `fixup` in the message to main branch.

## Backward Compatibility

- **Strict Config Compatibility**: Must support/convert `~/.graphify/config.json` (the Python Graphify configuration containing `backend`, `providers`, and `extraction` settings) seamlessly. If `~/.config/graphify/config.toml` (or similar TOML) is used, any existing `config.json` must be automatically converted or loaded as a fallback on startup or installation without breaking the user environment.
- **Strict Schema Compatibility**: All nodes must contain `file_type` and edges must contain `relation` (not `kind`), `source_file`, and `confidence` fields matching the python version.
