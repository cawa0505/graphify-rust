# Tasks: TUI Compose Tab

## 1. Modal 基礎
- [x] 1.1 `ModalState::ComposePanel` 變體（manifests/hovered/selected/diagram/error/scroll）+ `title()`/`len()`/`items()` 更新
- [x] 1.2 `draw_compose_menu()`：manifest 列表 + hover + Enter 提示
- [x] 1.3 `draw_compose_diagram()`：手繪 ASCII 顯示 + 紅字錯誤 + 捲動

## 2. TUI 整合
- [x] 2.1 `scan_compose_manifests()`：cwd + `manifests/` 掃描 `*.yaml`
- [x] 2.2 `build_compose_diagram()`：load_manifest → build_unified_graph → render_compose_ascii
- [x] 2.3 `render_compose_ascii()`：box-drawing 容器 + composition 邊（成員上限 3 行）
- [x] 2.4 `open_compose_panel()` / `select_compose_manifest()` / `compose_scroll()` / `compose_back_to_menu()`
- [x] 2.5 鍵盤：`y` 開啟、menu j/k/Enter、diagram j/k 捲動、Esc 逐層返回
- [x] 2.6 滑鼠：menu 點選 manifest、diagram 滾輪捲動

## 3. 驗證
- [x] 3.1 單元測試：`test_split_ref_plain_and_qualified`、`test_render_compose_ascii_boxes_and_edges`
- [x] 3.2 端到端煙霧測試：臨時 manifest + 兩個 workspace graph → `compose validate/graph/render` 全通
- [x] 3.3 clippy 全 workspace 乾淨、cargo test 全綠、cargo fmt

## 4. 收尾
- [ ] 4.1 文件：docs/cli.md TUI 段落補 `y` 鍵說明
- [ ] 4.2 openspec archive（實作驗證證據附於 spec）
