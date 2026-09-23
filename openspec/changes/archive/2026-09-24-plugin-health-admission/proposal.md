# Proposal: Plugin Health & Admission Protocol (v2.0-beta M2/M3/M4)

## Motivation

v2.0-alpha（Core Base）已交付並釋出 v2.0.0-alpha.1。依 `docs/ref/SPEC-2026-v2beta-roadmap.md`（v2.0-beta: Ecosystem & Governance），下一階段使命為**系統防禦（System Governance）**：確保第三方 Plugin 無論多異常，Rust Core 都能維持穩定。

現況差距（2026-08-10 調研，見 roadmap Note）：

- `PluginHost::broadcast` 僅用 `catch_unwind` 隔離 panic；**無 500ms 執行超時、無 Schema Strict Validation、無 3x Auto-Quarantine**。
- `plugin_registrations.status` 僅 `Ready`/`Unavailable` 二態，缺 `Degraded` 與 `Quarantined`。
- 無 Passive Health Probe（10ms 三態）。
- TUI 圖譜 Inspector 完整但 cwd 綁定，**無 workspace 切換、無 Plugin 健康度面板**，`[F5]` 一鍵復原不存在。

本 change 實作 roadmap 的 Beta-M2（熔斷 + Schema Filter）、Beta-M3（Health Probe + SQLite 三態 + TUI workspace 切換 + Plugin 面板 + [F5]）、Beta-M4（E2E Integration Test）。

## Scope

**In scope：**

- `PluginStatus` 擴充為四態：`Healthy` / `Degraded` / `Unavailable` / `Quarantined`（SQLite migration v2）。
- Plugin 執行熔斷：500ms hard timeout（Native trait 呼叫端）、連續失敗 3 次 → Quarantined，阻斷後續調用直到手動重置。
- Envelope Schema Strict Validation：Plugin 回傳 payload 不符合 `PluginMemoryEnvelope` 標準即捨棄並記錄 warning。
- Passive Health Probe：CLI 指令 / TUI 啟動時發起 <10ms ping，回報三態（Healthy/Degraded/Unavailable）並寫入 SQLite。
- TUI：workspace 切換器（讀 SQLite registry，取代 cwd 綁定）+ Plugin 健康度面板 + `[F5]` 重置 Quarantine 並重新探測。
- E2E Integration Test：異常 Plugin 被隔離且不影響 Core AST/.toon 導出。

**Out of scope：**

- `graphify-sdk` Python SDK（Beta-M1）— 已決策暫緩至 2.0 release 之後（roadmap Scope Decision Note）。
- 常駐背景 Health Probe 執行緒 — 依規格 §2.2 明確拒絕 background polling。
- gRPC transport / HNSW（P6 已 archived，SQL 驅動 batch upsert 取代）。

## Out of Scope

（見上節 In/Out scope 段落）

## Impact

| 影響面 | 內容 |
|--------|------|
| graphify-registry | `PluginStatus` 四態 + schema migration v2 + quarantine 相關 API |
| graphify-core | plugin trait 無需更動；若需 timeout 則在呼叫端包裝 |
| graphify-cli | `PluginHost::broadcast` 加 timeout/計數、`graphify tui` workspace 切換 + Plugin 面板 + [F5]、新 CLI 指令（probe/reset） |
| graphify-mcp | 若 MCP 有 plugin 工具註冊，需同步健康狀態過濾 |
| SQLite | `plugin_registrations` migration（status CHECK 約束擴充） |

## Alternatives Considered

- **常駐探針執行緒**：被規格明確拒絕（§2.2 拒絕常駐 Thread 背景 Polling），採用 CLI/TUI 觸發式。
- **Process-level sandbox（sidecar 進程隔離）**：v2.0-beta 範圍維持 in-process 呼叫 + 熔斷；完整 sandbox 屬後續版本（spec 未要求）。
