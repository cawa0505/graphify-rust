# Design: relay-remote-transport

## Context

graphify-mcp 供 NexusHub 與 opencode 兩層呼叫。目前唯一傳輸是 stdio：
`nexus → spawn graphify-mcp (child, cwd=nexus daemon cwd)`。NexusHub HA
（gateway caddy `lb_policy first` → node1 primary / node2 standby1 / serve-host
standby2）讓 servicing 節點漂移，stdio child 與其 XDG registry 隨之漂移；
而 relay 的 per-call `path`（relay-workspace-context D1/D2）指向 caller 的
repo —— 只有 serve-host 有 `/mnt/data/btrfs-ssd/...`。在 node1/node2 上市 path
不存在 → NoRoot / `Permission denied`（E2E 實測，本提案 §1）。

nexus 已有遠端 http upstream 先例：`CRG_BASE_URL=http://192.0.2.10:9877/mcp`
（code-review-graph on serve-host）、teleflux、soundwave 全部 `type: http`。

## Decisions

### D1 — 傳輸：streamable HTTP 唯一，stdio MCP 移除（定案 2026-09-30）

`graphify-mcp` 改為純 server（`serve --listen <host>:<port>`），協定走 MCP
streamable HTTP（與 nexus 既有 http upstream 同型——CRG / soundwave / teleflux
三個先例）。**stdio MCP mode 移除**（定案 2026-09-30）：stdio 唯一消費者是
nexus，nexus 改 `type: http` 後 stdio 消費者歸零；移除後「stdio child spawn 在
沒有 workspace 的機器」這整類問題永久消失，`main.rs` stdio 迴圈整塊刪除。

- listener 綁**內網介面**（非僅 127.0.0.1）：client 有兩類——nexus（agent
  流量）與 TUI/CLI `--remote`（人類流量，見 D6），後者來自別台機器。可選
  Bearer token（soundwave 先例：nexus.yaml `headers.Authorization`）。
- 服務端 workspace 身份維持 per-call `path` 推導：一台服務可服務同機多個
  workspace，語意不變；服務與 workspace 同機後，caller 傳錯 host 路徑的
  問題自然消失。

### D2 — 服務位置：serve-host（workspace 所在機）

relay / opendoc / review 都以**檔案系統 workspace** 為身份來源（relay.json、
specs/、`.code-relay/`、repo 內 AST 圖譜快取）。服務必須跑在那些檔案的機器上。
serve-host 是所有 repo 的 host → serve-host 跑 `graphify-mcp serve`，由 systemd user
unit 管（`graphify-http.service`，`Restart=on-failure`）。nexus（serve-host 本機
那份）的 `nexus.yaml`：

```yaml
- name: graphify
  type: http
  namespace: graphify
  endpoint: http://127.0.0.1:9899/mcp
```

stdio 宣告在 HA 其他節點（node2/node1）維持現狀或移除——**裁決：待辦**（T6，
見 tasks），過渡期兩邊並存，靠 nexus 的 `lb_policy first` 健康檢查自然導流。

### D3 — relay 身份語意：caller path 仍是唯一真相，服務機 = workspace 機

per-call `bind_for_cli(caller_path)`（relay-workspace-context D1/D2）不變。
HTTP mode 下「服務所在機掛著 workspace」使 `path` 恆可解析；caller（opencode
session）也因此穩定——不再随 HA 節點漂移。registry DB 回歸
workspace-adjacent（serve-host XDG），修掉 stdio-on-node1 的 registry 錯位。

CLI direct-spawn 場景（本機 `graphify relay status` 等命令）語意不變。

### D4 — 錯誤訊息可除錯化（本次事故的直接教訓）

`bind_for_cli` 解析失敗時，回錯 SHALL 區分並列出具體原因，禁止再用模稜兩可的
NoRoot 文案（現狀：path 不存在 → 「No relay.json found ... Run relayInit
first」，誤導成未初始化）：

- path 不存在 / 不目錄：`workspace path not found on this host: <p>`（+
  host 提示 `hostname` —— gateway 拓撲下 caller 看不出服務在哪台跑，本次
  除錯最大的摩擦點）
- path 存在但無 relay.json 且這是 status/save/close：`No relay.json found at
  <root> — run relayInit first`（保留現文案，但帶實際 root）
- init 寫入 IO 失敗：`init failed: <io error>`（不再裸 `io: Permission
  denied`）

### D6 — TUI/CLI remote 模式（定案 2026-09-30）

`graphify tui` 增加 `--remote <URL>` 旗標（`GRAPHIFY_REMOTE` env 為預設值，
旗標覆蓋 env）：remote 模式經同一 streamable HTTP 端點查詢（與 nexus 共用
同一份狀態）；local 模式（預設）維持直讀本機 `.toon`，workspace host 上零
變更。remote 不可達時顯示連線失敗 + server URL，不靜默退化。

### D5 — 部署面：caddy conf 源頭單一化

`NexusHub repo deploy/caddy/` 已是 caddy conf 的 source of truth（ Ubuntu
conf.d 註解自述）。本變更**不動** caddy（HA 入口不變）：變更發生在 nexus
upstream 層（nexus.yaml `type: http`）與 serve-host 服務層（systemd unit）。部署
 playbook 寫進 tasks T7。

## Risks / Trade-offs

- **SSE 服務是常駐行程**（stdio 是 per-session spawn）：常駐 + `GRAPHIFY_...`
  全域圖譜狀態需要一次 re-entrancy 審視（`Arc`/lock 化，§T3）；好處是圖譜
  index 常駐熱存，tool 延遲下降。
- **nexus http upstream 健康檢查**：caddy health 探的是 nexus :8765/health，
  nexus 對 http upstream 有自己的 admission（plugin-health-admission 已歸檔
  的機制）。graphify-http 掛掉時 nexus 需正確回 unhealthy 而非 500 —— 驗收
  項入 T8。
- **打破既有 caller（刻意）**：stdio mode 移除是一次性 breaking——nexus 三節點
  graphify 宣告改 `type: http`，這是部署步驟（T3）；工具 schema 本身不變。
- **開源面**：stdio 是外部 MCP client 的最通用 transport；移除後外部使用者
  需跑 `graphify-mcp serve` 並以 http URL 接入。2026 年主流 client 皆支援
  remote MCP，接受此摩擦（定案 2026-09-30）。
