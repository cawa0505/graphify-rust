use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use graphify_core::{GraphOutput, Node};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::{
    io,
    process::Command,
    time::Duration,
};

pub struct App {
    pub graph: GraphOutput,
    pub filtered_nodes: Vec<usize>,
    pub list_state: ListState,
    pub search_query: String,
    pub input_mode: bool,
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

        Self {
            graph,
            filtered_nodes,
            list_state,
            search_query: String::new(),
            input_mode: false,
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
    }

    #[must_use]
    pub fn selected_node(&self) -> Option<&Node> {
        self.list_state
            .selected()
            .and_then(|idx| self.filtered_nodes.get(idx))
            .and_then(|&node_idx| self.graph.nodes.get(node_idx))
    }
}

pub fn run_tui(graph: GraphOutput) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Establish panic hook to safely restore terminal state on panic!
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(out, LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    let mut app = App::new(graph);
    let res = run_loop(&mut terminal, &mut app);

    // Restore terminal state cleanly
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
            if let Event::Key(key) = event::read()? {
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
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.next();
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.previous();
                        }
                        KeyCode::Char('/') => {
                            app.input_mode = true;
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
                                
                                // Cleanly restore terminal to launch editor
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

                                // Re-enable raw mode and Alternate Screen
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
        }
    }
}

#[allow(clippy::too_many_lines)]
fn draw_ui(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search bar
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Status bar
        ])
        .split(f.area());

    // 1. Search Bar Rendering
    let search_style = if app.input_mode {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let search_text = if app.search_query.is_empty() {
        if app.input_mode {
            "Type to filter nodes... (Press Enter/Esc to confirm)"
        } else {
            "Press [/] to search / filter nodes..."
        }
    } else {
        &app.search_query
    };
    let search_p = Paragraph::new(search_text)
        .style(search_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 🔍 Search / Filter "),
        );
    f.render_widget(search_p, chunks[0]);

    // 2. Main Content Split (Left: Nodes List, Right: Node Inspector)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Left List Rendering
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

    // Right Inspector Rendering
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

        // Compute incoming/outgoing links from edge array
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

    // 3. Status Bar Rendering
    let status_line = Line::from(vec![
        Span::styled(" <j/k>: Navigate ", Style::default().bg(Color::Gray).fg(Color::Black)),
        Span::raw(" | "),
        Span::styled(" </>: Search ", Style::default().bg(Color::Cyan).fg(Color::Black)),
        Span::raw(" | "),
        Span::styled(" <g/Enter>: Jump to Code ", Style::default().bg(Color::Green).fg(Color::Black)),
        Span::raw(" | "),
        Span::styled(" <Esc/c>: Clear ", Style::default().bg(Color::Yellow).fg(Color::Black)),
        Span::raw(" | "),
        Span::styled(" <q>: Quit ", Style::default().bg(Color::Red).fg(Color::White)),
    ]);
    let status_p = Paragraph::new(status_line).block(Block::default().borders(Borders::ALL));
    f.render_widget(status_p, chunks[2]);
}
