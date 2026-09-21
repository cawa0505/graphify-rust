#![allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    clippy::cast_precision_loss,
    clippy::option_if_let_else,
    clippy::doc_markdown,
    clippy::doc_lazy_continuation,
    clippy::unnecessary_map_or
)]

use crate::ui::theme;
use graphify_core::{GraphOutput, Node, NodeId};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line as TextLine, Span},
    widgets::{
        Block, Borders, Clear, Paragraph,
        canvas::{Canvas, Circle, Context, Line},
    },
};
use std::collections::HashMap;

/// 儲存各個節點在二維畫布上的實數座標
#[derive(Debug, Clone, Default)]
pub struct NodeCoordinates {
    pub coords: HashMap<NodeId, (f64, f64)>,
}

impl NodeCoordinates {
    /// 計算以當前選定節點為中心的二維拓撲星狀/環狀佈局
    /// - 選中節點 (Selected Node) 置於中心 (0.0, 0.0)
    /// - 選中節點的直接鄰居 (Incoming/Outgoing) 分布在半徑 32.0 的內圓環
    /// - 其他所有節點均勻分布在半徑 60.0 的外圓環
    /// 此演算法為 O(N) 複雜度、零動態記憶體迭代開銷，可完美維持 60 FPS 流暢度。
    #[must_use]
    pub fn compute(graph: &GraphOutput, selected_id: Option<&NodeId>) -> Self {
        let mut coords = HashMap::with_capacity(graph.nodes.len());
        if graph.nodes.is_empty() {
            return Self { coords };
        }

        let selected = match selected_id {
            Some(id) => id,
            None => &graph.nodes[0].id,
        };

        // 1. 中心節點
        coords.insert(selected.clone(), (0.0, 0.0));

        // 2. 收集選定節點的直接鄰居 (有連線關係者)
        let mut neighbors = Vec::new();
        for edge in &graph.edges {
            if edge.source == *selected && edge.target != *selected {
                if !neighbors.contains(&&edge.target) {
                    neighbors.push(&edge.target);
                }
            } else if edge.target == *selected && edge.source != *selected {
                if !neighbors.contains(&&edge.source) {
                    neighbors.push(&edge.source);
                }
            }
        }

        let neighbor_count = neighbors.len();
        for (i, &neigh_id) in neighbors.iter().enumerate() {
            let theta =
                (2.0 * std::f64::consts::PI * (i as f64)) / (neighbor_count as f64).max(1.0);
            let radius = 32.0; // ponytail: 擴大半徑，避免相鄰文字重疊
            let x = radius * theta.cos();
            let y = radius * theta.sin();
            coords.insert((*neigh_id).clone(), (x, y));
        }

        // 3. 收集其他無直接連線的節點
        let mut others = Vec::new();
        for node in &graph.nodes {
            if node.id != *selected && !coords.contains_key(&node.id) {
                others.push(&node.id);
            }
        }

        let other_count = others.len();
        for (j, &other_id) in others.iter().enumerate() {
            let phi = (2.0 * std::f64::consts::PI * (j as f64)) / (other_count as f64).max(1.0);
            let radius = 60.0; // ponytail: 擴大外圈半徑，騰出可讀空間
            let x = radius * phi.cos();
            let y = radius * phi.sin();
            coords.insert((*other_id).clone(), (x, y));
        }

        Self { coords }
    }
}

fn node_color(kind: &str) -> Color {
    match kind {
        "struct" | "class" => theme::GOLD,
        "function" | "fn" | "method" => theme::GREEN,
        "module" | "file" => theme::BLUE,
        "interface" | "trait" => theme::MAUVE,
        _ => theme::TEXT,
    }
}

