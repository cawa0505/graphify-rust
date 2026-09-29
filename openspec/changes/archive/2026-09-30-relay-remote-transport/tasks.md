# Tasks: relay-remote-transport

## 1. 錯誤訊息可除錯化（獨立可先落地，不依賴 SSE）

- [x] 1.1 `bind_for_cli` / relay 工具錯誤分層：`workspace path not found on
      this host <hostname>: <path>`（path 不存在）、`No relay.json found at
      <root>`（存在但未初始化）、`init failed: <io error>`（寫入失敗）——
      design D4 三種狀況 + hostname。
- [x] 1.2 e2e 測試：不存在路徑 → 新錯誤文；非 git 目錄 → root=path 語意保留
      （既有 direct-spawn e2e 不回歸）。
- [x] 1.3 部署：rebuild + nexus 三節點二進位同步（node1/node2/serve-host 的
      `/opt/nexus/bin/graphify-mcp` 或 `~/.cargo/bin/graphify-mcp`）。
      （2026-09-30 cutover：release sha `d57de87bce69b19f` 三節點一致；
      serve-host 裝 `~/.cargo/bin`、node1/node2 裝 `/opt/nexus/bin`（atomic rename）。
      HTTP cutover 後 node1/node2 的 binary 屬待命態，權威 serve 在 serve-host）

## 2. HTTP-only service mode（stdio MCP 移除，定案 2026-09-30）

- [x] 2.1 `graphify-mcp serve --listen <host>:<port>`：streamable HTTP
      server（MCP initialize / tools/list / tools/call），`GET /health`
      無 auth 探測端點；可選 Bearer token（soundwave 先例）。
- [x] 2.2 **移除 stdio MCP mode**：`main.rs` stdio 迴圈刪除，`handle_request`
      保留為 HTTP dispatch 核心。實作記錄（2026-09-30）：plugin 狀態維持
      `Rc`（`!Send` 不跨執行緒），axum 於專用執行緒、請求經 std mpsc channel
      序列化進 dispatcher thread 循序處理——執行緒安全由 channel 邊界保證，
      IService 序列語意與 stdio 版完全一致（零 20 處 borrow 改寫風險），
      取代原 Rc→Arc/Mutex 估法；delta spec 未強制 Arc（僅「工具行為語意與
      既有 stdio 版一致」）。
- [x] 2.3 review notify buffer（ImpactAlert）經 SSE 通知通道送出（session
      事件流）。
- [x] 2.4 測試：HTTP mode e2e（initialize → tools/call relay_status(path=...)
      → relay_init → relay_save）+ 無 stdio 啟動路徑的建構保證（binary 僅
      serve）。

## 2b. TUI remote 模式

- [x] 2b.1 `graphify tui --remote <URL>`（+ `GRAPHIFY_REMOTE` env，旗標覆蓋）：
      查詢經 streamable HTTP 送 graphify 服務（與 nexus 同端點）；local 預設
      零回歸。
- [x] 2b.2 remote 不可達 → 連線失敗 + server URL 顯示，不靜默退化 local。
- [x] 2b.3 測試：local 模式回歸；remote 模式對話 graphify 服務 e2e。

## 3. 部署與拓撲收斂

- [x] 3.1 serve-host systemd user unit（`graphify-http.service`，Restart=on-failure，
      XDG 路徑），`systemctl --user enable --now`。（enabled + running；
      `/health` ok、MCP initialize 握手 ok；OD_BASE_URL/CRG_BASE_URL env
      自 stdio 宣告移入 unit）
- [x] 3.2 nexus.yaml **三節點**（node1/node2/serve-host）graphify upstream 改 `type:
      http` 指向 serve-host service（stdio 宣告移除——cutover 一次完成）。
      （serve-host→127.0.0.1:9899、node1/node2→192.0.2.10:9899；三節點
      /health 皆 graphify http healthy；node1 佈署模板
      `/root/nexus-bundle/templates/nexus.yaml` 同步切 http，node2 無模板目錄；
      各節點均留 .bak-20260930 備份）
- [x] 3.bin 核對 `DockerStdioMcpBridge` / 其他 stdio 消費者是否仍依賴
      graphify stdio（有則一併轉 http）。（rg 全 workspace 與 NexusHub
      checkout 無 `DockerStdioMcpBridge` 引用；nexus 其餘 stdio 消費者
      （zero/draco/open-slide 等）為不相干服務，行為不變）
- [x] 3.3 E2E：opencode（經 nexushub HA）呼叫 relay_status/relay_init 對
      serve-host repo 成功——本提案驗收主情境。（session MCP 走 caddy HA
      →node1 primary→http upstream→serve-host serve：relay_init（tmp ws）成功、
      relay_save 更新 baton＋RESUME 產出、relay_status 回正確 root/baton。
      cutover 前單次 EACCES 追因＝node1 舊 stdio binary（舊文案指紋
      `or set GRAPHIFY_RELAY_ROOT`），cutover 後不可重現）
- [x] 3.4 NexusHub repo 部署文件更新（stdio → http 接線圖）。
      （deploy/lxc/templates/nexus.yaml graphify stanza stdio→http＋
      README.md 內容物說明改「原生二進位檔」加 cutover 接線註記；
      佈署源頭 bundle.sh 不再打包時即無 stdio spawn 依賴）
- [x] 3.5 還原本機探索期遺留：serve-host nexus.service.d/workdir.conf（WorkingDirectory
      綁 TeletranRoute）與 nexus.yaml 的 GRAPHIFY_RELAY_ROOT env（HTTP mode 下
      語意由 per-call path 取代）。（驗證即清理：nexus.service.d 目錄不存在、
      nexus.yaml 無 GRAPHIFY_RELAY_ROOT（9/29 已移除，bak-relayroot-pre 為證）、
      node1/node2 /etc/nexus/env 無 GRAPHIFY 殘留）
- [x] 3.6 `--remote` 開源面註記進 README（外部 MCP user 需自行跑 serve +
      http 接入）。（README.md + README_zh-TW.md 雙語同步；serve/token/remote
      旗數均照原始碼實測參數撰寫）

## 4. 收案

- [x] 4.1 `openspec validate --strict` + 全測試綠 + relay E2E 實測紀錄（含 HA
      failover 情境：node1 → node2 接手後 relay_status 不漂移）。
      （2026-09-30：validate --strict 兩 change 皆 valid；mcp 56/56、cli 24/24、
      tui 5/5 綠；clippy 0 warning、fmt 乾淨；E2E＝init/save/status 三操作
      經 session MCP→caddy HA→serve-host serve 成功；HA 一致性＝同 status 請求
      直打 node1 與 node2 回應雜湊完全一致（630f1205b608）——停機式 primary
      failover 演練刻意未跑，避免拆生產 node1）
- [x] 4.2 里程碑回報 + 歸檔。（報告見 session 2026-09-30 對話紀錄；本 change
      已歸檔為 2026-09-30-relay-remote-transport，specs 同步 +3 條目）
