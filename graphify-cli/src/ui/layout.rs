use crate::ui::{ActiveTab, theme};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::time::{Duration, Instant};

/// 頂層版面：Tab 列 / 主視圖 (70%) / 事件日誌 (30%) / 快捷鍵列
#[derive(Debug, Clone, Copy)]
pub struct Chrome {
    pub tabs: Rect,
    pub main: Rect,
    pub log: Rect,
    pub footer: Rect,
}

#[must_use]
pub fn split(area: Rect) -> Chrome {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(70),
            Constraint::Percentage(30),
            Constraint::Length(3),
        ])
        .split(area);
    Chrome {
        tabs: rows[0],
        main: rows[1],
        log: rows[2],
        footer: rows[3],
    }
}

/// 事件日誌單筆紀錄
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub text: String,
    pub fg: Color,
}

impl LogEntry {
    #[must_use]
    pub fn new(text: impl Into<String>, fg: Color) -> Self {
        Self {
            text: text.into(),
            fg,
        }
    }
}

/// 事件日誌最大保留筆數
const MAX_ENTRIES: usize = 200;

/// 追加日誌並限制長度，避免無界增長
pub fn push_log(log: &mut Vec<LogEntry>, entry: LogEntry) {
    log.push(entry);
    if log.len() > MAX_ENTRIES {
        log.drain(..log.len() - MAX_ENTRIES);
    }
}

/// 快捷鍵行為標籤，用於 Footer 提示閃爍
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTag {
    Nav,
    Filter,
    Trace,
    Edit,
    Quit,
    Pan,
    Zoom,
    Select,
    Inspect,
    Reset,
}

/// 動作觸發後短暫閃爍高亮的狀態
#[derive(Debug, Clone, Copy, Default)]
pub struct Flash {
    pub action: Option<ActionTag>,
    pub until: Option<Instant>,
}

impl Flash {
    pub fn trigger(&mut self, action: ActionTag) {
        self.action = Some(action);
        self.until = Some(Instant::now() + Duration::from_millis(220));
    }

    #[must_use]
    pub fn is_active(&self, action: ActionTag) -> bool {
        self.until.is_some_and(|t| Instant::now() < t) && self.action == Some(action)
    }

    pub fn tick(&mut self) {
        if self.until.is_some_and(|t| Instant::now() >= t) {
            self.until = None;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Pill {
    key: &'static str,
    label: &'static str,
    color: Color,
    action: ActionTag,
}

const fn pill(key: &'static str, label: &'static str, color: Color, action: ActionTag) -> Pill {
    Pill {
        key,
        label,
        color,
        action,
    }
}

/// 快捷鍵 Footer：區塊式藥丸按鈕，執行對應動作時整顆反白閃爍
pub fn render_footer(f: &mut ratatui::Frame, tab: ActiveTab, flash: &Flash, area: Rect) {
    let pills = match tab {
        ActiveTab::Explorer => &[
            pill("Tab", "View", theme::CYAN, ActionTag::Nav),
            pill("j/k", "Nav", theme::CYAN, ActionTag::Nav),
            pill("/", "Filter", theme::GOLD, ActionTag::Filter),
            pill("t", "Trace", theme::GOLD, ActionTag::Trace),
            pill("g/Enter", "Code", theme::GREEN, ActionTag::Edit),
            pill("q", "Quit", theme::RED, ActionTag::Quit),
        ][..],
        ActiveTab::VisualGraph => &[
            pill("Drag", "Pan", theme::GREEN, ActionTag::Pan),
            pill("Scroll", "Zoom", theme::CYAN, ActionTag::Zoom),
            pill("Click", "Select", theme::GREEN, ActionTag::Select),
            pill("R-Click", "Inspect", theme::GOLD, ActionTag::Inspect),
            pill("r", "Reset", theme::CYAN, ActionTag::Reset),
            pill("q", "Quit", theme::RED, ActionTag::Quit),
        ][..],
    };

    let mut spans: Vec<Span> = Vec::new();
    for p in pills {
        let active = flash.is_active(p.action);
        let bg = if active { p.color } else { theme::SURFACE_HI };
        let key_fg = if active { theme::BG } else { p.color };
        let label_fg = if active { theme::BG } else { theme::SUBTLE };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" {} ", p.key),
            Style::default()
                .fg(key_fg)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {} ", p.label),
            Style::default().fg(label_fg).bg(bg),
        ));
    }

    let title = Line::from(vec![
        Span::styled(
            " ⌨ Keybindings ",
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if flash.until.is_some() { " ⚡ " } else { "" },
            Style::default()
                .fg(theme::GOLD)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE_HI))
        .title(title);
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

/// 下半部事件日誌：可滾動 List，顯示滑鼠/鍵盤事件
pub fn render_event_log(
    f: &mut ratatui::Frame,
    entries: &[LogEntry],
    state: &mut ListState,
    area: Rect,
) {
    let items: Vec<ListItem> = entries
        .iter()
        .map(|e| ListItem::new(Line::from(Span::styled(&e.text, Style::default().fg(e.fg)))))
        .collect();
    let title = Line::from(vec![
        Span::styled(
            " 📜 Event Log ",
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({})", entries.len()),
            Style::default().fg(theme::SUBTLE),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE_HI))
        .title(title);
    f.render_stateful_widget(List::new(items).block(block), area, state);
}
