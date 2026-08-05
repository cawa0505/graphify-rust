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

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use graphify_core::{GraphOutput, Node};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
    Terminal,
};
use std::{
    io,
    process::Command,
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Explorer,
    VisualGraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalState {
    None,
    BfsTrace(Vec<String>),
}

pub struct App {
    pub graph: GraphOutput,
    pub filtered_nodes: Vec<usize>,
    pub list_state: ListState,
    pub search_query: String,
    pub input_mode: bool,

    // TABS & MODALS
    pub active_tab: ActiveTab,
    pub modal_state: ModalState,

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

        Self {
            graph,
            filtered_nodes,
            list_state,
            search_query: String::new(),
            input_mode: false,
            active_tab: ActiveTab::Explorer,
            modal_state: ModalState::None,
            canvas_coords,
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            last_canvas_area: None,
            last_tabs_area: None,
            last_list_area: None,
            drag_start: None,
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
            self.canvas_coords = crate::ui::canvas::NodeCoordinates::compute(&self.graph, Some(&node.id));
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
            // 使用核心圖引擎構造有向圖
            if let Ok((graph, node_map)) = graphify_core::build_graph(&self.graph.nodes, &self.graph.edges) {
                // 呼叫核心 BFS 遍歷，獲取最大深度為 3 的有向子圖
                if let Ok(sub_graph) = graphify_core::query_bfs(&graph, &node_map, &start_node.id, 3) {
                    let mut trace = sub_graph.nodes
                        .iter()
                        .map(|n| format!("{}:{}", n.kind, n.label))
                        .collect::<Vec<String>>();
                    // 保持追蹤鏈在彈窗內的緊湊呈現 (最多 6 個節點)
                    trace.truncate(6);
                    return trace;
                }
            }
        }
        Vec::new()
    }
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

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    // 若當前有開啟 Modal，僅接受關閉 Modal 的按鍵
                    if let ModalState::BfsTrace(_) = app.modal_state {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('c') => {
                                app.modal_state = ModalState::None;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.input_mode {
                        match key.code {
                            KeyCode::Enter | KeyCode::Esc => {
                                app.input_mode = false;
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
                    } else {
                        match key.code {
                            KeyCode::Char('q') => {
                                return Ok(());
                            }
                            // 切換分頁
                            KeyCode::Tab => {
                                app.active_tab = match app.active_tab {
                                    ActiveTab::Explorer => ActiveTab::VisualGraph,
                                    ActiveTab::VisualGraph => ActiveTab::Explorer,
                                };
                            }
                            KeyCode::Char('1') => {
                                app.active_tab = ActiveTab::Explorer;
                            }
                            KeyCode::Char('2') => {
                                app.active_tab = ActiveTab::VisualGraph;
                            }
                            // 觸發 BFS 追蹤鏈 Modal
                            KeyCode::Char('t') | KeyCode::Char('T') => {
                                let trace = app.compute_bfs_trace();
                                app.modal_state = ModalState::BfsTrace(trace);
                            }
                            // 一般節點導航 (僅在 Explorer 分頁生效)
                            KeyCode::Char('j') | KeyCode::Down => {
                                if app.active_tab == ActiveTab::Explorer {
                                    app.next();
                                } else {
                                    // Visual Graph 平移向下
                                    app.pan_y -= 2.0 * app.zoom;
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if app.active_tab == ActiveTab::Explorer {
                                    app.previous();
                                } else {
                                    // Visual Graph 平移向上
                                    app.pan_y += 2.0 * app.zoom;
                                }
                            }
                            KeyCode::Char('h') | KeyCode::Left => {
                                if app.active_tab == ActiveTab::VisualGraph {
                                    // Visual Graph 平移向左
                                    app.pan_x -= 4.0 * app.zoom;
                                }
                            }
                            KeyCode::Char('l') | KeyCode::Right => {
                                if app.active_tab == ActiveTab::VisualGraph {
                                    // Visual Graph 平移向右
                                    app.pan_x += 4.0 * app.zoom;
                                }
                            }
                            // 縮放與重設
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                if app.active_tab == ActiveTab::VisualGraph {
                                    app.zoom = (app.zoom - 0.1).max(0.2); // 放大：縮小 bounds
                                }
                            }
                            KeyCode::Char('-') => {
                                if app.active_tab == ActiveTab::VisualGraph {
                                    app.zoom = (app.zoom + 0.1).min(3.0); // 縮小：擴大 bounds
                                }
                            }
                            KeyCode::Char('r') | KeyCode::Char('R') => {
                                if app.active_tab == ActiveTab::VisualGraph {
                                    app.pan_x = 0.0;
                                    app.pan_y = 0.0;
                                    app.zoom = 1.0;
                                }
                            }
                            KeyCode::Char('/') => {
                                if app.active_tab == ActiveTab::Explorer {
                                    app.input_mode = true;
                                }
                            }
                            KeyCode::Char('c') | KeyCode::Esc => {
                                if !app.search_query.is_empty() {
                                    app.search_query.clear();
                                    app.filter_nodes();
                                }
                            }
                            KeyCode::Char('g') | KeyCode::Enter => {
                                if let Some(node) = app.selected_node() {
                                    let file_path = &node.source_file;
                                    let line = node.start_line;

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
                                    let _ = cmd.arg(file_path).status();

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
                    }
                }
                Event::Mouse(mouse_event) => {
                    let click_col = mouse_event.column;
                    let click_row = mouse_event.row;

                    match mouse_event.kind {
                        MouseEventKind::ScrollUp => {
                            if app.active_tab == ActiveTab::VisualGraph {
                                app.zoom = (app.zoom - 0.05).max(0.1); // ponytail: 滾輪向上放大（調整視窗投射邊界縮小 = Zoom In 放大效果）
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            if app.active_tab == ActiveTab::VisualGraph {
                                app.zoom = (app.zoom + 0.05).min(5.0); // ponytail: 滾輪向下縮小（調整視窗投射邊界擴大 = Zoom Out 縮小效果）
                            }
                        }
                        MouseEventKind::Down(MouseButton::Left) => {
                            // 1. 記錄拖曳起點
                            if app.active_tab == ActiveTab::VisualGraph {
                                app.drag_start = Some((click_col, click_row));
                            }

                            // 2. 偵測點擊 Tab 切換 (動態區域)
                            if let Some(tabs_area) = app.last_tabs_area {
                                if click_row == tabs_area.y + 1 {
                                    let half_width = tabs_area.width / 2;
                                    if click_col >= tabs_area.x && click_col < tabs_area.x + half_width {
                                        app.active_tab = ActiveTab::Explorer;
                                    } else if click_col >= tabs_area.x + half_width && click_col < tabs_area.x + tabs_area.width {
                                        app.active_tab = ActiveTab::VisualGraph;
                                    }
                                }
                            }

                            // 3. 偵測點擊 Explorer 節點列表 (動態區域)
                            if app.active_tab == ActiveTab::Explorer {
                                if let Some(list_area) = app.last_list_area {
                                    let list_top = list_area.y + 1;
                                    let list_bottom = list_area.y + list_area.height - 1;
                                    if click_row >= list_top && click_row < list_bottom && click_col >= list_area.x && click_col < list_area.x + list_area.width {
                                        let clicked_idx = (click_row - list_top) as usize;
                                        if clicked_idx < app.filtered_nodes.len() {
                                            app.list_state.select(Some(clicked_idx));
                                            app.update_selected_coords();
                                        }
                                    }
                                }
                            }

                            // 4. 偵測點擊 Visual Graph 節點選擇 (動態畫布)
                            if app.active_tab == ActiveTab::VisualGraph {
                                if let Some(canvas_area) = app.last_canvas_area {
                                    let canvas_top = canvas_area.y;
                                    let canvas_bottom = canvas_area.y + canvas_area.height;
                                    if click_row >= canvas_top && click_row < canvas_bottom && click_col >= canvas_area.x && click_col < canvas_area.x + canvas_area.width {
                                        let w_ratio = f64::from(click_col - canvas_area.x) / f64::from(canvas_area.width);
                                        let h_ratio = f64::from(click_row - canvas_top) / f64::from(canvas_area.height);

                                        let x_min = -60.0 * app.zoom + app.pan_x;
                                        let x_max = 60.0 * app.zoom + app.pan_x;
                                        let y_min = -30.0 * app.zoom + app.pan_y;
                                        let y_max = 30.0 * app.zoom + app.pan_y;

                                        let clicked_x = x_min + w_ratio * (x_max - x_min);
                                        let clicked_y = y_max - h_ratio * (y_max - y_min);

                                        let mut closest_node_idx = None;
                                        let mut min_dist = 6.0 * app.zoom;

                                        for &idx in &app.filtered_nodes {
                                            if let Some(node) = app.graph.nodes.get(idx) {
                                                if let Some(&(nx, ny)) = app.canvas_coords.coords.get(&node.id) {
                                                    let dist = (clicked_x - nx).hypot(clicked_y - ny);
                                                    if dist < min_dist {
                                                        min_dist = dist;
                                                        closest_node_idx = Some(idx);
                                                    }
                                                }
                                            }
                                        }

                                        if let Some(target_idx) = closest_node_idx {
                                            if let Some(list_pos) = app.filtered_nodes.iter().position(|&node_idx| node_idx == target_idx) {
                                                app.list_state.select(Some(list_pos));
                                                app.update_selected_coords();
                                            }
                                        }
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
                                }
                            }
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            app.drag_start = None;
                        }
                        MouseEventKind::Down(MouseButton::Right) | MouseEventKind::Up(MouseButton::Right) => {
                            // 右鍵點擊：快速聚焦並開啟關係 Modal
                            if app.active_tab == ActiveTab::Explorer {
                                if let Some(list_area) = app.last_list_area {
                                    let list_top = list_area.y + 1;
                                    let list_bottom = list_area.y + list_area.height - 1;
                                    if click_row >= list_top && click_row < list_bottom && click_col >= list_area.x && click_col < list_area.x + list_area.width {
                                        let clicked_idx = (click_row - list_top) as usize;
                                        if clicked_idx < app.filtered_nodes.len() {
                                            app.list_state.select(Some(clicked_idx));
                                            app.update_selected_coords();
                                            let trace = app.compute_bfs_trace();
                                            app.modal_state = ModalState::BfsTrace(trace);
                                        }
                                    }
                                }
                            } else if app.active_tab == ActiveTab::VisualGraph {
                                if let Some(canvas_area) = app.last_canvas_area {
                                    let canvas_top = canvas_area.y;
                                    let canvas_bottom = canvas_area.y + canvas_area.height;
                                    if click_row >= canvas_top && click_row < canvas_bottom && click_col >= canvas_area.x && click_col < canvas_area.x + canvas_area.width {
                                        let w_ratio = f64::from(click_col - canvas_area.x) / f64::from(canvas_area.width);
                                        let h_ratio = f64::from(click_row - canvas_top) / f64::from(canvas_area.height);

                                        let x_min = -60.0 * app.zoom + app.pan_x;
                                        let x_max = 60.0 * app.zoom + app.pan_x;
                                        let y_min = -30.0 * app.zoom + app.pan_y;
                                        let y_max = 30.0 * app.zoom + app.pan_y;

                                        let clicked_x = x_min + w_ratio * (x_max - x_min);
                                        let clicked_y = y_max - h_ratio * (y_max - y_min);

                                        let mut closest_node_idx = None;
                                        let mut min_dist = 6.0 * app.zoom;

                                        for &idx in &app.filtered_nodes {
                                            if let Some(node) = app.graph.nodes.get(idx) {
                                                if let Some(&(nx, ny)) = app.canvas_coords.coords.get(&node.id) {
                                                    let dist = (clicked_x - nx).hypot(clicked_y - ny);
                                                    if dist < min_dist {
                                                        min_dist = dist;
                                                        closest_node_idx = Some(idx);
                                                    }
                                                }
                                            }
                                        }

                                        if let Some(target_idx) = closest_node_idx {
                                            if let Some(list_pos) = app.filtered_nodes.iter().position(|&node_idx| node_idx == target_idx) {
                                                app.list_state.select(Some(list_pos));
                                                app.update_selected_coords();
                                                let trace = app.compute_bfs_trace();
                                                app.modal_state = ModalState::BfsTrace(trace);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn draw_ui(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab Bar / Search Bar
            Constraint::Min(10),   // Main View Panel
            Constraint::Length(3), // Status Bar
        ])
        .split(f.area());

    // 1. Tab Bar 導航列繪製
    let tab_titles = vec![" 🔍 Explorer (1) ", " 📊 Visual Graph (2) "];
    let active_idx = match app.active_tab {
        ActiveTab::Explorer => 0,
        ActiveTab::VisualGraph => 1,
    };
    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).title(" 🧭 Graphify TUI Navigation "))
        .select(active_idx)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[0]);
    app.last_tabs_area = Some(chunks[0]);

    // 2. 依據目前分頁繪製主面板 (Main View Panel)
    match app.active_tab {
        ActiveTab::Explorer => {
            // Tab 1: 傳統的雙欄代碼與關係瀏覽器
            let content_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(5)])
                .split(chunks[1]);

            // 搜尋欄
            let search_style = if app.input_mode {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
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
                .style(search_style)
                .block(Block::default().borders(Borders::ALL).title(" 🔍 Search / Filter "));
            f.render_widget(search_p, content_chunks[0]);

            // 雙欄
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(content_chunks[1]);

            // 左側：節點列表
            let list_items: Vec<ListItem> = app
                .filtered_nodes
                .iter()
                .filter_map(|&idx| app.graph.nodes.get(idx))
                .map(|node| {
                    let icon = match node.kind.as_str() {
                        "struct" | "class" => "[S]",
                        "interface" | "trait" => "[I]",
                        "function" | "fn" | "method" => "[F]",
                        "module" | "file" => "[M]",
                        _ => "[N]",
                    };
                    let text = format!("{} {} ({})", icon, node.label, node.kind);
                    ListItem::new(text).style(Style::default().fg(Color::White))
                })
                .collect();

            let list_title = format!(" 🌲 Nodes List ({}/{}) ", app.filtered_nodes.len(), app.graph.nodes.len());
            let list_widget = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(list_title))
                .highlight_style(
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(" ➔ ");
            f.render_stateful_widget(list_widget, main_chunks[0], &mut app.list_state);
            app.last_list_area = Some(main_chunks[0]);

            // 右側：詳細 Node Inspector
            let mut inspector_lines = Vec::new();
            if let Some(node) = app.selected_node() {
                inspector_lines.push(Line::from(vec![
                    Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&node.id.0, Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow)),
                ]));
                inspector_lines.push(Line::from(vec![
                    Span::styled("Label: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&node.label, Style::default().fg(Color::White)),
                ]));
                inspector_lines.push(Line::from(vec![
                    Span::styled("Kind: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&node.kind, Style::default().fg(Color::Cyan)),
                ]));
                inspector_lines.push(Line::from(vec![
                    Span::styled("File: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{}:{}", node.source_file, node.start_line), Style::default().fg(Color::Green)),
                ]));
                if let Some(ref desc) = node.description {
                    inspector_lines.push(Line::from(""));
                    inspector_lines.push(Line::from(vec![
                        Span::styled("Description: ", Style::default().fg(Color::DarkGray)),
                        Span::styled(desc, Style::default().fg(Color::White)),
                    ]));
                }

                let mut outgoing_lines = Vec::new();
                let mut incoming_lines = Vec::new();
                for edge in &app.graph.edges {
                    if edge.source == node.id {
                        let text = format!("  ➔ {} ({})", edge.target.0, edge.relation);
                        outgoing_lines.push(Line::from(Span::styled(text, Style::default().fg(Color::White))));
                    }
                    if edge.target == node.id {
                        let text = format!("  ⇠ {} ({})", edge.source.0, edge.relation);
                        incoming_lines.push(Line::from(Span::styled(text, Style::default().fg(Color::White))));
                    }
                }

                if !outgoing_lines.is_empty() {
                    inspector_lines.push(Line::from(""));
                    inspector_lines.push(Line::from(Span::styled("Outgoing Calls / References:", Style::default().fg(Color::Magenta))));
                    for line in outgoing_lines.into_iter().take(10) {
                        inspector_lines.push(line);
                    }
                }

                if !incoming_lines.is_empty() {
                    inspector_lines.push(Line::from(""));
                    inspector_lines.push(Line::from(Span::styled("Incoming Calls / References:", Style::default().fg(Color::LightBlue))));
                    for line in incoming_lines.into_iter().take(10) {
                        inspector_lines.push(line);
                    }
                }
            } else {
                inspector_lines.push(Line::from("Select a node to inspect..."));
            }

            let inspector_p = Paragraph::new(inspector_lines)
                .block(Block::default().borders(Borders::ALL).title(" 📝 Node Inspector "))
                .wrap(ratatui::widgets::Wrap { trim: true });
            f.render_widget(inspector_p, main_chunks[1]);
        }
        ActiveTab::VisualGraph => {
            // Tab 2: 全螢幕 Canvas 關係圖繪製
            let selected_id = app.selected_node().map(|n| &n.id);
            let canvas_widget = crate::ui::canvas::create_canvas_widget(
                &app.graph,
                &app.canvas_coords,
                selected_id,
                app.pan_x,
                app.pan_y,
                app.zoom,
            );
            f.render_widget(canvas_widget, chunks[1]);
            app.last_canvas_area = Some(chunks[1]);
        }
    }

    // 3. Status Bar 狀態列繪製 (Neovim/fzf 極致高對比色調設計)
    let status_line = match app.active_tab {
        ActiveTab::Explorer => Line::from(vec![
            Span::styled(" [Tab]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" View ", Style::default().fg(Color::White)),
            Span::styled(" [j/k]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Nav ", Style::default().fg(Color::White)),
            Span::styled(" [/]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Filter ", Style::default().fg(Color::White)),
            Span::styled(" [t/Right-Click]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" BFS Trace ", Style::default().fg(Color::White)),
            Span::styled(" [g/Enter]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Code ", Style::default().fg(Color::White)),
            Span::styled(" [q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Quit", Style::default().fg(Color::White)),
        ]),
        ActiveTab::VisualGraph => Line::from(vec![
            Span::styled(" [Tab]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" View ", Style::default().fg(Color::White)),
            Span::styled(" [Left-Click]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Select ", Style::default().fg(Color::White)),
            Span::styled(" [Left-Drag/hjkl]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Pan ", Style::default().fg(Color::White)),
            Span::styled(" [Scroll/+/-]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Zoom ", Style::default().fg(Color::White)),
            Span::styled(" [Right-Click/t]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" BFS Trace ", Style::default().fg(Color::White)),
            Span::styled(" [r]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Reset ", Style::default().fg(Color::White)),
            Span::styled(" [q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Quit", Style::default().fg(Color::White)),
        ]),
    };
    let status_p = Paragraph::new(status_line).block(Block::default().borders(Borders::ALL));
    f.render_widget(status_p, chunks[2]);

    // 4. 若當前處於 BfsTrace Modal 狀態，繪製疊加置中彈窗！
    if let ModalState::BfsTrace(ref trace_path) = app.modal_state {
        crate::ui::modal::draw_bfs_modal(f, trace_path, f.area());
    }
}