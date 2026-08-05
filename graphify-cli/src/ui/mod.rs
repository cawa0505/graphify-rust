pub mod canvas;
pub mod layout;
pub mod modal;
pub mod theme;

/// 主視圖分頁
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Explorer,
    VisualGraph,
}
