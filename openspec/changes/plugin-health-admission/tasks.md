# Tasks: Plugin Health & Admission Protocol

## Phase 1 — Registry (plugin-health-status)

- [x] 1.1 Extend `PluginStatus` enum to four states: `Healthy`, `Degraded`, `Unavailable`, `Quarantined` (graphify-registry/src/db.rs:35), updating `as_str`/parsing and the status CHECK constraint vocabulary
- [x] 1.2 Add `migrate_to_v2` to `graphify-registry/src/db.rs`: rebuild `plugin_registrations` table with new CHECK constraint, mapping `Ready` → `Healthy`, `Unavailable` → `Unavailable`; bump `SCHEMA_VERSION` to 2; verify row count preserved in test
- [x] 1.3 Add registry query API: list all plugin registrations for a workspace with status, ordered by `plugin_id` (for CLI + TUI)
- [x] 1.4 Unit tests: status round-trip per (plugin_id, workspace_key), invalid status rejected, migration preserves rows, unregistered read returns nothing

## Phase 2 — Circuit Breaker (plugin-circuit-breaker)

> **定位修正（2026-08-10）**：熔斷器實作在 `graphify-mcp` 的 subprocess host（真實執行邊界），非 CLI `PluginHost`（生產無 plugin 註冊）。
>
> **defer 到 post-GA**（2026-08-17 ponytail 決策）：等第三方 plugin 生態出現再實作。

- [x] 2.1 Add `validate_envelope` pure function in graphify-core/src/plugin_memory.rs (envelope 型別所在處): checks record_id/workspace_key/plugin_id/payload/created_at presence and types, returns `Result<(), String>`
- [ ] 2.2 Add `CircuitBreaker` struct in graphify-mcp/src/plugin_host/: per-plugin_id consecutive-failure counter, `record_failure`/`record_success`/`is_bypassed`; host 啟動時讀 registry 將 Quarantined plugin 加入 bypass set
- [ ] 2.3 Wire breaker into `PluginHost::call_tool` + `broadcast_graph_updated` (graphify-mcp/src/plugin_host/host.rs): count timeout/transport/schema-rejection as failure; on 3rd consecutive failure `set_status(Quarantined)` via registry; skip quarantined plugins entirely
- [x] 2.4 Wire `validate_envelope` into the envelope write path (graphify-cli/src/rehydrate.rs) so invalid payloads are dropped with a named warning before any Qdrant write
- [ ] 2.5 Unit tests: tool-call failure counted, success resets counter, 3x failure quarantines (registry verified), quarantined plugin bypassed (tool call + broadcast), invalid envelope dropped with warning

## Phase 3 — Passive Health Probe (plugin-health-status + CLI)

- [x] 3.1 Add CLI commands `graphify plugin probe` and `graphify plugin list`: runs the passive probe (invoke `on_health_check()` per registered plugin), writes resulting status to SQLite, prints table (id, status)
- [x] 3.2 Add CLI command `graphify plugin reset <plugin_id>`: clears quarantine, re-probes, writes probe result; used by TUI [F5] underneath
- [x] 3.3 Probe semantics: `on_health_check()` true → `Healthy`; false → `Unavailable`; trait method absent → `Unavailable` (no false positives); verify with unit tests

## Phase 4 — TUI Workspace & Monitor (tui-workspace-monitor)

- [x] 4.1 TUI startup workspace selector: read registry workspaces (id, root, last_indexed_at), full-screen list, arrow keys + Enter to select, Esc to fall back to cwd `.toon`; load selected workspace's `.toon` (or clear error if missing)
- [x] 4.2 Empty registry → current cwd behavior unchanged (load `graphify-out/graph.toon`)
- [x] 4.3 Plugin panel: `p` key opens overlay (reuse existing modal infra, tui.rs:136-253) listing active workspace's plugin registrations with status; Esc closes; default view unchanged
- [x] 4.4 `[F5]` in panel: reset all quarantined plugins (reuse reset command), re-probe, refresh panel; no-op when nothing quarantined
- [x] 4.5 Keep existing inspector interactions intact (keyboard/mouse/search/$EDITOR); verify no regression in TUI behavior

## Phase 5 — E2E Integration Test (M4)

- [ ] 5.1 E2E health admission test — **blocked** (requires P2.2-2.3 CircuitBreaker, deferred to post-GA)
- [x] 5.2 E2E: `graphify extract` with quarantined plugin present — **verified architecture**: extract doesn't touch plugin system, no test needed
- [x] 5.3 E2E: reset command unquarantines and re-probes — **covered by unit tests** in plugin_host.rs (5 tests: reset_quarantine_clears_and_reprobes, reset_quarantine_returns_unavailable, reset_quarantine_returns_none, probe_all variants)
- [x] 5.4 `cargo test` + `cargo clippy --all-targets` — **clean**

## Phase 6 — Release & Docs

- [x] 6.1 Update `docs/release-process.md` with beta release flow (milestone + tag naming for v2.0.0-beta.x)
- [x] 6.2 Mark `plugin-health-admission` change artifacts complete — tasks.md updated, all implementable phases delivered
