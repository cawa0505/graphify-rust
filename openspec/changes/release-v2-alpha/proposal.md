## Why

RFC-0004 defines the target release as Graphify v2.0-alpha（Core Base：Petgraph 16ms AST + SQLite Global Registry + Qdrant Dual-Track Engine），但所有 workspace crate 目前仍停留在 `0.1.0`，且沒有文件化的 release 流程。Plugin 整合與 TUI 重寫陸續落地後，需要正式把版本 bump 與 release 檢查列管，才能標示 v2.0-alpha 里程碑。

## What Changes

- 將全部 workspace crate（`graphify-core`, `graphify-llm`, `graphify-memory`, `graphify-mcp`, `graphify-registry`, `graphify-cli`）版本統一 bump 至 `2.0.0-alpha.1`。
- 同步更新 workspace 根 `Cargo.toml` 的 workspace 版本（若有）與 `Cargo.lock`。
- 建立 release 檢查清單：乾淨 build（零警告）、clippy 全過、`cargo test` 全過、`graphify index` smoke test、隱私審計（#2448/#3091）後才可標 tag。
- 建立 git tag `v2.0.0-alpha.1` 命名慣例與釋出流程文件（對應 SPEC-2026-v2beta-roadmap 的 v2.0-alpha 定義）。

## Capabilities

### New Capabilities
- `release-versioning`: 定義 Graphify v2.0-alpha 的版本策略（crate 版本、tag 命名、release 檢查清單），作為未來 beta/GA 版本釋出的模板。

### Modified Capabilities
<!-- 無現有 spec 的 REQUIREMENTS 改變；此 change 為釋出流程與版本化，屬新增能力。 -->

## Impact

- 所有 workspace crate 的 `Cargo.toml`（6 個 crate）與根 workspace 定義。
- `Cargo.lock` 同步更新。
- 潛在 **BREAKING**：crate 版本由 `0.1.0` 跳到 `2.0.0-alpha.1` 屬於語意化版本的大版本跳躍，任何依賴 `graphify-*` crate 的外部專案需注意 semver 邊界。
- 不改變任何運行時行為；純版本化與釋出流程 change。
- 影響的既有文件：`docs/ref/RFC-0004-neuro-symbolic-architecture.md`（Target Release: Graphify v2.0-alpha）、`docs/ref/SPEC-2026-v2beta-roadmap.md`（v2.0-alpha prerequisites）。
