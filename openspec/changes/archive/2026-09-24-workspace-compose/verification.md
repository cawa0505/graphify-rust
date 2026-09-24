# workspace-compose 驗證證據（2026-09-24）

## 6.1 e2e narrative（多 workspace fixture 全鏈）

Fixture：`/tmp/opencode/compose-e2e/`（ws-a、ws-b 各含一個 Rust 檔，真實 `graphify index` 產生 `graphify-out/graph.toon`）。

Assembly Manifest（assembly.yaml）：

```yaml
workspaces:
  - id: ws-a
    path: ws-a
  - id: ws-b
    path: ws-b
relations:
  - from: ws-a
    type: uses
    to: ws-b
```

### validate

```
$ graphify compose validate assembly.yaml
✓ manifest 驗證通過: workspaces 2 個, relations 1 條 (assembly.yaml)
```

### graph

```
$ graphify compose graph assembly.yaml
{"cross_edges":1,"total_edges":3,"total_nodes":6,"workspaces":2}
```

### render（box-of-rain ASCII）

```
$ graphify compose render assembly.yaml

┌────────────────┐          ┌────────────────┐
│  ┌──────────┐  │          │  ┌──────────┐  │
│  └──────────┘  │          │  └──────────┘  │
│                │ ─ uses ─▶│                │
│  ┌──────────┐  │          │  ┌──────────┐  │
│  └──────────┘  │          │  └──────────┘  │
└────────────────┘          └────────────────┘
```

（雙 workspace 容器 box + `─ uses ─▶` 跨域 connection，npx box-of-rain 實跑輸出）

## 6.2 fmt / clippy / 測試

```
$ cargo fmt --check          # 乾淨
$ cargo clippy --workspace --all-targets   # 0 error（all + pedantic）
$ cargo test --workspace
passed: 180 failed: 0
```

## Spec 綁定測試清單

- `graphify-core/src/compose_manifest.rs`（15 tests）：三個 validate scenario（路徑不存在、relation 未宣告端點、節點引用不存在）+ 最小 manifest roundtrip + write 原子性兩 scenario（合法寫入可再載入、無效寫入位元組不變且暫存檔不殘留）
- `graphify-core/src/compose_merge.rs`（17 tests）：合併兩 workspace（前綴 + container）、跨 workspace 撞名去重、前綴格式、container 欄位、compose marker、relations 載入
- `graphify-mcp/src/main.rs`（4 compose tests）：read 結構、write 成功可再 read、write 失敗位元組不變、render 投影形狀（容器/成員/跨域 connection）
