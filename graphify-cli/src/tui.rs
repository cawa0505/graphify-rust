#![allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::option_if_let_else,
    clippy::manual_range_contains,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::doc_lazy_continuation,
    clippy::unnested_or_patterns
)]

use crate::ui::{ActiveTab, layout, modal, theme};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use graphify_core::{GraphOutput, Node};
use graphify_registry::db::{PluginRegistrationRow, PluginStatus, RegistryDb, WorkspaceRow};
use graphify_registry::registry_db_path;
use layout::{ActionTag, Flash, LogEntry};
use modal::{ModalItem, ModalState};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
};
use std::{
    io,
    process::Command,
    time::{Duration, Instant},
};

/// Tab 標題：繪製與點擊命中測試共用同一來源，避免偏移漂移
const TAB_TITLES: [&str; 3] = [
    " 🔍 Explorer (1) ",
    " 📊 Visual Graph (2) ",
    " 📡 Monitor (3) ",
];

pub struct App {
    pub graph: GraphOutput,
    pub filtered_nodes: Vec<usize>,
    pub list_state: ListState,
    pub search_query: String,
    pub input_mode: bool,

    // TABS & MODALS
    pub active_tab: ActiveTab,
    pub modal_state: ModalState,
    pub modal_hover: Option<usize>,
    pub last_modal_list_area: Option<Rect>,

    // CANVAS VIEWPORT CONTROLS
    pub canvas_coords: crate::ui::canvas::NodeCoordinates,
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,

    // DYNAMIC LAYOUT AREA TRACKING
    pub last_canvas_area: Option<Rect>,
    pub last_tabs_area: Option<Rect>,
    pub last_list_area: Option<Rect>,
    pub drag_start: Option<(u16, u16)>,

    // EVENT LOG & FOOTER FLASH
    pub show_event_log: bool,
    pub event_log: Vec<LogEntry>,
    pub log_state: ListState,
    pub flash: Flash,
    pub last_log: Option<Instant>,

    // MONITOR TAB
    pub workspaces: Vec<WorkspaceRow>,
    pub plugins: Vec<PluginRegistrationRow>,
    pub monitor_ws_idx: usize,
    pub monitor_plugin_idx: usize,
}

impl App {
    #[must_use]
    pub fn new(graph: GraphOutput) -> Self {
        let mut list_state = ListState::default();
        let count = graph.nodes.len();
        if count > 0 {
            list_state.select(Some(0));
        }
        let filtered_nodes = (0..count).collect();

        let initial_selected = graph.nodes.first().map(|n| &n.id);
        let canvas_coords = crate::ui::canvas::NodeCoordinates::compute(&graph, initial_selected);

        // ponytail: load workspaces/plugins once at startup, [R] refreshes
        let (workspaces, plugins) = load_monitor_data();

        Self {
            graph,
            filtered_nodes,
            list_state,
            search_query: String::new(),
            input_mode: false,
            active_tab: ActiveTab::Explorer,
            modal_state: ModalState::None,
            modal_hover: None,
            last_modal_list_area: None,
            canvas_coords,
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            last_canvas_area: None,
            last_tabs_area: None,
            last_list_area: None,
            drag_start: None,
            show_event_log: false,
            event_log: Vec::new(),
            log_state: ListState::default(),
            flash: Flash::default(),
            last_log: None,
            workspaces,
            plugins,
            monitor_ws_idx: 0,
            monitor_plugin_idx: 0,
        }
    }

    /// 追加事件日誌並自動滾動至最新一筆
    fn log(&mut self, text: impl Into<String>, fg: Color) {
        let entry = LogEntry::new(text, fg);
        layout::push_log(&mut self.event_log, entry);
        self.log_state
            .select(Some(self.event_log.len().saturating_sub(1)));
    }

    /// 高頻事件 (平移/縮放) 節流紀錄，避免日誌被刷爆
    fn log_throttled(&mut self, text: impl Into<String>, fg: Color) {
        if self
            .last_log
            .is_some_and(|t| t.elapsed() < Duration::from_millis(120))
        {
            return;
        }
        self.last_log = Some(Instant::now());
        self.log(text, fg);
    }

    fn open_bfs_modal(&mut self) {
        let trace = self.compute_bfs_trace();
        let mut items = Vec::with_capacity(trace.len() + 1);
        items.push(ModalItem::new(
            "── BFS Path Steps (depth ≤ 3) ──",
            theme::MAUVE,
        ));
        if trace.is_empty() {
            items.push(ModalItem::new(
                "No call path found from this node.",
                theme::SUBTLE,
            ));
        } else {
            for (i, step) in trace.iter().enumerate() {
                items.push(ModalItem::new(
                    format!(" {}. {}", i + 1, step),
                    theme::GREEN,
                ));
            }
        }
        self.modal_state = ModalState::BfsTrace(items);
        self.modal_hover = Some(0);
        self.flash.trigger(ActionTag::Trace);
        self.log("Keyboard: 't' → BFS Trace modal", theme::GOLD);
    }

