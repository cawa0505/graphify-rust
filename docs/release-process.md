# Graphify Release Process

> Version strategy & mandatory release checklist for Graphify v2.0-alpha and
> subsequent releases. Governed by the `release-v2-alpha` OpenSpec change
> (`openspec/changes/release-v2-alpha/`), spec `release-versioning`.

## Version Strategy

- All workspace crates carry the **same** release version in a single release:
  `graphify-core`, `graphify-llm`, `graphify-memory`, `graphify-mcp`,
  `graphify-registry`, `graphify-cli`.
- Version scheme: semver with pre-release markers. The v2.0-alpha milestone is
  `2.0.0-alpha.1`; beta follows as `2.0.0-beta.x`, GA as `2.0.0`.
- CLI and MCP server report their version via `env!("CARGO_PKG_VERSION")`
  (compile-time from Cargo.toml), so `graphify --version` and the MCP
  `initialize` `serverInfo.version` always match the crate version automatically.
- Release tags follow `v<semver>` (e.g. `v2.0.0-alpha.1`).
- A release commit bumps every crate + `Cargo.lock` in one change; the CLI/MCP
  version strings derive from the crates and need no manual edit.

## Mandatory Release Checklist

A release MUST NOT be tagged unless **all** of the following pass:

1. **Zero-warning build**: `cargo build --all-targets` — no compiler warnings.
2. **Clippy**: `cargo clippy --all-targets --all-features` — zero warnings
   (includes `clippy::all`, `clippy::pedantic`, nursery).
3. **Tests**: `cargo test` — entire workspace passes.
4. **Smoke test**: `graphify index <real-path> -f` succeeds against a real
   fixture (nodes indexed into Qdrant, snapshot saved).
5. **Privacy audit** (project rules #2448/#3091): no local IPs
   (`192.168.*` / RFC-1918), private hostnames, or personal keys in default
   configs or docs. Docs/ref are verbatim snapshots — sanitize by redaction
   (`<redacted:...>`), not rewrite.

## Release Steps

1. Apply the version bump (crates + hardcoded strings) and regenerate
   `Cargo.lock` via `cargo build`.
2. Run the checklist above; fix any failure and re-verify.
3. Document the release (this file stays current; docs/ref get a one-line note
   per rule #3127 when referenced version milestones shift).
4. Commit the release change.
5. On maintainer approval: `git tag v<semver>` and push tag + branch.

### Beta Release Flow

The v2.0.0-beta.x release follows the same process as alpha, with these
additional steps:

1. **Version bump**: `2.0.0-beta.1` (incrementing `.x` for each beta).
2. **Checklist addition**: In addition to the mandatory checklist, verify
   `graphify plugin {probe,reset,list}` outputs correct status for all bound
   plugins (health probe integration test, P3).
3. **Tag**: `git tag v2.0.0-beta.1` (matching the crate version).
4. **Pre-release note**: Add a line to the History section documenting the
   beta release date and version.

## History

- 2026-08-17: v2.0.0-beta.x — beta release process documented
  (post-P3 health probe, P4 TUI workspace/monitor, P5 E2E tests).
- 2026-08-10: v2.0.0-alpha.1 — first release under this process
  (RFC-0004 Target Release: Graphify v2.0-alpha).
