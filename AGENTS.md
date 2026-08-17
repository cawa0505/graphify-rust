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

## TUI 設計約束 (Global)

現有 TUI（`graphify-cli/src/tui.rs`，ratatui 0.28）是**簡潔、單一職責**的圖譜 Inspector。任何 TUI 變更必須遵守以下硬性約束：

- **預設畫面不可變複雜**：TUI 啟動時的預設畫面必須維持現有 graph inspector（雙欄拓撲 + Inspector）。新面板（workspace selector、plugin monitor 等）必須是**選擇性開啟**（單鍵切換），不得預設佔用畫面。
- **既有互動不可破壞**：鍵盤/滑鼠/搜尋/`$EDITOR` 跳轉/BFS modal 等既有交互流程不得因新功能而改變行為或增加按鍵衝突。
- **新增功能走覆蓋層（overlay/modal）**：新資訊面板應重用既有 modal 機制（`tui.rs:136-253`），Esc 可關閉，關閉後回到與今天完全相同的 inspector。
- **一次只做一件事**：不把 workspace 管理、plugin 管理、事件流等多職責塞進單一畫面。每一項獨立成 panel/modal，不互相耦合。
- **不引入常駐背景執行緒**：TUI 內不得有背景 polling 執行緒（健康探針採 CLI/TUI 觸發式，見 plugin-health-admission spec）。

## Documentation (OpenSpec)

- Specs live in `openspec/specs/<name>/spec.md`. Status: `draft` → `in-progress` → `implemented`.
- **剛性規格先行規範（Spec-First Policy）**：
  - 任何新功能開發或變更，**第一步必須先檢索與理解 `openspec/specs/` 內的規格**。
  - 禁止跳過規格直接撰寫代碼！若發現無對應規格或規格需擴充，必須先呼叫 `openspec-propose` 或 `openspec-update` 工具，撰寫變更計畫或 Delta Spec，經用戶授權同意後，方可啟動代碼編輯。
- **規格與測試剛性綁定（Spec-to-Test Binding）**：
  - 規格書中定義的所有臨界常數、核心行為、異常容災上限（如：金鑰輪轉模除邏輯、Ollama 備用 retry 限制、XDG 轉換行為、`.toon` 欄位排列順序等），**必須以 `assert!` 或具體測試案例，在 Rust 單元測試中剛性存在**。
  - 代碼一旦背離 OpenSpec 規格，`cargo test` 必須報錯失敗，使規格與代碼達到 100% 的物理一致性。
- **未定範疇防禦（Unresolved Scope Protection）**：
  - 任何尚不明確的第三方依賴、API 結構、或待敲定行為，在規格中必須強制標註 `[待討論]`，禁止 AI 自行腦補或猜測實作，以此作為與用戶進行架構防禦與共識收斂的動態探針。
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

## GraphifyPlugins 外部依賴

GraphifyRust 透過 `path = "../GraphifyPlugins/graphify-plugin-xxx"` 引用外掛 crate，這些外掛集中在獨立的 [graphify-plugins](https://github.com/cawa0505/graphify-plugins.git) repo。

### 目錄結構

- `../GraphifyPlugins/` — 單一 git repo（subtree merge，非 submodule）
  - `graphify-plugin-handoff/`
  - `graphify-plugin-opendoc/`
  - `graphify-plugin-review/`
  - `graphify-plugin-telemetry/`
  - `graphify-plugin-test-coverage/`
  - `Cargo.toml` — workspace 定義
  - `README.md` — Plugin 生態系總覽

### 開發流程

```bash
# 修改 plugin 後，進該 plugin 目錄 commit/push 到獨立 repo
cd ../GraphifyPlugins/graphify-plugin-review
git add -A && git commit -m "fix: ..."
git push  # 推到獨立 repo (cawa0505/graphify-plugin-review)

# 同步到 graphify-plugins 總 repo
cd ..
git add graphify-plugin-review
git commit -m "sync: graphify-plugin-review <commit message>"
git push  # 推到 cawa0505/graphify-plugins

# 若需從獨立 repo 拉新 commit 進總 repo
git subtree pull --prefix=graphify-plugin-review https://github.com/cawa0505/graphify-plugin-review.git main

# 在 GraphifyRust 中 build（會自動用 path dep 編譯本地 plugin）
cd ../GraphifyRust
cargo build
```

### 注意事項

- 各 plugin 有各自的 GitHub repo，可獨立發 PR、跑 CI。
- `graphify-plugins` 總 repo 用 subtree merge 保留所有歷史，clone 一次即獲得全部 plugin。
- 修改 plugin 後記得先在 GraphifyRust 跑 `cargo build` 和 `cargo test` 確認無迴歸。

## Backward Compatibility

- **Strict Config Compatibility**: Must support/convert `~/.graphify/config.json` (the Python Graphify configuration containing `backend`, `providers`, and `extraction` settings) seamlessly. If `~/.config/graphify/config.toml` (or similar TOML) is used, any existing `config.json` must be automatically converted or loaded as a fallback on startup or installation without breaking the user environment.
- **Strict Schema Compatibility**: All nodes must contain `file_type` and edges must contain `relation` (not `kind`), `source_file`, and `confidence` fields matching the python version.
