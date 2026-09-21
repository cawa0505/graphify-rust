//! 通用層（Chrome/split、事件日誌、Flash 藥丸）委派 rust-tuikit；
//! ActionTag/ActiveTab 屬專案語意，留在本 crate。行為與原版逐位一致。

use crate::ui::{ActiveTab, theme};
use ratatui::{layout::Rect, style::Color};
use rust_tuikit::flash::Pill;

pub use rust_tuikit::layout::Chrome;
pub use rust_tuikit::log::{LogEntry, push_log, render_event_log};

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
    Log,
    Refresh,
}

/// kit 泛型 `Flash<K>` 綁定專案 `ActionTag`（呼叫端介面與原版一致）
pub type Flash = rust_tuikit::flash::Flash<ActionTag>;

/// 版面分割。`show_event_log = false` 時主視圖佔滿；true 時 70% / 日誌 30%
#[must_use]
pub fn split(area: Rect, show_event_log: bool) -> Chrome {
    rust_tuikit::layout::split(area, if show_event_log { Some(30) } else { None })
}

#[allow(clippy::missing_const_for_fn)] // Color 為非 const Type，與原版 pill() 相同限制
const fn pill(
    key: &'static str,
    label: &'static str,
    color: Color,
    action: ActionTag,
) -> Pill<ActionTag> {
    Pill {
        key,
        label,
        color,
        action,
    }
}

/// 快捷鍵 Footer：區塊式藥丸按鈕，執行對應動作時整顆反白閃爍
pub fn render_footer(f: &mut ratatui::Frame, tab: ActiveTab, flash: &Flash, area: Rect) {
    let pills: &[Pill<ActionTag>] = match tab {
        ActiveTab::Explorer => &[
            pill("Tab", "View", theme::CYAN, ActionTag::Nav),
            pill("j/k", "Nav", theme::CYAN, ActionTag::Nav),
            pill("/", "Filter", theme::GOLD, ActionTag::Filter),
            pill("t", "Trace", theme::GOLD, ActionTag::Trace),
            pill("p", "Plugins", theme::MAUVE, ActionTag::Nav),
            pill("g/Enter", "Code", theme::GREEN, ActionTag::Edit),
            pill("e", "Event Log", theme::MAUVE, ActionTag::Log),
            pill("q", "Quit", theme::RED, ActionTag::Quit),
        ][..],
        ActiveTab::VisualGraph => &[
            pill("Drag", "Pan", theme::GREEN, ActionTag::Pan),
            pill("Scroll", "Zoom", theme::CYAN, ActionTag::Zoom),
            pill("Click", "Select", theme::GREEN, ActionTag::Select),
            pill("R-Click", "Inspect", theme::GOLD, ActionTag::Inspect),
            pill("p", "Plugins", theme::MAUVE, ActionTag::Nav),
            pill("r", "Reset", theme::CYAN, ActionTag::Reset),
            pill("e", "Event Log", theme::MAUVE, ActionTag::Log),
            pill("q", "Quit", theme::RED, ActionTag::Quit),
        ][..],
        ActiveTab::Architecture => &[
            pill("j/k", "Nav", theme::CYAN, ActionTag::Nav),
            pill("Enter", "Open", theme::GREEN, ActionTag::Select),
            pill("Esc", "Back", theme::GOLD, ActionTag::Nav),
            pill("y", "Rescan", theme::MAUVE, ActionTag::Nav),
            pill("e", "Event Log", theme::MAUVE, ActionTag::Log),
            pill("q", "Quit", theme::RED, ActionTag::Quit),
        ][..],
    };

    rust_tuikit::flash::render_pills(f, pills, flash, area);
}