    fn open_relations_modal(&mut self) {
        let items = self.build_relations_items();
        let label = self
            .selected_node()
            .map(|n| n.label.clone())
            .unwrap_or_default();
        self.modal_state = ModalState::Relations(items);
        self.modal_hover = Some(0);
        self.flash.trigger(ActionTag::Inspect);
        self.log(format!("Right-Click: Inspect '{label}'"), theme::GOLD);
    }

    /// 從選中節點建構 Outgoing / Incoming 關係列表
    fn build_relations_items(&self) -> Vec<ModalItem> {
        let mut items = Vec::new();
        let Some(node) = self.selected_node() else {
            items.push(ModalItem::new("No node selected.", theme::SUBTLE));
            return items;
        };

        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();
        for edge in &self.graph.edges {
            if edge.source == node.id {
                outgoing.push(edge);
            } else if edge.target == node.id {
                incoming.push(edge);
            }
        }

        if outgoing.is_empty() {
            items.push(ModalItem::new("── Outgoing Calls: none ──", theme::SUBTLE));
        } else {
            items.push(ModalItem::new(
                format!("── Outgoing Calls / References ({}) ──", outgoing.len()),
                theme::GOLD,
            ));
            for edge in outgoing {
                items.push(ModalItem::new(
                    format!(" ➔ {} ({})", edge.target.0, edge.relation),
                    theme::TEXT,
                ));
            }
        }

        if incoming.is_empty() {
            items.push(ModalItem::new("── Incoming Calls: none ──", theme::SUBTLE));
        } else {
            items.push(ModalItem::new(
                format!("── Incoming Calls / References ({}) ──", incoming.len()),
                theme::MAUVE,
            ));
            for edge in incoming {
                items.push(ModalItem::new(
                    format!(" ⇠ {} ({})", edge.source.0, edge.relation),
                    theme::BLUE,
                ));
            }
        }
        items
    }

    fn modal_next(&mut self) {
        let len = self.modal_state.len();
        if len == 0 {
            return;
        }
        self.modal_hover = Some(
            self.modal_hover
                .map_or(0, |h| if h + 1 >= len { 0 } else { h + 1 }),
        );
        self.flash.trigger(ActionTag::Nav);
    }

    fn modal_prev(&mut self) {
        let len = self.modal_state.len();
        if len == 0 {
            return;
        }
        self.modal_hover = Some(
            self.modal_hover
                .map_or(0, |h| if h == 0 { len - 1 } else { h - 1 }),
        );
        self.flash.trigger(ActionTag::Nav);
    }

    fn close_modal(&mut self) {
        self.modal_state = ModalState::None;
        self.modal_hover = None;
        self.last_modal_list_area = None;
    }

    /// 畫布座標命中測試：回傳最接近節點的 graph index (6.0*zoom 半徑內)
    fn hit_test_canvas(&self, click_col: u16, click_row: u16) -> Option<usize> {
        let canvas_area = self.last_canvas_area?;
        if click_row < canvas_area.y || click_row >= canvas_area.y + canvas_area.height {
            return None;
        }
        if click_col < canvas_area.x || click_col >= canvas_area.x + canvas_area.width {
            return None;
        }

        let w_ratio = f64::from(click_col - canvas_area.x) / f64::from(canvas_area.width);
        let h_ratio = f64::from(click_row - canvas_area.y) / f64::from(canvas_area.height);

        let x_min = -60.0 * self.zoom + self.pan_x;
        let x_max = 60.0 * self.zoom + self.pan_x;
        let y_min = -30.0 * self.zoom + self.pan_y;
        let y_max = 30.0 * self.zoom + self.pan_y;

        let clicked_x = x_min + w_ratio * (x_max - x_min);
        let clicked_y = y_max - h_ratio * (y_max - y_min);

        let mut closest_node_idx = None;
        let mut min_dist = 6.0 * self.zoom;

        for &idx in &self.filtered_nodes {
            if let Some(node) = self.graph.nodes.get(idx) {
                if let Some(&(nx, ny)) = self.canvas_coords.coords.get(&node.id) {
                    let dist = (clicked_x - nx).hypot(clicked_y - ny);
                    if dist < min_dist {
                        min_dist = dist;
                        closest_node_idx = Some(idx);
                    }
                }
            }
        }
        closest_node_idx
    }

    /// Modal 開啟時，依據滑鼠位置更新懸停項目
    fn update_modal_hover(&mut self, row: u16, col: u16) {
        if matches!(self.modal_state, ModalState::None) {
            return;
        }
        match self.last_modal_list_area {
            Some(r) if row >= r.y && row < r.y + r.height && col >= r.x && col < r.x + r.width => {
                let idx = (row - r.y) as usize;
                self.modal_hover = if idx < self.modal_state.len() {
                    Some(idx)
                } else {
                    None
                };
            }
            _ => self.modal_hover = None,
        }
    }

