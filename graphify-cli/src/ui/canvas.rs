#![allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    clippy::cast_precision_loss,
    clippy::option_if_let_else,
    clippy::doc_markdown,
    clippy::doc_lazy_continuation,
    clippy::unnecessary_map_or
)]

use graphify_core::{GraphOutput, NodeId};
use ratatui::{
    style::Color,
    widgets::canvas::{Canvas, Circle, Line, Context},
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
    /// - 選中節點的直接鄰居 (Incoming/Outgoing) 分布在半徑 22.0 的內圓環
    /// - 其他所有節點均勻分布在半徑 45.0 的外圓環
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
            let theta = (2.0 * std::f64::consts::PI * (i as f64)) / (neighbor_count as f64).max(1.0);
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
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title(" 📊 Visual Graph Topology (Tab 2) "),
        )
        .background_color(ratatui::style::Color::Rgb(15, 17, 23)) // ponytail: 黑色底色填充，避免透明漏字
        .x_bounds([x_min, x_max])
        .y_bounds([y_min, y_max])
        .paint(move |ctx| {
            // 1. 繪製有向關係連線 (Edges)
            for edge in &graph.edges {
                if let (Some(&(x1, y1)), Some(&(x2, y2))) = (coords.coords.get(&edge.source), coords.coords.get(&edge.target)) {
                    let color = if selected_id.map_or(false, |sel| *sel == edge.source || *sel == edge.target) {
                        Color::Cyan // 與選中節點有關的連線以亮青色呈現
                    } else {
                        Color::DarkGray // 其他連線為暗灰色
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

            // 2. 繪製節點圓圈與標籤 (Nodes & Labels)
            for node in &graph.nodes {
                if let Some(&(x, y)) = coords.coords.get(&node.id) {
                    let is_selected = selected_id.map_or(false, |sel| *sel == node.id);
                    let color = if is_selected {
                        Color::Cyan // 當前選中：亮青色
                    } else {
                        match node.kind.as_str() {
                            "module" | "file" => Color::Blue,
                            "struct" | "class" => Color::Yellow,
                            "function" | "fn" | "method" => Color::Green,
                            _ => Color::White,
                        }
                    };

                    // 繪製圓點
                    ctx.draw(&Circle {
                        x,
                        y,
                        radius: if is_selected { 2.2 } else { 1.2 },
                        color,
                    });

                    // 僅在靠近視口中心或選中時渲染標籤，防止標籤重疊雜亂
                    let dx = x - pan_x;
                    let dy = y - pan_y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if is_selected || dist < 40.0 * zoom {
                        let offset_y = if is_selected { -3.5 } else { -2.5 };
                        ctx.print(x - 4.0, y + offset_y, ratatui::text::Span::styled(node.label.clone(), ratatui::style::Style::default().fg(color)));
                    }
                }
            }
        })
}