# Graphify — Hierarchical Graph Assembly

> 構想文件｜先記錄概念，不代表立即實作
> （docs/ref/ 逐字快照，2026-09-21 由用戶提供）

## 核心構想

Graphify 的 Graph 組裝方式從「單一 Workspace 內的 AST Graph」擴展成兩個層級：

- Workspace Graph：每個 workspace 內，由目錄 / code 結構產生 AST。
- Architecture Graph：多個 workspace 之間，以語意 relation 描述系統架構。

```text
Architecture Graph
        │
        ├── Workspace A
        │      └── AST Graph
        ├── Workspace B
        │      └── AST Graph
        └── Workspace C
               └── AST Graph
```

Workspace 是 composition boundary；workspace relation 是 composition edge。

## 新的組裝方式

原本：

```text
Workspace → Parse → AST → Graph
```

新的模型：

```text
Workspace
  └── Local AST / Graph

Workspace
  └── Local AST / Graph

Workspace
  └── Local AST / Graph

        ↕
Architecture Relations
```

Architecture relation 不描述 workspace 內部的 code 結構，而描述 workspace 與 workspace
之間的語意關係，例如 uses、audits、integrates、executes_via、depends_on、provides。

## Manifest 草案

```yaml
workspaces:
  - id: agentshop
    path: ./AgentShop

  - id: nexusledger
    path: ./NexusLedger

  - id: teleflux
    path: ./TeleFlux

relations:
  - from: agentshop
    type: uses
    to: nexusledger

  - from: teleflux
    type: audits
    to: agentshop
```

人只描述 workspace 與 architecture relation；workspace 內部的 AST 由 Graphify 自動產生。

## Unified Graph

```text
Architecture Model
       │
       ├── Workspace Graphs
       │       └── AST
       │
       └── Workspace Relations
               │
               ▼
          Unified Graph
               │
        ┌──────┼──────┐
        ▼      ▼      ▼
       TUI    TOON   Diagram
```

可以從 macro architecture zoom in 到 workspace，再繼續 zoom in 到 AST / code structure。

## Diagram Renderer

第一個 renderer 候選：box-of-rain

Repository: https://github.com/switz/box-of-rain

流程：

```text
Graphify Unified Graph
        ↓
Diagram Projection
        ↓
box-of-rain
        ↓
ASCII / SVG
```

box-of-rain 先作為 renderer，不把 layout engine 納入 Graphify Core。

## MCP 方向

未來可以提供 Diagram MCP，使 Agent 能要求：

- 「畫出 AgentShop 的 architecture」
- 「把 AgentShop 展開到 AST」
- 「畫出 TeleFlux 與相關 workspace」

可能流程：

```text
Agent
  ↓
Diagram MCP
  ↓
Graphify query
  ↓
semantic / architecture graph
  ↓
diagram projection
  ↓
box-of-rain
  ↓
ASCII / SVG
```

## 原則

1. 不先擴 Graphify Core。
2. Architecture relation 優先作為 Plugin / 外部資料來源。
3. Diagram renderer 與 Graph model 分離。
4. box-of-rain 先作 renderer 積木，不重新實作 layout。
5. 先驗證 Graphify Graph → box-of-rain input → diagram，再決定是否需要 MCP。

這是一個構想，不代表現在就進入實作。

## 一句話

Graphify 不只組裝 code AST，也組裝 workspace 之間的 architecture relation；
workspace 是 graph boundary，relation 是 graph composition edge。