    pub fn filter_nodes(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            self.filtered_nodes = (0..self.graph.nodes.len()).collect();
        } else {
            self.filtered_nodes = self
                .graph
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| {
                    node.label.to_lowercase().contains(&query)
                        || node.id.0.to_lowercase().contains(&query)
                })
                .map(|(idx, _)| idx)
                .collect();
        }

        if self.filtered_nodes.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
        self.update_selected_coords();
    }

    pub fn update_selected_coords(&mut self) {
        if let Some(node) = self.selected_node() {
            self.canvas_coords =
                crate::ui::canvas::NodeCoordinates::compute(&self.graph, Some(&node.id));
        }
    }

    pub fn next(&mut self) {
        if self.filtered_nodes.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered_nodes.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.update_selected_coords();
    }

    pub fn previous(&mut self) {
        if self.filtered_nodes.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_nodes.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.update_selected_coords();
    }

    #[must_use]
    pub fn selected_node(&self) -> Option<&Node> {
        self.list_state
            .selected()
            .and_then(|idx| self.filtered_nodes.get(idx))
            .and_then(|&node_idx| self.graph.nodes.get(node_idx))
    }

    /// 計算目前選中節點的 3 層 BFS 追蹤鏈
    #[must_use]
    pub fn compute_bfs_trace(&self) -> Vec<String> {
        if let Some(start_node) = self.selected_node() {
            if let Ok((graph, node_map)) =
                graphify_core::build_graph(&self.graph.nodes, &self.graph.edges)
            {
                if let Ok(sub_graph) =
                    graphify_core::query_bfs(&graph, &node_map, &start_node.id, 3)
                {
                    let mut trace = sub_graph
                        .nodes
                        .iter()
                        .map(|n| format!("{}:{}", n.kind, n.label))
                        .collect::<Vec<String>>();
                    trace.truncate(6);
                    return trace;
                }
            }
        }
        Vec::new()
    }

    // ── monitor tab helpers ──

    fn monitor_down(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }
        // 目前聚焦在 workspace 欄還是 plugin 欄？
        // 如果 plugin 欄有選中項目且非最後一個，先移動 plugin 選取
        // 否則跳到下一個 workspace
        if !self.plugins.is_empty() && self.monitor_plugin_idx + 1 < self.plugins.len() {
            self.monitor_plugin_idx += 1;
        } else if self.monitor_ws_idx + 1 < self.workspaces.len() {
            self.monitor_ws_idx += 1;
            self.monitor_plugin_idx = 0;
            self.load_plugins_for_selected_ws();
        } else {
            // 到底了，回到開頭
            self.monitor_ws_idx = 0;
            self.monitor_plugin_idx = 0;
            self.load_plugins_for_selected_ws();
        }
    }

    fn monitor_up(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }
        if self.monitor_plugin_idx > 0 {
            self.monitor_plugin_idx -= 1;
        } else if self.monitor_ws_idx > 0 {
            self.monitor_ws_idx -= 1;
            self.monitor_plugin_idx = 0;
            self.load_plugins_for_selected_ws();
        } else {
            // 到頂了，跳到底部
            self.monitor_ws_idx = self.workspaces.len() - 1;
            self.monitor_plugin_idx = 0;
            self.load_plugins_for_selected_ws();
        }
    }

    fn load_plugins_for_selected_ws(&mut self) {
        if let Some(ws) = self.workspaces.get(self.monitor_ws_idx) {
            self.plugins = RegistryDb::open(&registry_db_path())
                .ok()
                .and_then(|db| db.list_registrations(&ws.workspace_key).ok())
                .unwrap_or_default();
        } else {
            self.plugins.clear();
        }
    }

    fn refresh_monitor(&mut self) {
        let (ws, plugins) = load_monitor_data();
        self.workspaces = ws;
        self.plugins = plugins;
        self.monitor_ws_idx = self.monitor_ws_idx.min(self.workspaces.len().saturating_sub(1));
        self.monitor_plugin_idx = 0;
    }

    fn reset_selected_plugin(&mut self) {
        let Some(ws) = self.workspaces.get(self.monitor_ws_idx) else {
            return;
        };
        let Some(reg) = self.plugins.get(self.monitor_plugin_idx) else {
            return;
        };
        if let Ok(db) = RegistryDb::open(&registry_db_path()) {
            let _ = db.set_status(&reg.plugin_id, &ws.workspace_key, PluginStatus::Unavailable);
            // 重新載入 plugins 列表
            self.plugins = db
                .list_registrations(&ws.workspace_key)
                .ok()
                .unwrap_or_default();
        }
    }
}

/// 從 registry 載入 workspaces + plugins 清單，失敗時回傳空資訊
/// ponytail: 單次載入，[R] 重新整理
fn load_monitor_data() -> (Vec<WorkspaceRow>, Vec<PluginRegistrationRow>) {
    let db = RegistryDb::open(&registry_db_path()).ok();
    let workspaces = db
        .as_ref()
        .and_then(|d| d.list_workspaces().ok())
        .unwrap_or_default();
    // 如果有 active workspace，預載其 plugins
    let ws_key = workspaces
        .iter()
        .find(|w| w.is_active)
        .map(|w| &w.workspace_key);
    let plugins = match (db.as_ref(), ws_key) {
        (Some(d), Some(k)) => d.list_registrations(k).ok().unwrap_or_default(),
        _ => Vec::new(),
    };
    (workspaces, plugins)
}

