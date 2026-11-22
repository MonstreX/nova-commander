//! The list above the command line while typing (FAR's completion menu: at the edit's left, at most 8
//! lines, a thin frame, the source as its title).

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Clear;

use super::format;
use super::theme;
use crate::app::{App, Completion, CompletionItem};

/// FAR's `CBoxMaxHeight`.
const MOST_LINES: usize = 8;
/// As wide as this at least (FAR's `m_Where.left + 20`).
const LEAST_WIDTH: u16 = 20;

/// The box above `line` (the command line's row), its left at `left`, no higher than `top`.
fn box_rect(line: Rect, left: u16, top: u16, lines: usize) -> Option<Rect> {
    let free = line.y.saturating_sub(top);
    let rows = (u16::try_from(lines.min(MOST_LINES)).ok()? + 2).min(free);
    if rows < 3 {
        return None;
    }
    let left = left.min(line.right().saturating_sub(LEAST_WIDTH));
    Some(Rect::new(left, line.y - rows, line.right() - left, rows))
}

/// The first line to show so that the selected one is on screen; it stays while that one is
/// (FAR's `TopPos`).
fn first_shown(top: &std::cell::Cell<usize>, selected: usize, lines: usize, rows: usize) -> usize {
    crate::app::keep_in_view(top, selected, lines, rows)
}

fn frame_line(
    buffer: &mut Buffer,
    area: Rect,
    y: u16,
    ends: (char, char),
    title: &str,
    style: Style,
) {
    let inner = usize::from(area.width).saturating_sub(2);
    let title = if title.is_empty() {
        String::new()
    } else {
        format!(" {title} ")
    };
    let title: String = title.chars().take(inner).collect();
    let free = inner - title.chars().count();
    let line = format!(
        "{}{}{title}{}{}",
        ends.0,
        "─".repeat(free / 2),
        "─".repeat(free - free / 2),
        ends.1
    );
    buffer.set_string(area.x, y, line, style);
}

fn draw_list(frame: &mut Frame, area: Rect, completion: &Completion) -> usize {
    frame.render_widget(Clear, area);
    let buffer = frame.buffer_mut();
    let normal = theme::dialog(false);
    frame_line(buffer, area, area.y, ('┌', '┐'), completion.title, normal);
    frame_line(buffer, area, area.bottom() - 1, ('└', '┘'), "", normal);
    let inner = usize::from(area.width).saturating_sub(2);
    let rows = usize::from(area.height) - 2;
    let first = first_shown(
        &completion.top,
        completion.selected,
        completion.items.len(),
        rows,
    );
    let shown = completion.items.iter().enumerate().skip(first);
    for (y, (index, item)) in (area.y + 1..area.bottom() - 1).zip(shown) {
        if let CompletionItem::Group(title) = item {
            frame_line(buffer, area, y, ('├', '┤'), title, normal);
            continue;
        }
        let text = match item {
            CompletionItem::Text { text, .. } => text.as_str(),
            _ => "",
        };
        let style = if index == completion.selected {
            theme::menu_highlight()
        } else {
            normal
        };
        buffer.set_string(area.x, y, "│", normal);
        buffer.set_string(area.x + 1, y, format::fit(text, inner), style);
        buffer.set_string(area.right() - 1, y, "│", normal);
    }
    first
}

/// Draws the list when it is up; returns where, for the mouse. `left`: where the text of the command
/// line begins.
pub fn draw(
    frame: &mut Frame,
    line: Rect,
    left: u16,
    top: u16,
    app: &App,
) -> Option<crate::app::MenuArea> {
    let completion = app.completion.as_ref().filter(|list| list.is_shown())?;
    let area = box_rect(line, left, top, completion.items.len())?;
    let first = draw_list(frame, area, completion);
    Some(super::menu_area(area, area, first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_stands_above_the_line_and_grows_to_eight_lines() {
        let line = Rect::new(0, 20, 80, 1);
        assert_eq!(box_rect(line, 10, 0, 3), Some(Rect::new(10, 15, 70, 5)));
        assert_eq!(box_rect(line, 10, 0, 30), Some(Rect::new(10, 10, 70, 10)));
    }

    #[test]
    fn a_long_prompt_leaves_the_box_its_least_width() {
        let line = Rect::new(0, 20, 80, 1);
        assert_eq!(box_rect(line, 75, 0, 3), Some(Rect::new(60, 15, 20, 5)));
    }

    #[test]
    fn with_no_room_above_the_line_there_is_no_box() {
        assert_eq!(box_rect(Rect::new(0, 2, 80, 1), 0, 0, 8), None);
    }

    #[test]
    fn the_box_shrinks_to_the_rows_there_are_and_leaves_the_status_row() {
        let line = Rect::new(0, 6, 80, 1);
        assert_eq!(box_rect(line, 0, 1, 8), Some(Rect::new(0, 1, 80, 5)));
    }

    #[test]
    fn the_selected_line_is_kept_on_screen() {
        let fresh =
            |selected, lines, rows| first_shown(&std::cell::Cell::new(0), selected, lines, rows);
        assert_eq!(fresh(0, 20, 8), 0);
        assert_eq!(fresh(7, 20, 8), 0);
        assert_eq!(fresh(8, 20, 8), 1);
        assert_eq!(fresh(19, 20, 8), 12);
        assert_eq!(fresh(2, 3, 8), 0);
        assert_eq!(fresh(5, 20, 3), 3, "a box shrunk to three lines");
        // Kept while the selected one is on screen, moved when it leaves.
        let top = std::cell::Cell::new(12);
        assert_eq!(first_shown(&top, 14, 20, 8), 12);
        assert_eq!(first_shown(&top, 9, 20, 8), 9);
    }
}
