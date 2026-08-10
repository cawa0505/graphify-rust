## Purpose

Defines the Graphify v2.0-alpha release versioning strategy: workspace crate version bump, git tag naming, and the mandatory release checklist, serving as the template for future beta/GA releases.

## ADDED Requirements

### Requirement: Workspace crate version alignment
All workspace crates (`graphify-core`, `graphify-llm`, `graphify-memory`, `graphify-mcp`, `graphify-registry`, `graphify-cli`) MUST carry the same release version within a single release. The v2.0-alpha release MUST use version `2.0.0-alpha.1` across all crates, with `Cargo.lock` synchronized.

#### Scenario: All crates bumped for v2.0-alpha
- **WHEN** the release-v2-alpha change is implemented
- **THEN** every workspace crate's `Cargo.toml` declares `version = "2.0.0-alpha.1"` and `Cargo.lock` reflects the aligned versions

#### Scenario: Future releases keep alignment
- **WHEN** a subsequent release (beta/GA) is prepared
- **THEN** all workspace crates MUST be bumped to the same new version in a single commit

### Requirement: Git tag naming convention
Release tags MUST follow the pattern `v<semver>` matching the crate versions, e.g. `v2.0.0-alpha.1`. The tag MUST be created only after the release checklist passes.

#### Scenario: Tag created for v2.0-alpha
- **WHEN** the release checklist has passed for version `2.0.0-alpha.1`
- **THEN** a git tag `v2.0.0-alpha.1` exists pointing at the release commit

#### Scenario: Tag rejected before checklist passes
- **WHEN** the release checklist has not passed
- **THEN** no release tag is created

### Requirement: Mandatory release checklist
A release MUST NOT be tagged unless all of the following hold: the workspace builds with zero compiler warnings, `cargo clippy --all-targets --all-features` passes with zero warnings, `cargo test` passes for the entire workspace, a `graphify index` smoke test succeeds against a real fixture, and a privacy audit (per project rule #2448/#3091) confirms no local IPs, private hostnames, or personal keys remain in default configs or documentation.

#### Scenario: Checklist passes
- **WHEN** build, clippy, tests, smoke test, and privacy audit all succeed
- **THEN** the release may proceed to tagging

#### Scenario: Checklist failure blocks release
- **WHEN** any checklist item fails (e.g. clippy warning or failing test)
- **THEN** the release is blocked until the failure is resolved and re-verified

### Requirement: Release documentation
The v2.0-alpha release process MUST be documented such that future maintainers can reproduce it, including the version strategy (crate alignment, tag naming) and the checklist items, consistent with the v2.0-alpha definition in the RFC-0004 and SPEC-2026-v2beta-roadmap reference documents.

#### Scenario: Documentation exists at release time
- **WHEN** the v2.0-alpha release is tagged
- **THEN** a release-process document describing versioning and checklist is present in the repository

#### Scenario: Documentation stays consistent with references
- **WHEN** the release-versioning documentation is updated
- **THEN** it remains consistent with `docs/ref/RFC-0004-neuro-symbolic-architecture.md` and `docs/ref/SPEC-2026-v2beta-roadmap.md` (verbatim snapshots, updated via one-line notes per docs/ref rule)
