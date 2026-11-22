//! FAR's quick search box (`FastFind::ShowBorder`, `InitPositionAndSize`): 22×3 double box in dialog
//! colours on the active panel's bottom frame, " Search " in the top frame, the typed text in an edit
//! field.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use super::theme;
use super::vmenu::{set_text, to_u16};
use crate::app::QuickSearch;

const WIDTH: u16 = 22;
const FIELD: usize = 18;

/// `panel` is the active panel's rectangle; returns the cursor position in the field.
pub fn draw(frame: &mut Frame, area: Rect, panel: Rect, search: &QuickSearch) -> Position {
    let screen_right = area.right().saturating_sub(1);
    let screen_bottom = area.bottom().saturating_sub(1);
    let x = (panel.x + 9).min(screen_right.saturating_sub(WIDTH));
    let y = (panel.bottom().saturating_sub(1)).min(screen_bottom.saturating_sub(2));
    let buffer = frame.buffer_mut();
    let frame_style = theme::dialog(false);
    let lines = theme::look().dialog_frame.set();
    let inner = usize::from(WIDTH - 2);
    let rows = [
        (lines.top_left, lines.horizontal_top, lines.top_right),
        (lines.vertical_left, " ", lines.vertical_right),
        (
            lines.bottom_left,
            lines.horizontal_bottom,
            lines.bottom_right,
        ),
    ];
    for (row, (left, middle, right)) in (0..).zip(rows) {
        let text = format!("{left}{}{right}", middle.repeat(inner));
        set_text(buffer, x, y + row, &text, frame_style);
    }
    set_text(buffer, x + 7, y, " Search ", frame_style);
    // The field shows the end of a long text.
    let length = search.text.chars().count();
    let shown: String = search
        .text
        .chars()
        .skip(length.saturating_sub(FIELD - 1))
        .collect();
    let field = format!("{shown:<FIELD$}");
    set_text(buffer, x + 2, y + 1, &field, theme::input_field());
    Position::new(x + 2 + to_u16(shown.chars().count()), y + 1)
}
