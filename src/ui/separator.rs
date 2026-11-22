//! A line across a dialog's box with a title in its middle (FAR's `DIF_SEPARATOR` / `DIF_SEPARATOR2` with
//! a text, `message.cpp`'s `\1` / `\2` lines): `╟──── Total ────╢`, `╠════ Total ════╣`.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use super::theme;
use crate::app::form::Control;

/// A form's `Control::TitledSeparator` at `at` (its title from there when it is `left`).
pub fn draw_item(buffer: &mut Buffer, inside: Rect, at: Position, control: &Control, style: Style) {
    if let Control::TitledSeparator {
        double,
        title,
        left,
    } = control
    {
        draw(
            buffer,
            inside,
            at.y,
            (*double, title, left.then_some(at.x)),
            style,
        );
    }
}

/// The line across `inside` at row `y`, single or `double`; `title` in spaces in its middle when it fits,
/// or from column `title_x` (FAR's separator with its `X1` given).
pub fn draw(
    buffer: &mut Buffer,
    inside: Rect,
    y: u16,
    (double, title, title_x): (bool, &str, Option<u16>),
    style: Style,
) {
    let frame = theme::look().dialog_frame;
    let width = usize::from(inside.width);
    let line = if double {
        frame.double_separator(width)
    } else {
        frame.separator(width)
    };
    buffer.set_string(inside.x, y, line, style);
    if title.is_empty() {
        return;
    }
    let mut text = title.to_string();
    if !text.starts_with(' ') {
        text.insert(0, ' ');
    }
    if !text.ends_with(' ') {
        text.push(' ');
    }
    let length = text.chars().count();
    if let Some(x) = title_x {
        let room = usize::from(inside.right().saturating_sub(1).saturating_sub(x));
        let shown: String = text.chars().take(room).collect();
        buffer.set_string(x, y, shown, style);
    } else if length < width {
        let x = inside.x + u16::try_from((width - length) / 2).unwrap_or(0);
        buffer.set_string(x, y, text, style);
    }
}

/// FAR's `DI_SINGLEBOX` at `at`, `width` columns and `rows` rows: a frame of single lines, or one row high
/// a line across; `title` in spaces in its top's middle, cut to the room it has (`dialog.cpp`'s
/// `ShowDialog`).
pub fn draw_box(
    buffer: &mut Buffer,
    at: Position,
    (width, rows): (u16, u16),
    title: &str,
    style: Style,
) {
    let wide = usize::from(width);
    // A plugin's box may reach past the screen: what is outside is not drawn (the buffer would panic).
    let mut put = |x: u16, y: u16, text: &str| {
        if buffer.area.contains(Position { x, y }) {
            buffer.set_string(x, y, text, style);
        }
    };
    if rows <= 1 {
        put(at.x, at.y, &"─".repeat(wide));
    } else {
        let line = "─".repeat(wide.saturating_sub(2));
        put(at.x, at.y, &format!("┌{line}┐"));
        for row in 1..rows.saturating_sub(1) {
            let y = at.y.saturating_add(row);
            put(at.x, y, "│");
            put(at.x.saturating_add(width.saturating_sub(1)), y, "│");
        }
        put(at.x, at.y.saturating_add(rows - 1), &format!("└{line}┘"));
    }
    if !buffer.area.contains(at) {
        return;
    }
    if title.is_empty() || wide <= 2 {
        return;
    }
    let mut text: String = title.chars().take(wide - 2).collect();
    if text.chars().count() < wide - 2 {
        text = format!(" {text} ");
    }
    let length = u16::try_from(text.chars().count()).unwrap_or(width);
    buffer.set_string(at.x + width.saturating_sub(length) / 2, at.y, text, style);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(width: u16, rows: u16, title: &str) -> Vec<String> {
        let area = Rect::new(0, 0, width, rows);
        let mut buffer = Buffer::empty(area);
        draw_box(
            &mut buffer,
            Position::new(0, 0),
            (width, rows),
            title,
            Style::default(),
        );
        (0..rows)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    #[test]
    fn a_box_one_row_high_is_a_line_with_its_title_in_the_middle() {
        assert_eq!(boxed(15, 1, "Mode"), vec!["──── Mode ─────"]);
        assert_eq!(boxed(6, 1, "Transfer"), vec!["─Tran─"], "cut to the room");
    }

    #[test]
    fn a_taller_box_is_a_frame() {
        assert_eq!(boxed(8, 3, "Ab"), vec!["┌─ Ab ─┐", "│      │", "└──────┘"]);
    }

    #[test]
    fn a_box_past_the_screen_is_cut_not_a_panic() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 2));
        draw_box(
            &mut buffer,
            Position::new(2, 0),
            (10, 9),
            "T",
            Style::default(),
        );
        draw_box(
            &mut buffer,
            Position::new(1, 5),
            (3, 3),
            "T",
            Style::default(),
        );
        draw_box(
            &mut buffer,
            Position::new(9, 0),
            (3, 1),
            "T",
            Style::default(),
        );
        assert_eq!(buffer[(2, 0)].symbol(), "┌");
        assert_eq!(buffer[(2, 1)].symbol(), "│");
    }

    fn row(double: bool, title: &str, width: u16) -> String {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        draw(
            &mut buffer,
            area,
            0,
            (double, title, None),
            Style::default(),
        );
        (0..width).map(|x| buffer[(x, 0)].symbol()).collect()
    }

    #[test]
    fn a_title_from_a_column_stays_inside_the_box() {
        let area = Rect::new(0, 0, 12, 1);
        let mut buffer = Buffer::empty(area);
        draw(
            &mut buffer,
            area,
            0,
            (false, "Details", Some(2)),
            Style::default(),
        );
        let row: String = (0..12).map(|x| buffer[(x, 0)].symbol()).collect();
        assert_eq!(row, "╟─ Details ╢");
    }

    #[test]
    fn the_title_sits_in_the_lines_middle_in_spaces_when_it_fits() {
        assert_eq!(row(false, "Total", 15), "╟─── Total ───╢");
        assert_eq!(row(true, " Error ", 15), "╠═══ Error ═══╣");
        assert_eq!(row(true, "", 6), "╠════╣");
        assert_eq!(
            row(false, "Much too long", 8),
            "╟──────╢",
            "no room: the line alone"
        );
    }
}