pub fn run_tui(graph: GraphOutput) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 建立 Panic Hook，確保崩潰時自動還原終端機狀態
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(out, LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    let mut app = App::new(graph);
    let res = run_loop(&mut terminal, &mut app);

    // 還原終端機
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("\x1B[1;31mError running TUI: {err}\x1B[0m");
    }

    Ok(())
}

fn run_loop<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        // 16ms 輪詢 = 60 FPS 渲染節奏
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    if handle_key(terminal, app, key)? {
                        return Ok(());
                    }
                }
                Event::Mouse(mouse) => handle_mouse(app, mouse),
                _ => {}
            }
        }
    }
}

/// 回傳 `true` 表示要求退出
#[allow(clippy::too_many_lines)]
fn handle_key<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    // 若當前有開啟 Modal，僅接受關閉/導航按鍵
    if !matches!(app.modal_state, ModalState::None) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') => {
                app.close_modal();
                app.log("Modal closed", theme::SUBTLE);
            }
            KeyCode::Char('j') | KeyCode::Down => app.modal_next(),
            KeyCode::Char('k') | KeyCode::Up => app.modal_prev(),
            _ => {}
        }
        return Ok(false);
    }

    if app.input_mode {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                app.input_mode = false;
                app.log("Filter locked", theme::CYAN);
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
                app.filter_nodes();
            }
            KeyCode::Backspace => {
                app.search_query.pop();
                app.filter_nodes();
            }
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') => {
            app.flash.trigger(ActionTag::Quit);
            app.log("Keyboard: 'q' Quit", theme::RED);
            return Ok(true);
        }
        // 切換分頁
        KeyCode::Tab => {
            app.active_tab = match app.active_tab {
                ActiveTab::Explorer => ActiveTab::VisualGraph,
                ActiveTab::VisualGraph => ActiveTab::Monitor,
                ActiveTab::Monitor => ActiveTab::Explorer,
            };
            app.flash.trigger(ActionTag::Nav);
            app.log("Keyboard: [Tab] switch view", theme::CYAN);
        }
        KeyCode::Char('1') => {
            app.active_tab = ActiveTab::Explorer;
            app.flash.trigger(ActionTag::Nav);
            app.log("Keyboard: '1' → Explorer", theme::CYAN);
        }
        KeyCode::Char('2') => {
            app.active_tab = ActiveTab::VisualGraph;
            app.flash.trigger(ActionTag::Nav);
            app.log("Keyboard: '2' → Visual Graph", theme::CYAN);
        }
        KeyCode::Char('3') => {
            app.active_tab = ActiveTab::Monitor;
            app.flash.trigger(ActionTag::Nav);
            app.log("Keyboard: '3' → Monitor", theme::CYAN);
        }
        // 觸發 BFS 追蹤鏈 Modal
        KeyCode::Char('t') | KeyCode::Char('T') => app.open_bfs_modal(),
        KeyCode::Char('j') | KeyCode::Down => {
            if app.active_tab == ActiveTab::Explorer {
                app.next();
                app.flash.trigger(ActionTag::Nav);
                app.log("Keyboard: 'j' navigate down", theme::CYAN);
            } else if app.active_tab == ActiveTab::Monitor {
                app.monitor_down();
                app.flash.trigger(ActionTag::Nav);
                app.log("Keyboard: 'j' navigate down", theme::CYAN);
            } else {
                // Visual Graph 平移向下
                app.pan_y -= 2.0 * app.zoom;
                app.flash.trigger(ActionTag::Pan);
                app.log_throttled("Keyboard: Pan down", theme::GREEN);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.active_tab == ActiveTab::Explorer {
                app.previous();
                app.flash.trigger(ActionTag::Nav);
                app.log("Keyboard: 'k' navigate up", theme::CYAN);
            } else if app.active_tab == ActiveTab::Monitor {
                app.monitor_up();
                app.flash.trigger(ActionTag::Nav);
                app.log("Keyboard: 'k' navigate up", theme::CYAN);
            } else {
                // Visual Graph 平移向上
                app.pan_y += 2.0 * app.zoom;
                app.flash.trigger(ActionTag::Pan);
                app.log_throttled("Keyboard: Pan up", theme::GREEN);
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if app.active_tab == ActiveTab::VisualGraph {
                app.pan_x -= 4.0 * app.zoom;
                app.flash.trigger(ActionTag::Pan);
                app.log_throttled("Keyboard: Pan left", theme::GREEN);
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if app.active_tab == ActiveTab::VisualGraph {
                app.pan_x += 4.0 * app.zoom;
                app.flash.trigger(ActionTag::Pan);
                app.log_throttled("Keyboard: Pan right", theme::GREEN);
            }
        }
        // 縮放與重設
        KeyCode::Char('+') | KeyCode::Char('=') => {
            if app.active_tab == ActiveTab::VisualGraph {
                app.zoom = (app.zoom - 0.1).max(0.2);
                app.flash.trigger(ActionTag::Zoom);
                app.log_throttled("Keyboard: Zoom In", theme::CYAN);
            }
        }
        KeyCode::Char('-') => {
            if app.active_tab == ActiveTab::VisualGraph {
                app.zoom = (app.zoom + 0.1).min(3.0);
                app.flash.trigger(ActionTag::Zoom);
                app.log_throttled("Keyboard: Zoom Out", theme::CYAN);
            }
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            if app.active_tab == ActiveTab::VisualGraph {
                app.pan_x = 0.0;
                app.pan_y = 0.0;
                app.zoom = 1.0;
                app.flash.trigger(ActionTag::Reset);
                app.log("Keyboard: 'r' reset view", theme::CYAN);
            } else if app.active_tab == ActiveTab::Monitor {
                // 小寫 r: reset plugin；大寫 R: refresh 全部
                if key.code == KeyCode::Char('R') {
                    app.refresh_monitor();
                    app.flash.trigger(ActionTag::Refresh);
                    app.log("Keyboard: 'R' refresh monitor", theme::GREEN);
                } else {
                    app.reset_selected_plugin();
                    app.flash.trigger(ActionTag::Reset);
                    app.log("Keyboard: 'r' reset plugin", theme::GOLD);
                }
            }
        }
        KeyCode::Char('e') | KeyCode::Char('E') => {
            app.show_event_log = !app.show_event_log;
            app.flash.trigger(ActionTag::Log);
            let state = if app.show_event_log {
                "shown"
            } else {
                "hidden"
            };
            app.log(format!("Keyboard: 'e' → Event Log {state}"), theme::MAUVE);
        }
        KeyCode::Char('/') => {
            if app.active_tab == ActiveTab::Explorer {
                app.input_mode = true;
                app.flash.trigger(ActionTag::Filter);
                app.log("Keyboard: '/' filter mode", theme::GOLD);
            }
        }
        KeyCode::Char('c') | KeyCode::Esc => {
            if !app.search_query.is_empty() {
                app.search_query.clear();
                app.filter_nodes();
                app.log("Search cleared", theme::SUBTLE);
            }
        }
        KeyCode::Char('g') | KeyCode::Enter => {
            if let Some(node) = app.selected_node() {
                let file_path = node.source_file.clone();
                let line = node.start_line;
                app.flash.trigger(ActionTag::Edit);
                app.log(
                    format!("Keyboard: 'g' open editor → {file_path}"),
                    theme::GREEN,
                );

                // 還原終端機以啟用系統預設編輯器
                disable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
                terminal.show_cursor()?;

                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let mut cmd = Command::new(&editor);
                if editor.contains("vi") || editor.contains("nvim") {
                    cmd.arg(format!("+{line}"));
                }
                let _ = cmd.arg(&file_path).status();

                // 重新載入 TUI
                enable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    EnterAlternateScreen,
                    EnableMouseCapture
                )?;
                terminal.clear()?;
            }
        }
        _ => {}
    }
    Ok(false)
}

