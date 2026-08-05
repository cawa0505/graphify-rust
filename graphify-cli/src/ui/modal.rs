use crate::ui::theme;
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

/// 浮動視窗狀態：BFS 追蹤鏈 或 關係檢查器 (Outgoing / Incoming)
#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    BfsTrace(Vec<ModalItem>),
    Relations(Vec<ModalItem>),
}

impl ModalState {
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::BfsTrace(_) => " 🔍 BFS Path Trace ",
            Self::Relations(_) => " 📡 Relations Inspector ",
        }
    }

    #[must_use]
    pub fn items(&self) -> &[ModalItem] {
        match self {
            Self::None => &[],
            Self::BfsTrace(items) | Self::Relations(items) => items,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items().is_empty()
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
    }
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
