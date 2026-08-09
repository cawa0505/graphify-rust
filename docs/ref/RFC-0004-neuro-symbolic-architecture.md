# RFC-0004: Graphify Neuro-Symbolic Architecture & Memory Plugin System

Status: Proposed

Authors: Graphify Core Team

Created: 2026-08-09

Target Release: Graphify v2.0-alpha

## Executive Summary

Graphify 旨在升級為兼具 16ms 符號拓撲（Petgraph AST） 與 長效向量記憶（Qdrant Neural Memory） 的雙模架構（Neuro-Symbolic Architecture）。本 RFC 規範了 Graphify 的全域記憶體矩陣、三層儲存設計、原生與第三方 Plugin 隔離機制、.toon 豐富化（Enrichment）規範、以及跨 Session / 跨 Agent 的 HandoffSnapshot 接力恢復流程。

## System Architecture Overview

Graphify 採用三層分級儲存架構（Tri-Layer Storage Architecture），實現「全域中繼資料、靜態結構拓撲、神經向量記憶」的完全解耦：

```plaintext
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│ Graphify Control Gateway │
└──────────────┬───────────────────────────┬──────────────────────────────┬───────────────┘
               │                            │                              │
               ▼                            ▼                              ▼
┌─────────────────────────────┐  ┌───────────────────────────┐  ┌─────────────────────────────┐
│ 1. Global Registry          │  │ 2. Symbolic AST Engine    │  │ 3. Neural Vector Engine     │
│ (SQLite)                    │  │ (Petgraph In-Memory)      │  │ (Qdrant Unified Engine)     │
│                             │  │                           │  │                             │
│ - Global workspace_key map  │  │ - 16ms BFS Blast Radius   │  │ - Qdrant Local (Embedded)   │
│ - Plugin Registrations      │  │ - Pure deterministic code │  │   OR External Qdrant Server │
│ - Handoff Global Index      │  │   topology (.toon)        │  │ - Domain Memory Collections │
└─────────────────────────────┘  └───────────────────────────┘  └─────────────────────────────┘
```

## 1. Tri-Layer Storage Infrastructure

### 1.1 Global Metadata Registry (SQLite)

Location: `~/.graphify/graphify.db`

Role: 儲存跨專案中繼資料、工作區路徑映射與全局狀態。

Schema:

```sql
CREATE TABLE IF NOT EXISTS workspaces (
    workspace_key TEXT PRIMARY KEY,
    workspace_name TEXT NOT NULL,
    root_path TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    last_indexed_at INTEGER NOT NULL,
    ast_node_count INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT 1
);

CREATE TABLE IF NOT EXISTS plugin_registrations (
    plugin_id TEXT NOT NULL,
    workspace_key TEXT NOT NULL,
    qdrant_collection_name TEXT NOT NULL,
    last_synced_at INTEGER,
    status TEXT NOT NULL,
    PRIMARY KEY (plugin_id, workspace_key),
    FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS handoff_registry (
    snapshot_id TEXT PRIMARY KEY,
    workspace_key TEXT NOT NULL,
    session_id TEXT NOT NULL,
    task_goal TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
);
```

### 1.2 Symbolic Topology Engine (Petgraph + .toon)

Role: 記憶體內 16ms 毫秒級極速 AST 代碼圖譜與衝擊分析，完全離線且無 Token 浪費。

Data Contract: 由 Graphify Core 掌管唯一寫入權，維持單一真實來源（Single Source of Truth, SSOT）。

### 1.3 Neural Vector Engine (Qdrant Local / Server Dual-Track)

Role: 支援各 Plugin 獨立 Collection 的向量檢索。

Dual-Track Mechanism:

- Default / Fallback: Qdrant Local 內嵌模式（直接寫入 `~/.graphify/storage/`），零伺服器依賴、開箱即用。
- Server Mode: 當外部 Qdrant 伺服器（gRPC / REST）可用時自動升級；斷線時秒級無縫降級至 Local 模式。

