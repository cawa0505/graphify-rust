## Context

See proposal.md - Why. All six workspace crates (`graphify-core`, `graphify-llm`, `graphify-memory`, `graphify-mcp`, `graphify-registry`, `graphify-cli`) currently declare `version = "0.1.0"`. RFC-0004 targets Graphify v2.0-alpha as the release milestone; no release process or tag convention is documented today. Workspace root `Cargo.toml` uses per-crate version fields (no `workspace.package.version` shared field observed), so alignment must be applied explicitly to each crate.

## Goals / Non-Goals

**Goals:**
- Align all crate versions to `2.0.0-alpha.1` in one commit, with `Cargo.lock` in sync.
- Establish a repeatable release checklist (build/clippy/test/smoke/privacy) as a documented process.
- Define the `v<semver>` tag convention and record it for future beta/GA releases.

**Non-Goals:**
- Not changing any runtime behavior or public APIs in this change.
- Not automating CI release publishing (no `cargo publish` automation) - that belongs to a later infra change.
- Not rewriting the RFC/SPEC reference documents themselves (docs/ref are verbatim snapshots; only one-line notes allowed per rule #3127).

## Decisions

**D1: Version scheme `2.0.0-alpha.1` (semver pre-release) instead of `0.2.0`**
- Rationale: RFC-0004 and SPEC-2026-v2beta-roadmap explicitly frame the milestone as "Graphify v2.0-alpha"; semver pre-release markers (`-alpha.1`) are the standard way to express a pre-1.0-milestone target while staying semver-clean for the eventual `2.0.0` GA. `0.2.0` would misrepresent the roadmap intent.
- Alternative considered: `0.1.1` patch bump - rejected (does not communicate the alpha milestone).
- Alternative considered: `2.0.0` directly - rejected (alpha must not claim GA stability).

**D2: Explicit per-crate version edit rather than introducing `workspace.package.version`**
- Rationale: Minimal diff; avoids reworking workspace metadata in the same change as the version bump. Keeps this change focused on versioning semantics.
- Alternative considered: centralize version in workspace root - deferred to a follow-up cleanup change, since it touches every crate's manifest structure.

**D3: Release checklist as a documented process file, not a script**
- Rationale: The checklist gates a human-maintained tag; a script would need ongoing maintenance and its own tests. A checklist document (checklist of commands + privacy audit scope) is the smallest artifact that satisfies the "mandatory release checklist" spec requirement.
- Alternative considered: `scripts/release.sh` enforcing every gate - rejected for now (YAGNI until CI publishing exists); can be added when automation lands.

**D4: Tag name `v2.0.0-alpha.1` matching crate versions**
- Rationale: Common convention (`v<semver>`); matches the sync-toon-packet `format_version` style of explicit versioning used elsewhere in the repo. Single source of truth for what "the release" means.

## Risks / Trade-offs

- [Semver jump 0.1.0 → 2.0.0-alpha.1 looks drastic] → Mitigation: document the roadmap intent in proposal + release doc; alpha prerelease marker keeps semver resolution honest for dependents.
- [Workspace crates drift if future bumps are applied one-by-one] → Mitigation: checklist mandates a single commit bumping all crates; spec requirement "workspace crate version alignment" enforces it.
- [Cargo.lock mismatch if a crate bumps without the others] → Mitigation: `cargo build`/`cargo test` verification in checklist surfaces lock drift.
- [Privacy audit scope creep] → Mitigation: checklist pins audit scope to default configs + docs (rule #2448/#3091); no code review in this change.

## Migration Plan

1. Bump all six crates to `2.0.0-alpha.1` + regenerate `Cargo.lock` (`cargo build`).
2. Run checklist gates: zero-warning build, `cargo clippy --all-targets --all-features`, `cargo test`, `graphify index` smoke test, privacy audit.
3. Create `docs/release-process.md` (or extend existing docs) capturing version strategy + checklist.
4. On user approval: `git tag v2.0.0-alpha.1` and push.
5. Rollback: revert the bump commit; no code behavior change means rollback is trivial.

## Open Questions

None - deferrable unknowns (CI automation, `cargo publish` pipeline) are explicitly out of scope and listed in Non-Goals.
