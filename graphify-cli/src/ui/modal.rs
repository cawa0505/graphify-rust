use crate::ui::theme;
use graphify_registry::db::{PluginRegistrationRow, PluginStatus, WorkspaceRow};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

/// Modal 列表單一項目
#[derive(Debug, Clone)]
pub struct ModalItem {
    pub text: String,
    pub fg: Color,
}

impl ModalItem {
    #[must_use]
    pub fn new(text: impl Into<String>, fg: Color) -> Self {
        Self {
            text: text.into(),
            fg,
        }
    }
}

/// 浮動視窗狀態：BFS 追蹤鏈、關係檢查器、Plugin 面板、或 Workspace 選擇器
#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    BfsTrace(Vec<ModalItem>),
    Relations(Vec<ModalItem>),
    PluginPanel {
        plugins: Vec<PluginRegistrationRow>,
        hovered: usize,
    },
    WorkspaceSelector {
        workspaces: Vec<WorkspaceRow>,
        hovered: usize,
    },
}

impl ModalState {
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::BfsTrace(_) => " 🔍 BFS Path Trace ",
            Self::Relations(_) => " 📡 Relations Inspector ",
            Self::PluginPanel { .. } => " 🔌 Plugin Health Monitor ",
            Self::WorkspaceSelector { .. } => " 📂 Switch Workspace ",
        }
    }

    #[must_use]
    pub fn items(&self) -> &[ModalItem] {
        match self {
            Self::None | Self::PluginPanel { .. } | Self::WorkspaceSelector { .. } => &[],
            Self::BfsTrace(items) | Self::Relations(items) => items,
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::BfsTrace(items) | Self::Relations(items) => items.len(),
            Self::PluginPanel { plugins, .. } => plugins.len(),
            Self::WorkspaceSelector { workspaces, .. } => workspaces.len(),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 計算置中的彈出式視窗區域，可用於 Modal 疊加
#[must_use]
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// 繪製浮動 Modal (Clear 疊加 + 亮紫邊框 + 可懸停列表)
/// 回傳列表內部區域以供滑鼠懸停命中測試
pub fn draw_modal(
    f: &mut ratatui::Frame,
    state: &ModalState,
    hovered: Option<usize>,
    area: Rect,
) -> Option<Rect> {
    match state {
        ModalState::None => None,
        ModalState::BfsTrace(_) | ModalState::Relations(_) => Some(draw_list_modal(
            f,
            state.title(),
            state.items(),
            hovered,
            area,
        )),
        ModalState::PluginPanel { plugins, hovered: h } => {
            Some(draw_plugin_panel(f, plugins, *h, area))
        }
        ModalState::WorkspaceSelector { workspaces, hovered: h } => {
            Some(draw_workspace_selector(f, workspaces, *h, area))
        }
    }
}

fn draw_plugin_panel(
    f: &mut ratatui::Frame,
    plugins: &[PluginRegistrationRow],
    hovered: usize,
    area: Rect,
) -> Rect {
    let popup = centered_rect(70, 65, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )
        .title(Line::from(Span::styled(
            " 🔌 Plugin Health Monitor ",
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // header hint
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " [Esc/c] Close · [j/k] Navigate · [F5] Reset Quarantine ",
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[0],
    );

    // plugin list
    let list_items: Vec<ListItem> = if plugins.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No plugins registered for this workspace.",
            Style::default().fg(theme::SUBTLE),
        )))]
    } else {
        plugins
            .iter()
            .enumerate()
            .map(|(i, reg)| {
                let (icon, color) = match reg.status {
                    PluginStatus::Healthy => ("●", theme::GREEN),
                    PluginStatus::Degraded => ("◐", theme::GOLD),
                    PluginStatus::Unavailable => ("○", theme::SUBTLE),
                    PluginStatus::Quarantined => ("⊘", theme::RED),
                };
                let arrow = if i == hovered { "▶ " } else { "  " };
                let last_synced = if reg.last_synced_at > 0 {
                    format!("last: {}", reg.last_synced_at)
                } else {
                    "last: ──".to_string()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        arrow,
                        Style::default()
                            .fg(theme::CYAN)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {icon} "), Style::default().fg(color)),
                    Span::styled(
                        &reg.plugin_id,
                        Style::default().fg(theme::TEXT),
                    ),
                    Span::raw("  "),
                    Span::styled(last_synced, Style::default().fg(theme::SUBTLE)),
                ]))
            })
            .collect()
    };
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(theme::SURFACE_HI)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut list_state = ListState::default();
    list_state.select(Some(hovered));
    f.render_stateful_widget(list, rows[1], &mut list_state);

    // footer count
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} plugins ", plugins.len()),
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[2],
    );

    rows[1]
}

fn draw_workspace_selector(
    f: &mut ratatui::Frame,
    workspaces: &[WorkspaceRow],
    hovered: usize,
    area: Rect,
) -> Rect {
    let popup = centered_rect(70, 65, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )
        .title(Line::from(Span::styled(
            " 📂 Switch Workspace ",
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " [Esc/c] Close · [j/k] Navigate · [Enter] Switch ",
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[0],
    );

    let list_items: Vec<ListItem> = if workspaces.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No registered workspaces.",
            Style::default().fg(theme::SUBTLE),
        )))]
    } else {
        workspaces
            .iter()
            .enumerate()
            .map(|(i, ws)| {
                let marker = if ws.is_active { " ◉ " } else { "    " };
                let arrow = if i == hovered { "▶ " } else { "  " };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        arrow,
                        Style::default()
                            .fg(theme::CYAN)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(marker, Style::default().fg(theme::GOLD)),
                    Span::styled(
                        &ws.root_path,
                        Style::default().fg(theme::TEXT),
                    ),
                    Span::raw(" ("),
                    Span::styled(
                        &ws.workspace_key,
                        Style::default().fg(theme::SUBTLE),
                    ),
                    Span::raw(")"),
                ]))
            })
            .collect()
    };
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(theme::SURFACE_HI)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut list_state = ListState::default();
    list_state.select(Some(hovered));
    f.render_stateful_widget(list, rows[1], &mut list_state);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} workspaces ", workspaces.len()),
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[2],
    );

    rows[1]
}

fn draw_list_modal(
    f: &mut ratatui::Frame,
    title: &str,
    items: &[ModalItem],
    hovered: Option<usize>,
    area: Rect,
) -> Rect {
    let popup = centered_rect(64, 60, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )
        .title(Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " [Esc/c] Close · [j/k] Navigate · [Mouse] Hover ",
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[0],
    );

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let arrow = if hovered == Some(i) { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(
                    arrow,
                    Style::default()
                        .fg(theme::CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&it.text, Style::default().fg(it.fg)),
            ]))
        })
        .collect();
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(theme::SURFACE_HI)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut list_state = ListState::default();
    list_state.select(hovered);
    f.render_stateful_widget(list, rows[1], &mut list_state);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} items ", items.len()),
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[2],
    );

    rows[1]
}
