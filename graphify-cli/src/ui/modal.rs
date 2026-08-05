use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

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

/// 繪製 BFS Trace 彈出式浮動視窗 (Modal)
pub fn draw_bfs_modal(f: &mut ratatui::Frame, trace_path: &[String], area: Rect) {
    // 1. 計算置中區域 (寬度 70%, 高度 55%)
    let popup_area = centered_rect(70, 55, area);

    // 2. 使用 Clear 清除背景
    f.render_widget(Clear, popup_area);

    // 3. 建立 Modal 外框與樣式
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 🔍 Breadth-First Search (BFS) Path Trace ")
        .style(Style::default().bg(Color::Reset))
        .border_style(Style::default().fg(Color::Rgb(189, 147, 249)).add_modifier(Modifier::BOLD)); // 亮紫色邊框

    let mut content = Vec::new();
    content.push(Line::from(""));
    content.push(Line::from(vec![
        Span::styled("🎯 BFS Trace Results", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));
    content.push(Line::from(""));

    if trace_path.is_empty() {
        content.push(Line::from("  No call path found from this node."));
    } else {
        content.push(Line::from(vec![
            Span::styled("  📍 Path: ", Style::default().fg(Color::DarkGray)),
            Span::styled(trace_path.join(" ➔ "), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]));
    }

    content.push(Line::from(""));
    content.push(Line::from("─".repeat(popup_area.width as usize - 4)));
    content.push(Line::from(""));

    // .toon 導出優勢標籤 Badge
    content.push(Line::from(vec![
        Span::styled(" 💾 Export Status: ", Style::default().fg(Color::DarkGray)),
        Span::styled("Active (.toon)", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled("⚡ -60% Token Usage Saved", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
    ]));
    content.push(Line::from(""));
    content.push(Line::from(vec![
        Span::styled(" Press [Esc / c] to close this modal and return ", Style::default().fg(Color::Gray).add_modifier(Modifier::DIM)),
    ]));

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, popup_area);
}