#[allow(clippy::too_many_lines)]
fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let click_col = mouse.column;
    let click_row = mouse.row;

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if app.active_tab == ActiveTab::VisualGraph {
                app.zoom = (app.zoom - 0.05).max(0.1); // ponytail: 滾輪向上放大（投射邊界縮小 = Zoom In）
                app.flash.trigger(ActionTag::Zoom);
                app.log_throttled("Mouse Scroll: Zoom In", theme::CYAN);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.active_tab == ActiveTab::VisualGraph {
                app.zoom = (app.zoom + 0.05).min(5.0); // ponytail: 滾輪向下縮小（投射邊界擴大 = Zoom Out）
                app.flash.trigger(ActionTag::Zoom);
                app.log_throttled("Mouse Scroll: Zoom Out", theme::CYAN);
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // 1. 記錄拖曳起點
            if app.active_tab == ActiveTab::VisualGraph {
                app.drag_start = Some((click_col, click_row));
            }

            // 2. 偵測點擊 Tab 切換：以實際標題文字寬度逐字元命中測試
            //    (Tabs widget 左對齊、標題連續排列，不可用半寬切分)
            if let Some(tabs_area) = app.last_tabs_area {
                if click_row == tabs_area.y + 1 {
                    let mut col = usize::from(tabs_area.x) + 1; // 內側起點：邊框佔 1 列
                    let mut clicked: Option<ActiveTab> = None;
                    for (i, title) in TAB_TITLES.iter().enumerate() {
                        let w = ratatui::text::Line::from(*title).width();
                        if (col..col + w).contains(&usize::from(click_col)) {
                            clicked = Some(match i {
                                0 => ActiveTab::Explorer,
                                1 => ActiveTab::VisualGraph,
                                _ => ActiveTab::Monitor,
                            });
                            break;
                        }
                        col += w;
                    }
                    if let Some(tab) = clicked {
                        let tab_name = match tab {
                            ActiveTab::Explorer => "Explorer",
                            ActiveTab::VisualGraph => "VisualGraph",
                            ActiveTab::Monitor => "Monitor",
                        };
                        app.active_tab = tab;
                        app.log(
                            format!("Mouse Click: x={click_col}, y={click_row} [Tab: {tab_name}]"),
                            theme::CYAN,
                        );
                    }
                }
            }

            // 3. 偵測點擊 Explorer 節點列表 (動態區域)
            if app.active_tab == ActiveTab::Explorer {
                if let Some(list_area) = app.last_list_area {
                    let list_top = list_area.y + 1;
                    let list_bottom = list_area.y + list_area.height - 1;
                    if click_row >= list_top
                        && click_row < list_bottom
                        && click_col >= list_area.x
                        && click_col < list_area.x + list_area.width
                    {
                        let clicked_idx = (click_row - list_top) as usize;
                        if clicked_idx < app.filtered_nodes.len() {
                            let label = app
                                .filtered_nodes
                                .get(clicked_idx)
                                .and_then(|&i| app.graph.nodes.get(i))
                                .map(|n| n.label.clone())
                                .unwrap_or_default();
                            app.list_state.select(Some(clicked_idx));
                            app.update_selected_coords();
                            app.flash.trigger(ActionTag::Nav);
                            app.log(
                                format!(
                                    "Mouse Click: x={click_col}, y={click_row} [Node: {label}]"
                                ),
                                theme::CYAN,
                            );
                        }
                    }
                }
            }

            // 4. 偵測點擊 Visual Graph 節點選擇 (動態畫布)
            if app.active_tab == ActiveTab::VisualGraph {
                match app.hit_test_canvas(click_col, click_row) {
                    Some(target_idx) => {
                        if let Some(list_pos) =
                            app.filtered_nodes.iter().position(|&n| n == target_idx)
                        {
                            app.list_state.select(Some(list_pos));
                            app.update_selected_coords();
                            app.flash.trigger(ActionTag::Select);
                            let label = app
                                .graph
                                .nodes
                                .get(target_idx)
                                .map(|n| n.label.clone())
                                .unwrap_or_default();
                            app.log(
                                format!(
                                    "Mouse Click: x={click_col}, y={click_row} [Node: {label}]"
                                ),
                                theme::GREEN,
                            );
                        }
                    }
                    None => {
                        app.log(
                            format!(
                                "Mouse Click: x={click_col}, y={click_row} [Canvas: empty space]"
                            ),
                            theme::SUBTLE,
                        );
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.active_tab == ActiveTab::VisualGraph {
                if let Some((start_col, start_row)) = app.drag_start {
                    let dc = f64::from(click_col) - f64::from(start_col);
                    let dr = f64::from(click_row) - f64::from(start_row);

                    // 平曳視角
                    let scale = 0.5 * app.zoom;
                    app.pan_x -= dc * scale;
                    app.pan_y += dr * scale;

                    app.drag_start = Some((click_col, click_row));
                    app.flash.trigger(ActionTag::Pan);
                    app.log_throttled(format!("Mouse Pan: dx={dc:.0}, dy={dr:.0}"), theme::GREEN);
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.drag_start = None;
        }
        MouseEventKind::Down(MouseButton::Right) | MouseEventKind::Up(MouseButton::Right) => {
            // 右鍵點擊：快速聚焦並開啟關係檢查器 Modal
            if app.active_tab == ActiveTab::Explorer {
                if let Some(list_area) = app.last_list_area {
                    let list_top = list_area.y + 1;
                    let list_bottom = list_area.y + list_area.height - 1;
                    if click_row >= list_top
                        && click_row < list_bottom
                        && click_col >= list_area.x
                        && click_col < list_area.x + list_area.width
                    {
                        let clicked_idx = (click_row - list_top) as usize;
                        if clicked_idx < app.filtered_nodes.len() {
                            app.list_state.select(Some(clicked_idx));
                            app.update_selected_coords();
                            app.open_relations_modal();
                        }
                    }
                }
            } else if app.active_tab == ActiveTab::VisualGraph {
                if let Some(target_idx) = app.hit_test_canvas(click_col, click_row) {
                    if let Some(list_pos) = app.filtered_nodes.iter().position(|&n| n == target_idx)
                    {
                        app.list_state.select(Some(list_pos));
                        app.update_selected_coords();
                        app.open_relations_modal();
                    }
                }
            }
        }
        MouseEventKind::Moved => app.update_modal_hover(click_row, click_col),
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
fn draw_ui(f: &mut ratatui::Frame, app: &mut App) {
    let chrome = layout::split(f.area(), app.show_event_log);

    // 1. Tab 列導航
    let active_idx = match app.active_tab {
        ActiveTab::Explorer => 0,
        ActiveTab::VisualGraph => 1,
        ActiveTab::Monitor => 2,
    };
    let tabs = Tabs::new(TAB_TITLES)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::SURFACE_HI))
                .title(Line::from(Span::styled(
                    " 🧭 Graphify TUI Navigation ",
                    Style::default()
                        .fg(theme::MAUVE)
                        .add_modifier(Modifier::BOLD),
                ))),
        )
        .select(active_idx)
        .style(Style::default().fg(theme::SUBTLE))
        .highlight_style(
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chrome.tabs);
    app.last_tabs_area = Some(chrome.tabs);

    // 2. 主視圖面板 (70% or 100%)
    match app.active_tab {
        ActiveTab::Explorer => draw_explorer(f, app, chrome.main),
        ActiveTab::VisualGraph => draw_visual_graph(f, app, chrome.main),
        ActiveTab::Monitor => draw_monitor(f, app, chrome.main),
    }

    // 3. 事件日誌面板 (30%) — 'e' 隱藏時主視圖佔滿 100%
    if app.show_event_log {
        layout::render_event_log(f, &app.event_log, &mut app.log_state, chrome.log);
    }

    // 4. 快捷鍵 Footer (藥丸按鈕)
    layout::render_footer(f, app.active_tab, &app.flash, chrome.footer);

    // 5. 浮動 Modal 疊加 (Clear + 亮紫邊框)
    app.last_modal_list_area = modal::draw_modal(f, &app.modal_state, app.modal_hover, f.area());
    app.flash.tick();
}

fn draw_visual_graph(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let selected_id = app.selected_node().map(|n| &n.id);
    let canvas_widget = crate::ui::canvas::create_canvas_widget(
        &app.graph,
        &app.canvas_coords,
        selected_id,
        app.pan_x,
        app.pan_y,
        app.zoom,
    );
    f.render_widget(canvas_widget, area);
    app.last_canvas_area = Some(area);
    crate::ui::canvas::render_metrics_overlay(
        f,
        &app.graph,
        app.selected_node(),
        app.zoom,
        app.pan_x,
        app.pan_y,
        area,
    );
}

#[allow(clippy::too_many_lines)]
fn draw_monitor(f: &mut ratatui::Frame, app: &App, area: Rect) {
    // 左半邊：workspace 列表，右半邊：plugin 列表
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // ── workspace list ──
    let ws_items: Vec<ListItem> = app
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let active_mark = if ws.is_active { " ◉ " } else { " ○ " };
            let style = if i == app.monitor_ws_idx {
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };
            let plugin_count = if ws.is_active {
                format!("  plugins: {}", app.plugins.len())
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::styled(active_mark, style),
                Span::styled(&ws.root_path, style),
                Span::styled(plugin_count, Style::default().fg(theme::SUBTLE)),
            ]))
        })
        .collect();
    let ws_list = List::new(ws_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::SURFACE_HI))
            .title(Line::from(Span::styled(
                " 📁 Workspaces ",
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::BOLD),
            ))),
    );
    f.render_widget(ws_list, chunks[0]);

    // ── plugin list for selected workspace ──
    let plugin_items: Vec<ListItem> = if app.plugins.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No plugins registered.",
            Style::default().fg(theme::SUBTLE),
        )))]
    } else {
        app.plugins
            .iter()
            .enumerate()
            .map(|(i, reg)| {
                let (icon, color) = match reg.status {
                    PluginStatus::Healthy => ("●", theme::GREEN),
                    PluginStatus::Degraded => ("◐", theme::GOLD),
                    PluginStatus::Unavailable => ("○", theme::SUBTLE),
                    PluginStatus::Quarantined => ("⊘", theme::RED),
                };
                let selected = i == app.monitor_plugin_idx;
                let name_style = if selected {
                    Style::default()
                        .fg(theme::CYAN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                let last_synced = if reg.last_synced_at > 0 {
                    format!("last: {}", reg.last_synced_at)
                } else {
                    "last: ──".to_string()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {icon} "), Style::default().fg(color)),
                    Span::styled(&reg.plugin_id, name_style),
                    Span::raw("  "),
                    Span::styled(last_synced, Style::default().fg(theme::SUBTLE)),
                ]))
            })
            .collect()
    };
    let plugin_list = List::new(plugin_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::SURFACE_HI))
            .title(Line::from(Span::styled(
                " 🔌 Plugins ",
                Style::default()
                    .fg(theme::MAUVE)
                    .add_modifier(Modifier::BOLD),
            ))),
    );
    f.render_widget(plugin_list, chunks[1]);
}

