//! The panel's path in its top frame (FAR's `ShowFileList`): shortened to fit, clear of the clock.

use crate::app::text_cut::truncate_path;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::panel::{set_text, to_u16};
use super::theme;

/// The path centred in the top frame (FAR's `ShowFileList`): shortened to fit, and moved or limited only
/// when it would run into the clock at `clock_x`.
pub(super) fn draw_title(buffer: &mut Buffer, area: Rect, path: &str, focused: bool, clock_x: u16) {
    let path_length = to_u16(path.chars().count());
    let mut title_right = area.right().saturating_sub(1);
    if area.x + path_length + 2 >= clock_x {
        title_right = title_right.min(clock_x);
    }
    let max = usize::from(title_right.saturating_sub(area.x + 1));
    if max < 2 {
        return;
    }
    let title = format!(" {} ", truncate_path(path, max - 2));
    let length = to_u16(title.chars().count());
    let mut x = area.x + 1 + title_right.saturating_sub(area.x + length) / 2;
    if x + length > clock_x {
        x = clock_x.saturating_sub(length).max(area.x + 1);
    }
    set_text(buffer, x, area.y, &title, theme::panel_title(focused));
}
