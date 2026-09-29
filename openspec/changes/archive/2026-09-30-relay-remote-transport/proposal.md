# 變更提案：relay 遠端傳輸（graphify-mcp streamable HTTP/SSE service mode）

## 1. 背景與現狀問題（2026-09-30 實測全鏈路）

relay 在 NexusHub gateway 拓撲下**不可用**，實測證據鏈（E2E 驗證）：

```
opencode (serve-host) → https://nexushub.example
  = gateway caddy :443 (lb_policy first)
  → 192.0.2.11:8765  hypervisor LXC node1 「nexushub-node1」(primary，現役)
  → stdio spawn /opt/nexus/bin/graphify-mcp (v2.2.0, 2026-09-29 14:50 UTC build)
      cwd = /opt/nexus（nexus daemon 的工作目錄）
```

1. **caller workspace 物理不可達**：relay 工具的 `path`（relay-workspace-context
   已落地、per-call 綁定**機制本身正常**——在 node1 上
   `graphify_relay_init(path=/tmp/ws-new)` 實測成功寫入 relay.json + specs/）
   指向 serve-host 本機的 repo 路徑（`/mnt/data/btrfs-ssd/...`）。LXC node1 沒有這個
   路徑（`/mnt` 不存在）→ `relay_status` 回 NoRoot、`relay_init` 回
   `io: Permission denied`。**問題不是綁定機制，是「stdio child 跑在沒有
   workspace 的機器上」。**
2. **HA 三節點共享 stdio 上游**：node1 / node2 / serve-host 任一節點接手，stdio
   child 就 spawn 在該節點，relay 身份隨節點漂移；三節點各自的
   registry DB（XDG data `graphify.db`）也互不相通。
3. **錯誤訊息誤導**：path 在本機不存在時，status 回「Run relayInit first, or
   set GRAPHIFY_RELAY_ROOT」（是路徑不存在，不是未初始化）、init 回裸
   `io: Permission denied`。在 gateway 拓撲下第一次除錯花了整個 session 才定位。

## 2. 解決方案：graphify-mcp 增加遠端 service mode（SSE）

方向（定案 2026-09-30）：**stdio 模式重新規劃成 SSE**。新增 streamable HTTP
（SSE）server mode，讓 graphify-mcp 以 systemd user service 跑在 **workspace
所在機器**（serve-host），nexus 以既有 `type: http` upstream（code-review-graph /
teleflux / soundwave 同款模式，`CRG_BASE_URL=http://192.0.2.10:9877` 已是
先例）遠端接入。

- workspace（relay.json / specs/ / graphify.db registry）與服務同機 → per-call
  `path` 綁定天然成立，HA 節點漂移不再影響 relay 身份。
- stdio direct-spawn 模式**保留**（CLI / TUI / 單機場景不回歸），PROTOCOL.md
  明寫兩端差異。
