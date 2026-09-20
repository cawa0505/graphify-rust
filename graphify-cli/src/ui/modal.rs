use crate::ui::theme;
use graphify_registry::db::{PluginRegistrationRow, PluginStatus, WorkspaceRow};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::path::PathBuf;

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
    /// Compose 面板：menu 層（manifest 列表）與 diagram 層（ASCII 關聯圖）。
    /// `selected` 為 diagram 層當前顯示的 manifest 檔名；`diagram` 為手繪
    /// ASCII 行陣列；`error` 為驗證/合併錯誤（紅字逐行顯示）。
    ComposePanel {
        manifests: Vec<PathBuf>,
        hovered: usize,
        selected: Option<PathBuf>,
        diagram: Vec<String>,
        error: Vec<String>,
        scroll: u16,
        h_scroll: u16,
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
            // menu/diagram 兩層共用 Compose 標題（diagram 層另於邊框顯示 manifest 路徑）
            Self::ComposePanel { .. } => " 🧩 Compose ",
        }
    }

    #[must_use]
    pub fn items(&self) -> &[ModalItem] {
        match self {
            Self::BfsTrace(items) | Self::Relations(items) => items,
            // ComposePanel 用 diagram/error 專屬渲染，不走 ModalItem 列表
            Self::None
            | Self::PluginPanel { .. }
            | Self::WorkspaceSelector { .. }
            | Self::ComposePanel { .. } => &[],
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::BfsTrace(items) | Self::Relations(items) => items.len(),
            Self::PluginPanel { plugins, .. } => plugins.len(),
            Self::WorkspaceSelector { workspaces, .. } => workspaces.len(),
            Self::ComposePanel { manifests, .. } => manifests.len(),
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
        ModalState::PluginPanel {
            plugins,
            hovered: h,
        } => Some(draw_plugin_panel(f, plugins, *h, area)),
        ModalState::WorkspaceSelector {
            workspaces,
            hovered: h,
        } => Some(draw_workspace_selector(f, workspaces, *h, area)),
        ModalState::ComposePanel {
            manifests,
            hovered: h,
            selected,
            diagram,
            error,
            scroll,
            h_scroll,
        } => match selected {
            // Compose 已改為 Architecture tab 內嵌繪製（draw_ui 分派），
            // 浮動 modal 路徑僅防禦保留（不應被觸發）
            None => Some(draw_compose_menu(f, manifests, *h, area)),
            Some(path) => Some(draw_compose_diagram(
                f, path, diagram, error, *scroll, *h_scroll, area,
            )),
        },
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
                    Span::styled(&reg.plugin_id, Style::default().fg(theme::TEXT)),
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
                    Span::styled(&ws.root_path, Style::default().fg(theme::TEXT)),
                    Span::raw(" ("),
                    Span::styled(&ws.workspace_key, Style::default().fg(theme::SUBTLE)),
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

/// Compose 面板 menu 層：列出 manifest 檔（相對路徑），hover + Enter 選取。
pub fn draw_compose_menu(
    f: &mut ratatui::Frame,
    manifests: &[PathBuf],
    hovered: usize,
    area: Rect,
) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE_HI))
        .title(Line::from(Span::styled(
            " 🧩 Compose Manifests ",
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);

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
            " [Esc/c] Close · [j/k] Navigate · [Enter] Render diagram ",
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[0],
    );

    let list_items: Vec<ListItem> = if manifests.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "找不到 Assembly Manifest（*.yaml）— 於 workspace 根目錄或 manifests/ 放置",
            Style::default().fg(theme::SUBTLE),
        )))]
    } else {
        manifests
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let arrow = if i == hovered { "▶ " } else { "  " };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        arrow,
                        Style::default()
                            .fg(theme::CYAN)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(path.display().to_string(), Style::default().fg(theme::TEXT)),
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
            format!(" {} manifests ", manifests.len()),
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[2],
    );

    rows[1]
}

/// Compose 面板 diagram 層：手繪 ASCII 關聯圖（或紅字錯誤），可捲動。
pub fn draw_compose_diagram(
    f: &mut ratatui::Frame,
    manifest_path: &std::path::Path,
    diagram: &[String],
    error: &[String],
    scroll: u16,
    h_scroll: u16,
    area: Rect,
) -> Rect {
    let title = format!(" 🧩 Compose — {} ", manifest_path.display());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE_HI))
        .title(Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);

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
            " [Esc] Back to menu · [j/k] Scroll · [h/l] Horizontal ",
            Style::default().fg(theme::SUBTLE),
        ))),
        rows[0],
    );

    if error.is_empty() {
        // 手繪 ASCII 關聯圖（垂直捲動 + 水平捲動；不 wrap——wrap 會折斷 box-drawing 邊框）
        let lines: Vec<Line> = diagram
            .iter()
            .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme::TEXT))))
            .collect();
        f.render_widget(Paragraph::new(lines).scroll((scroll, h_scroll)), rows[1]);
    } else {
        // 驗證錯誤：紅字逐行，不靜默空白
        let lines: Vec<Line> = std::iter::once(Line::from(Span::styled(
            "✗ Manifest 驗證失敗：",
            Style::default().fg(theme::RED).add_modifier(Modifier::BOLD),
        )))
        .chain(error.iter().map(|e| {
            Line::from(Span::styled(
                format!("  • {e}"),
                Style::default().fg(theme::RED),
            ))
        }))
        .collect();
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[1]);
    }

    let status = if error.is_empty() {
        format!(" {} lines ", diagram.len())
    } else {
        format!(" {} errors ", error.len())
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            status,
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
