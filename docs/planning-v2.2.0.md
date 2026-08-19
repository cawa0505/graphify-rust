# Graphify v2.2.0 Planning

> 發布目標：v2.2.0 (next release after v2.1.0)
> 狀態：Planning
> 上次更新：2026-08-19

---

## 當前狀態 (As-Is)

### 版本
- 所有 workspace crates: **v2.1.0**
- 最新 tag: `v2.0.0-beta.1` (尚未 tag v2.1.0)

### 已完成 (P1–P4 全數交付)

| 里程碑 | 變更 | 狀態 |
|--------|------|------|
| P1 | HandoffSnapshot 補全 | ✅ delivered |
| P2 | SQLite Global Registry | ✅ delivered |
| P3 | TUI Stage 1 (Inspector 基礎) | ✅ delivered |
| P4 | Qdrant Local Fallback | ✅ delivered |

### 已規劃未實作

| 變更 | 設計 | 任務 | 優先級 |
|------|------|------|--------|
| **mcp-tool-ux-v2** | ✅ design.md | ❌ 未開始 | 高 |
| **cli-tui-graph** (TUI Stage 2) | ✅ design.md | ❌ 未開始 | 高 |

### Draft Specs (待規劃)

| Spec | 狀態 |
|------|------|
| api-rotation | draft |
| config-compat | draft |
| llm-gateway-contract | draft |
| mcp-server | draft |

---

## v2.2.0 範圍

### 目標

1. **MCP Tool UX v2** — 統一命名、help 工具、自動 broadcast、workspace_key 預設值、relay auto-save
2. **TUI Stage 2** — plugin monitor 面板、workspace selector、BFS modal overlay
3. **v2.1.0 補 tagging**

### 排程

```
v2.1.0 tag ───→ mcp-tool-ux-v2 ───→ cli-tui-graph (Stage 2) ───→ v2.2.0 release
```

#### P0: v2.1.0 補 Tag
- 對當前 main 加上 `v2.1.0` tag 並推送
- 更新 release-process.md 的 History 記錄

#### P1: mcp-tool-ux-v2
- 源自 `openspec/changes/mcp-tool-ux-v2/`
- 7 個 task 區塊 (tasks.md)
- 設計設計審查通過 (design.md)
- **breaking change**: MCP tool 名稱全部 snake_case

#### P2: cli-tui-graph (TUI Stage 2)
- 源自 `openspec/changes/cli-tui-graph/`
- 在現有 TUI Inspector 上疊加新面板
- 7 個 task 區塊 (tasks.md)
- 不破壞既有 Inspector 互動 (spec 強約束)

---

## 未來展望 (v2.3.0+)

| 項目 | 說明 | 依賴 |
|------|------|------|
| LLM Gateway Contract | Plugin 共享 LLM 合約 | mcp-tool-ux-v2 |
| API Rotation Spec | Provider failover 文件化 | — |
| Config Compat Spec | JSON→TOML 相容性文件化 | — |
| MCP Server Spec | graphify-mcp 架構文件化 | mcp-tool-ux-v2 |
| File Extension 擴展 | 36 種 tree-sitter grammar | 長期 |

---

## Release Checklist (v2.2.0)

- [ ] `cargo build --all-targets` — zero warnings
- [ ] `cargo clippy --all-targets --all-features` — zero warnings
- [ ] `cargo test` — all pass
- [ ] `graphify index <real-path> -f` — smoke test
- [ ] 隱私審查 (規則 #2448)
- [ ] 所有 crate 版本 bump → 2.2.0
- [ ] tag `v2.2.0` + push