```rust
pub enum StorageMode {
    LocalPath(std::path::PathBuf),
    ServerUrl(String),
}

pub struct GraphifyVectorStore {
    client: qdrant_client::Qdrant,
    mode: StorageMode,
}

impl GraphifyVectorStore {
    pub async fn init_with_fallback(server_url: &str, local_path: std::path::PathBuf) -> Self {
        if let Ok(client) = qdrant_client::Qdrant::from_url(server_url).build() {
            if client.health_check().await.is_ok() {
                return Self { client, mode: StorageMode::ServerUrl(server_url.to_string()) };
            }
        }
        let client = qdrant_client::Qdrant::from_path(&local_path).build()
            .expect("Failed to initialize Qdrant Local storage");
        Self { client, mode: StorageMode::LocalPath(local_path) }
    }
}
```

## 2. Plugin Domain Memory & Access Rules

### 2.1 Memory Isolation Principles

- **Read-Only Core Memory**: Core Memory 只由 Graphify Indexing Pipeline 寫入，Plugin 僅能讀取。
- **Collection-per-Plugin (Database-per-Module)**: 每個 Plugin 使用系統託管產生的獨立 Collection (`graphify_plugin_<plugin_id>`)，避免 Schema 與向量維度互相干擾。
- **Safe Memory Gateway**: 第三方/MCP Plugin 不得直接操作底層 Data Driver，必須透過帶有 workspace_key 邊界校驗的受限介面（如 `graphify_memory_query`）進行存取。

### 2.2 Versioned Envelope & Domain Payloads

所有 Plugin 寫入其專屬 Collection 時，必須包裝於統一的 `PluginMemoryEnvelope<T>` 中：

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMemoryEnvelope<T> {
    pub format_version: u32,
    pub workspace_key: String,
    pub plugin_id: String,
    pub record_id: String,
    pub record_kind: String,
    pub created_at: i64,
    pub source_refs: Vec<String>,
    pub payload: T,
}

