# Graphify — Assembly Manifest

> 構想文件｜先記錄概念，不代表立即實作
> （docs/ref/ 逐字快照，2026-09-21 由用戶提供）

## 核心概念

Graphify 的基本積木是 **Workspace**。

每個 Workspace 內部都有自己的 AST / local graph：

```text
Workspace A
└── AST Graph

Workspace B
└── AST Graph

Workspace C
└── AST Graph
```

而 **YAML 是一份「積木組裝說明書」**。

它不是宣告整個系統唯一正確的 architecture，而是描述：

> 「我要把哪些既有 Workspace，以什麼關係組成一個我現在要看的視角。」

因此，同一批 Workspace 可以存在多份 YAML，每一份代表不同的組裝方式。

## 多份 Assembly Manifest

例如同一批 workspace：

```text
AgentShop
NexusLedger
WooCommerce
TeleFlux
Teletran
```

可以有：

```text
ecommerce.yaml
observability.yaml
agent.yaml
```

### ecommerce.yaml

```yaml
workspaces:
  - agentshop
  - nexusledger
  - woocommerce

relations:
  - from: agentshop
    type: uses
    to: nexusledger

  - from: agentshop
    type: executes_via
    to: woocommerce
```

### observability.yaml

```yaml
workspaces:
  - agentshop
  - teletran
  - teleflux

relations:
  - from: teleflux
    type: audits
    to: teletran

  - from: teletran
    type: serves
    to: agentshop
```

同一組積木，不同說明書，就得到不同的 Architecture Graph。

## Assembly → Graph → Diagram

基本流程：

```text
Assembly Manifest (.yaml)
          │
          ▼
   Graphify Assembly
          │
          ▼
   Architecture Graph
          │
          ▼
    Diagram Projection
          │
          ▼
      box-of-rain
          │
       ┌──┴──┐
       ▼     ▼
     ASCII   SVG
```

### Renderer

第一個 renderer 候選：

**box-of-rain**

Repository: https://github.com/switz/box-of-rain

它負責 layout / diagram rendering；Graphify 不需要把 layout engine 放進 Core。

## 可以往下展開 AST

Architecture Graph 不是終點。

一個 Workspace node 可以被展開：

```text
Architecture Graph
       │
       ├── AgentShop
       │      │
       │      └── expand
       │             ↓
       │          Workspace AST
       │             ├── src/
       │             ├── gateway/
       │             ├── purchase/
       │             └── policy/
       │
       └── NexusLedger
              │
              └── expand
                     ↓
                  Workspace AST
```

因此 Graphify 可以提供由 macro 到 micro 的 navigation：

```text
Assembly
  ↓
Architecture
  ↓
Workspace
  ↓
Directory / File
  ↓
AST
  ↓
Code structure
```

## 組裝模型

可以把 Graphify 的 graph 看成兩種關係：

```text
Workspace
   │
   └── contains / derives
          ↓
       Local AST Graph

Workspace
   │
   └── architecture relation
          ↕
       Workspace
```

其中：

* Workspace 是 graph composition 的基本 boundary。
* AST 描述 Workspace 內部結構。
* Assembly Manifest 描述 Workspace 之間如何被組裝。
* Relation 是 composition edge。
* Renderer 負責把某一份 assembly 投影成視覺圖。

## 重要原則

1. **一個 YAML 是一份組裝說明書，不是系統唯一真相。**
2. 同一批 Workspace 可以有多份 assembly manifest。
3. 不把 Workspace 內部 AST 再手寫進 YAML；由 Graphify 自動產生。
4. Architecture relation 與 code AST 分層。
5. Assembly 可以只選擇需要的 Workspace，不必載入整個世界。
6. Diagram renderer 與 Graph model 分離。
7. `box-of-rain` 先作 renderer 積木，不重新實作 layout engine。
8. 先驗證 `Assembly YAML → Graphify Graph → box-of-rain → diagram`。
9. MCP 是後續可能的 Agent interface，不是這個構想的必要前提。
10. 這是一個構想，不代表現在立即實作。

## 一句話

> **YAML 是積木的組裝說明書；Graphify 負責把說明書組成 Graph，box-of-rain 負責把 Graph 畫出來，而 Workspace 還可以繼續往下展開成 AST。**
