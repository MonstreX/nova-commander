//! The guest shell (Alt+Shift+T) with the program's own top and bottom rows around it, so it is clear
//! where one is: on top the clock (as over the panels), at the bottom in the
//! key bar's colours what the shell is, its process, whether it waits for a command, and the way back
//! to the panels. FAR has no guest shell; its hidden panels keep the clock and the key bar too.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Clear;

use super::{draw_clock, terminal, theme};
use crate::terminal::Session;

/// Draws the frame and the shell between its rows; returns the shell's cursor.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    guest: Option<&Session>,
    clock: bool,
) -> Option<Position> {
    let shell_area = shell_area(area);
    if area.height < 3 {
        let view = guest.map(Session::view);
        return terminal::draw_screen(frame, area, view.as_ref());
    }
    let top = Rect::new(area.x, area.y, area.width, 1);
    let bottom = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    draw_top(frame, top, clock);
    draw_bottom(frame, bottom, guest);
    let view = guest.map(Session::view);
    terminal::draw_screen(frame, shell_area, view.as_ref())
}

/// Where the shell's screen is in the guest's view.
pub fn shell_area(area: Rect) -> Rect {
    if area.height < 3 {
        area
    } else {
        Rect::new(area.x, area.y + 1, area.width, area.height - 2)
    }
}

/// Only the clock on the right, as FAR keeps it over its user screen: the rest of the row is empty
/// console, coloured only under the clock. The shell shows its folder itself.
fn draw_top(frame: &mut Frame, area: Rect, clock: bool) {
    frame.render_widget(Clear, area);
    draw_clock(frame, area, clock);
}

/// What runs and how to leave it, in the key bar's colours: names in the key colour, values in the
/// label colour.
fn draw_bottom(frame: &mut Frame, area: Rect, guest: Option<&Session>) {
    let buffer = frame.buffer_mut();
    buffer.set_style(area, theme::key_label());
    let mut x = area.x;
    for (text, style) in status_parts(guest) {
        if x >= area.right() {
            break;
        }
        let room = usize::from(area.right() - x);
        let (next, _) = buffer.set_stringn(x, area.y, &text, room, style);
        x = next;
    }
}

/// The bottom row as (text, style) pieces.
fn status_parts(guest: Option<&Session>) -> Vec<(String, Style)> {
    let key = theme::key_number();
    let label = theme::key_label();
    let mut parts = Vec::new();
    if let Some(guest) = guest {
        parts.push((format!(" {} ", guest.shell().title()), label));
        if let Some(pid) = guest.process_id() {
            parts.push(("PID".to_string(), key));
            parts.push((format!(" {pid} "), label));
        }
        let state = if guest.at_prompt() {
            "waits for a command"
        } else {
            "a program runs"
        };
        parts.push((format!(" {state} "), label));
    }
    parts.push(("Alt+Shift+T".to_string(), key));
    parts.push((" Panels ".to_string(), label));
    if guest.is_none_or(Session::at_prompt) {
        parts.push(("Esc".to_string(), key));
        parts.push((" Panels ".to_string(), label));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_shell_the_way_back_is_still_shown() {
        let text: String = status_parts(None)
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        assert!(text.contains("Alt+Shift+T Panels"), "{text}");
        assert!(text.contains("Esc Panels"), "{text}");
    }
}
