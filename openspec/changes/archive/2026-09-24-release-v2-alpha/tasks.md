## 1. Version Bump

- [x] 1.1 Bump `graphify-core/Cargo.toml` version to `2.0.0-alpha.1`
- [x] 1.2 Bump `graphify-llm/Cargo.toml` version to `2.0.0-alpha.1`
- [x] 1.3 Bump `graphify-memory/Cargo.toml` version to `2.0.0-alpha.1`
- [x] 1.4 Bump `graphify-mcp/Cargo.toml` version to `2.0.0-alpha.1`
- [x] 1.5 Bump `graphify-registry/Cargo.toml` version to `2.0.0-alpha.1`
- [x] 1.6 Bump `graphify-cli/Cargo.toml` version to `2.0.0-alpha.1`
- [x] 1.7 Run `cargo build` to regenerate `Cargo.lock` and verify no warnings

## 2. Release Checklist Gates

- [x] 2.1 Zero-warning build: `cargo build --all-targets` completes with no compiler warnings
- [x] 2.2 Clippy: `cargo clippy --all-targets --all-features` passes with zero warnings
- [x] 2.3 Tests: `cargo test` passes for the entire workspace
- [x] 2.4 Smoke test: `graphify index` succeeds against a real fixture
- [x] 2.5 Privacy audit: verify default configs and docs contain no local IPs, private hostnames, or personal keys (rule #2448/#3091)

## 3. Release Documentation

- [x] 3.1 Create `docs/release-process.md` documenting version strategy (crate alignment, `v<semver>` tag naming) and the mandatory checklist
- [x] 3.2 Add one-line notes to `docs/ref/RFC-0004-neuro-symbolic-architecture.md` and `docs/ref/SPEC-2026-v2beta-roadmap.md` referencing the release-process doc (docs/ref verbatim rule #3127)

## 4. Tag and Publish

- [x] 4.1 On user approval, create git tag `v2.0.0-alpha.1` at the release commit
- [x] 4.2 Push tag and branch to remote
