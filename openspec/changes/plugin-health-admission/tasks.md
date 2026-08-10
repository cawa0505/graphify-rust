# Tasks: Plugin Health & Admission Protocol

## Phase 1 — Registry (plugin-health-status)

- [ ] 1.1 Extend `PluginStatus` enum to four states: `Healthy`, `Degraded`, `Unavailable`, `Quarantined` (graphify-registry/src/db.rs:35), updating `as_str`/parsing and the status CHECK constraint vocabulary
- [ ] 1.2 Add `migrate_to_v2` to `graphify-registry/src/db.rs`: rebuild `plugin_registrations` table with new CHECK constraint, mapping `Ready` → `Healthy`, `Unavailable` → `Unavailable`; bump `SCHEMA_VERSION` to 2; verify row count preserved in test
- [ ] 1.3 Add registry query API: list all plugin registrations for a workspace with status, ordered by `plugin_id` (for CLI + TUI)
- [ ] 1.4 Unit tests: status round-trip per (plugin_id, workspace_key), invalid status rejected, migration preserves rows, unregistered read returns nothing

## Phase 2 — Circuit Breaker (plugin-circuit-breaker)

- [ ] 2.1 Add `validate_envelope` pure function in graphify-core/src/plugin.rs: checks record_id/workspace_key/plugin_id/payload/created_at presence and types, returns `Result<(), String>`
- [ ] 2.2 Add `CircuitBreaker` struct in graphify-cli/src/plugin_host.rs: per-(plugin_id, workspace_key) consecutive-failure counter, `record_failure`/`record_success`/`is_bypassed`
- [ ] 2.3 Rewrite `PluginHost::broadcast` to invoke each hook via `std::thread` + `recv_timeout(500ms)`; count timeout/panic/schema-rejection as failure; on 3rd consecutive failure set status `Quarantined` via registry; skip quarantined plugins entirely
- [ ] 2.4 Wire `validate_envelope` into the envelope write path (graphify-cli/src/rehydrate.rs) so invalid payloads are dropped with a named warning before any Qdrant write
- [ ] 2.5 Unit tests: slow hook aborted after 500ms, fast hook unaffected, 3x failure quarantines, success resets counter, quarantined plugin bypassed, invalid envelope dropped with warning

## Phase 3 — Passive Health Probe (plugin-health-status + CLI)

- [ ] 3.1 Add CLI command `graphify plugin health [workspace]`: runs the passive probe (invoke `on_health_check()` per registered plugin), writes resulting status to SQLite, prints table (id, status, reason)
- [ ] 3.2 Add CLI command `graphify plugin reset <plugin_id>` (or `--all`): clears quarantine, re-probes, writes probe result; used by TUI [F5] underneath
- [ ] 3.3 Probe semantics: `on_health_check()` true → `Healthy`; false → `Unavailable`; trait method absent → `Unavailable` (no false positives); verify with unit tests

## Phase 4 — TUI Workspace & Monitor (tui-workspace-monitor)

- [ ] 4.1 TUI startup workspace selector: read registry workspaces (id, root, last_indexed_at), full-screen list, arrow keys + Enter to select, Esc to fall back to cwd `.toon`; load selected workspace's `.toon` (or clear error if missing)
- [ ] 4.2 Empty registry → current cwd behavior unchanged (load `graphify-out/graph.toon`)
- [ ] 4.3 Plugin panel: `p` key opens overlay (reuse existing modal infra, tui.rs:136-253) listing active workspace's plugin registrations with status; Esc closes; default view unchanged
- [ ] 4.4 `[F5]` in panel: reset all quarantined plugins (reuse reset command), re-probe, refresh panel; no-op when nothing quarantined
- [ ] 4.5 Keep existing inspector interactions intact (keyboard/mouse/search/$EDITOR); verify no regression in TUI behavior

## Phase 5 — E2E Integration Test (M4)

- [ ] 5.1 Add tests/e2e_health_admission.rs: good plugin + bad plugin registered; 3 broadcasts → bad quarantined (assert via SQLite); 4th broadcast bypasses bad, good still receives
- [ ] 5.2 E2E: run `graphify extract` with quarantined plugin present → `.toon` exports normally (Core unaffected)
- [ ] 5.3 E2E: reset command unquarantines and re-probes; final state matches probe result
- [ ] 5.4 Run `cargo test` full suite + `cargo clippy --all-targets --all-features` (zero warnings) before commit

## Phase 6 — Release & Docs

- [ ] 6.1 Update `docs/release-process.md` if the beta release flow differs (milestone + tag naming for v2.0.0-beta.1)
- [ ] 6.2 Mark `plugin-health-admission` change artifacts complete (`openspec validate`)