#[allow(clippy::too_many_lines)]
fn draw_explorer(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    // 搜尋欄 + 主內容
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    let search_border = if app.input_mode {
        theme::CYAN
    } else {
        theme::SURFACE_HI
    };
    let search_fg = if app.input_mode {
        theme::CYAN
    } else {
        theme::SUBTLE
    };
    let search_text = if app.search_query.is_empty() {
        if app.input_mode {
            "Type to filter... (Press Enter/Esc to lock)"
        } else {
            "Press [/] to filter AST symbols..."
        }
    } else {
        &app.search_query
    };
    let search_p = Paragraph::new(search_text)
        .style(Style::default().fg(search_fg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(search_border))
                .title(Line::from(Span::styled(
                    " 🔍 Search / Filter ",
                    Style::default()
                        .fg(theme::MAUVE)
                        .add_modifier(Modifier::BOLD),
                ))),
        );
    f.render_widget(search_p, content_chunks[0]);

    // 雙欄：節點列表 + 詳細 Inspector
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(content_chunks[1]);

    // 左側：節點列表 (kind 圖示著色)
    let list_items: Vec<ListItem> = app
        .filtered_nodes
        .iter()
        .filter_map(|&idx| app.graph.nodes.get(idx))
        .map(|node| {
            let (icon, color) = match node.kind.as_str() {
                "struct" | "class" => ("[S]", theme::GOLD),
                "interface" | "trait" => ("[I]", theme::MAUVE),
                "function" | "fn" | "method" => ("[F]", theme::GREEN),
                "module" | "file" => ("[M]", theme::BLUE),
                _ => ("[N]", theme::TEXT),
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    icon,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(&node.label, Style::default().fg(theme::TEXT)),
                Span::styled(
                    format!(" ({})", node.kind),
                    Style::default().fg(theme::SUBTLE),
                ),
            ]))
        })
        .collect();

    let list_title = Line::from(vec![
        Span::styled(
            " 🌲 Nodes List ",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}/{})", app.filtered_nodes.len(), app.graph.nodes.len()),
            Style::default().fg(theme::SUBTLE),
        ),
    ]);
    let list_widget = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::SURFACE_HI))
                .title(list_title),
        )
        .highlight_style(
            Style::default()
                .bg(theme::CYAN)
                .fg(theme::BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ▶ ");
    f.render_stateful_widget(list_widget, main_chunks[0], &mut app.list_state);
    app.last_list_area = Some(main_chunks[0]);

    // 右側：詳細 Node Inspector
    let mut inspector_lines = Vec::new();
    if let Some(node) = app.selected_node() {
        inspector_lines.push(Line::from(vec![
            Span::styled("ID: ", Style::default().fg(theme::SUBTLE)),
            Span::styled(
                &node.id.0,
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(theme::GOLD),
            ),
        ]));
        inspector_lines.push(Line::from(vec![
            Span::styled("Label: ", Style::default().fg(theme::SUBTLE)),
            Span::styled(&node.label, Style::default().fg(theme::TEXT)),
        ]));
        inspector_lines.push(Line::from(vec![
            Span::styled("Kind: ", Style::default().fg(theme::SUBTLE)),
            Span::styled(&node.kind, Style::default().fg(theme::CYAN)),
        ]));
        inspector_lines.push(Line::from(vec![
            Span::styled("File: ", Style::default().fg(theme::SUBTLE)),
            Span::styled(
                format!("{}:{}", node.source_file, node.start_line),
                Style::default().fg(theme::GREEN),
            ),
        ]));
        if let Some(ref desc) = node.description {
            inspector_lines.push(Line::from(""));
            inspector_lines.push(Line::from(vec![
                Span::styled("Description: ", Style::default().fg(theme::SUBTLE)),
                Span::styled(desc, Style::default().fg(theme::TEXT)),
            ]));
        }

        let mut outgoing_lines = Vec::new();
        let mut incoming_lines = Vec::new();
        for edge in &app.graph.edges {
            if edge.source == node.id {
                outgoing_lines.push(Line::from(Span::styled(
                    format!("  ➔ {} ({})", edge.target.0, edge.relation),
                    Style::default().fg(theme::TEXT),
                )));
            }
            if edge.target == node.id {
                incoming_lines.push(Line::from(Span::styled(
                    format!("  ⇠ {} ({})", edge.source.0, edge.relation),
                    Style::default().fg(theme::BLUE),
                )));
            }
        }

        if !outgoing_lines.is_empty() {
            inspector_lines.push(Line::from(""));
            inspector_lines.push(Line::from(Span::styled(
                "Outgoing Calls / References:",
                Style::default()
                    .fg(theme::GOLD)
                    .add_modifier(Modifier::BOLD),
            )));
            inspector_lines.extend(outgoing_lines.into_iter().take(10));
        }

        if !incoming_lines.is_empty() {
            inspector_lines.push(Line::from(""));
            inspector_lines.push(Line::from(Span::styled(
                "Incoming Calls / References:",
                Style::default()
                    .fg(theme::MAUVE)
                    .add_modifier(Modifier::BOLD),
            )));
            inspector_lines.extend(incoming_lines.into_iter().take(10));
        }
    } else {
        inspector_lines.push(Line::from("Select a node to inspect..."));
    }

    let inspector_p = Paragraph::new(inspector_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::SURFACE_HI))
                .title(Line::from(Span::styled(
                    " 📝 Node Inspector ",
                    Style::default()
                        .fg(theme::MAUVE)
                        .add_modifier(Modifier::BOLD),
                ))),
        )
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(inspector_p, main_chunks[1]);
}