// Plugin Domain Payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenDocPayload {
    pub doc_identity: String,
    pub doc_version: String,
    pub chunk_index: usize,
    pub raw_content: String,
    pub linked_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPayload {
    pub review_id: String,
    pub git_commit_sha: Option<String>,
    pub affected_symbols: Vec<String>,
    pub finding_severity: String,
    pub resolution_status: String,
    pub review_comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffPayload {
    pub task_goal: String,
    pub pinned_node_ids: Vec<String>,
    pub focused_subgraph_toon: String,
    pub reconstructable_query_metadata: serde_json::Value,
}
```

## 3. .toon Schema Enrichment Specification

為防範 Plugin 隨意添加根欄位導致 Schema Drift 與 Token 膨脹，所有 Plugin 的附加中繼資料必須收覆於 reserved `plugin_data` 容器中：

```yaml
metadata:
  format_version: "1"
  workspace_key: "ws_backend_core_9921"
  generated_at: 1786252800

plugin_data:
  opendoc:
    linked_doc_id: "doc_spec_v3.pdf"
    section_ref: "3.2.1"
  review:
    finding_severity: "Warning"
    historical_pitfall_id: "pitfall_142"
  handoff:
    task_phase: "execution"
    pinned_symbols: ["MemoryConfig", "flush_cache"]

nodes:
  - id: N1
    symbol: "crates/core/src/memory.rs::MemoryConfig"
    kind: "struct"
  - id: N2
    symbol: "crates/store/src/cache.rs::flush_cache"
    kind: "fn"

edges:
  - from: N1
    to: N2
    relation: "calls"
```

## 4. Handoff Snapshot & Reconstruction Mechanism

### 4.1 Resilient Reference Model

HandoffSnapshot 嚴禁依賴 Qdrant Point ID。快照必須保存確定性 Graphify AST Node IDs (workspace_key + Node Canonical Path) 與可重建的查詢條件，確保在 Vector Re-index、Collection 遷移或離線跨機時均能 100% 恢復拓撲視野。

### 4.2 Handoff Contracts

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryCriteria {
    pub target_symbols: Vec<String>,
    pub domain_categories: Vec<String>,
    pub search_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSnapshot {
    pub session_id: String,
    pub workspace_key: String,
    pub task_goal: String,
    pub pinned_node_ids: Vec<String>,
    pub focused_subgraph_toon: String,
    pub memory_query_metadata: MemoryQueryCriteria,
    pub timestamp: i64,
}
```

## 5. Graceful Degradation & Health Status

當 Embedding Provider（如 本地 Ollama、ONNX 或雲端 API）不可用時，系統遵循 「Symbolic（符號）為骨架，Neural（神經）為增強」 原則進行優雅降級：

- **AST Topology Fault-Tolerance**: Petgraph 及 .toon 計算 100% 保持正常（16ms 響應）。
- **Explicit Status Report**: 向量查詢介面明確回傳 `MemoryStatus::Unavailable` 錯誤，不回傳假資料（Null/Hash Vectors）或空結果，避免 Agent 產生語意誤判。
- **Resync Pipeline**: Provider 恢復運作後，背景執行 memory sync 補齊向量，無須重建已有 AST 圖譜。

## 6. Native Plugin Specifications

### 6.1 graphify-plugin-opendoc

機能: 解析非結構化/二進位文檔（.xlsx, .pdf, .docx），提取 Chunk 與關聯 Symbol 寫入 `graphify_plugin_opendoc` Collection，並向 Core 發射 16ms 雙向追蹤。

### 6.2 graphify-plugin-review

機能: 計算 git diff 之 BFS 衝擊半徑，結合 Qdrant 中的歷史 Review 經驗（Pitfalls），為當前修改與本地小模型（7B/14B）提供零幻覺審查。

### 6.3 graphify-plugin-handoff

機能: 將 Agent 當前心智狀態與 Focused AST 子圖壓縮為 ~1.5KB 的 HandoffSnapshot，實作跨 Session 續接與 Multi-Agent 分工流水線。

## 7. TUI Architecture & Multi-Workspace Inspector

TUI 採用 Graphify Control Gateway，提供 [Plugins & Memory] 控制台，支援多工作區動態切換：

```plaintext
┌─ Graphify Control Center ─────────────────────────────────────────── [Plugins & Memory] ─┐
│ Active Workspace: [ 1. Backend-core (/Users/dev/backend) ▼ ] Mode: [Live Tracking]     │
├───────────────────────────────────────────────────────────────────────────────────────────┤
│ ┌─ Core & Neural Health ────────────────┐  ┌─ Domain Memory Storage ──────────────────┐  │
│ │ Petgraph AST Engine: ACTIVE (16ms)   │  │ [opendoc] 2,410 chunks | Qdrant Local    │  │
│ │ Qdrant Vector DB: LOCAL (Embedded)   │  │ [review] 182 findings  | Qdrant Local    │  │
│ │ Embedding Provider: READY (Ollama)   │  │ [handoff] 5 snapshots | Qdrant Local     │  │
│ └───────────────────────────────────────┘  └────────────────────────────────────────────┘  │
│ ┌─ Active Plugins & Safety ─────────────┐  ┌─ Live Relay & Trace Monitor ───────────────┐  │
│ │ [✓] graphify-plugin-opendoc (v1.2)   │  │ [10:42:01] Review trace: 3-step Blast Radius │ │
│ │ [✓] graphify-plugin-review (v2.0)    │  │ [10:42:03] Handoff exported (1.4 KB .toon)   │ │
│ │ [✓] graphify-plugin-handoff (v1.0)   │  │ [10:42:15] OpenDoc: Ref 'MemoryConfig' linked│ │
│ └───────────────────────────────────────┘  └────────────────────────────────────────────┘  │
│ [F1] Re-index AST  [F2] Sync Memory  [F3] Clear Handoffs  [F5] Purge Plugin Cache       │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

## Rationale & Alternatives Considered

**Why SQLite over pure JSON/YAML file for global registry?**
SQLite 提供 0.1ms 極速查詢、Transactional 併發鎖定與內建 JSON 工具，能完美滿足 TUI 即時繪製與多 Agent 寫入，避免檔案競爭與數據損毀。

**Why Qdrant Local over SQLite-vec as primary fallback?**
Qdrant Local 模式與 Qdrant Server 共用相同的 API、Query DSL 與 Rust SDK，完全消除了別名適配層（Adapter Code）的維護成本。

## Unresolved Questions & Future Work

- **Handoff Auto-Pruning**: 未來需針對 handoff_registry 定義基於 LRU 與 TTL 的背景自動清理機制。
- **Cross-Workspace Knowledge Synergy**: 研議是否允許授權的特定 Plugin 進行跨 Workspace 的跨域向量聯合檢索（Cross-Workspace Joint Search）。