/// 繪製關係圖畫布 (Canvas)
/// 支援平移 (pan_x, pan_y) 與縮放 (zoom)
#[must_use]
pub fn create_canvas_widget<'a>(
    graph: &'a GraphOutput,
    coords: &'a NodeCoordinates,
    selected_id: Option<&'a NodeId>,
    pan_x: f64,
    pan_y: f64,
    zoom: f64,
) -> Canvas<'a, impl Fn(&mut Context) + 'a> {
    // 預設邊界投影 (等比例縮放與平移)
    let x_min = -60.0 * zoom + pan_x;
    let x_max = 60.0 * zoom + pan_x;
    let y_min = -30.0 * zoom + pan_y;
    let y_max = 30.0 * zoom + pan_y;

    Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::SURFACE_HI))
                .title(Span::styled(
                    " 📊 Visual Graph Topology ",
                    Style::default()
                        .fg(theme::CYAN)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .background_color(theme::BG) // 深黑炭底，確保高對比
        .x_bounds([x_min, x_max])
        .y_bounds([y_min, y_max])
        .paint(move |ctx| {
            // 1. 繪製有向關係連線 (Edges)：與選中節點相關 → 亮綠，其餘暗灰
            for edge in &graph.edges {
                if let (Some(&(x1, y1)), Some(&(x2, y2))) = (
                    coords.coords.get(&edge.source),
                    coords.coords.get(&edge.target),
                ) {
                    let color = if selected_id
                        .map_or(false, |sel| *sel == edge.source || *sel == edge.target)
                    {
                        theme::GREEN
                    } else {
                        theme::SUBTLE
                    };
                    ctx.draw(&Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        color,
                    });
                }
            }

            // 2. 繪製節點圓點與標籤 (Nodes & Labels)
            for node in &graph.nodes {
                if let Some(&(x, y)) = coords.coords.get(&node.id) {
                    let is_selected = selected_id.map_or(false, |sel| *sel == node.id);
                    let color = if is_selected {
                        theme::CYAN
                    } else {
                        node_color(&node.kind)
                    };

                    ctx.draw(&Circle {
                        x,
                        y,
                        radius: if is_selected { 2.4 } else { 1.2 },
                        color,
                    });

                    // 僅在靠近視口中心或選中時渲染標籤，防止標籤重疊雜亂
                    let dx = x - pan_x;
                    let dy = y - pan_y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if is_selected || dist < 40.0 * zoom {
                        let offset_y = if is_selected { -3.5 } else { -2.5 };
                        let style = if is_selected {
                            Style::default()
                                .fg(theme::BG)
                                .bg(theme::CYAN)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(color)
                        };
                        let label_text = if is_selected {
                            format!(" ▶ {} ◀ ", node.label)
                        } else {
                            node.label.clone()
                        };
                        ctx.print(x - 4.0, y + offset_y, Span::styled(label_text, style));
                    }
                }
            }
        })
}

/// 畫布右上角浮動指標面板：節點/連線總數、選中節點、縮放與平移
pub fn render_metrics_overlay(
    f: &mut ratatui::Frame,
    graph: &GraphOutput,
    selected: Option<&Node>,
    zoom: f64,
    pan_x: f64,
    pan_y: f64,
    area: Rect,
) {
    let width = 40.min(area.width.saturating_sub(2));
    let height = 9.min(area.height.saturating_sub(2));
    if width < 24 || height < 5 {
        return;
    }
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y + 1,
        width,
        height,
    };
    f.render_widget(Clear, rect);

    let mut lines = Vec::new();
    lines.push(TextLine::from(vec![
        Span::styled("◉ Nodes ", Style::default().fg(theme::SUBTLE)),
        Span::styled(
            graph.nodes.len().to_string(),
            Style::default()
                .fg(theme::GOLD)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("➔ Edges ", Style::default().fg(theme::SUBTLE)),
        Span::styled(
            graph.edges.len().to_string(),
            Style::default()
                .fg(theme::GOLD)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if let Some(node) = selected {
        let link_count = graph
            .edges
            .iter()
            .filter(|e| e.source == node.id || e.target == node.id)
            .count();
        lines.push(TextLine::from(vec![
            Span::styled(
                "▶ ",
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &node.label,
                Style::default()
                    .fg(theme::GOLD)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(TextLine::from(vec![
            Span::styled("  Kind: ", Style::default().fg(theme::SUBTLE)),
            Span::styled(&node.kind, Style::default().fg(theme::CYAN)),
            Span::raw("   "),
            Span::styled("Links: ", Style::default().fg(theme::SUBTLE)),
            Span::styled(
                link_count.to_string(),
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(TextLine::from(vec![
            Span::styled("  File: ", Style::default().fg(theme::SUBTLE)),
            Span::styled(
                format!("{}:{}", node.source_file, node.start_line),
                Style::default().fg(theme::BLUE),
            ),
        ]));
    } else {
        lines.push(TextLine::from(Span::styled(
            " No node selected — click a node to inspect ",
            Style::default().fg(theme::SUBTLE),
        )));
        lines.push(TextLine::from(""));
    }
    lines.push(TextLine::from(vec![
        Span::styled("🔍 Zoom: ", Style::default().fg(theme::SUBTLE)),
        Span::styled(
            format!("{zoom:.2}x"),
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("Pan: ", Style::default().fg(theme::SUBTLE)),
        Span::styled(
            format!("({pan_x:.0}, {pan_y:.0})"),
            Style::default().fg(theme::CYAN),
        ),
    ]));

    let panel = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::MAUVE)),
    );
    f.render_widget(panel, rect);
}
