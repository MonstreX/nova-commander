//! Renders the embedded shell's VT screen into a console area.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Clear, Paragraph};

use crate::terminal::ShellView;

/// The shell's whole screen (terminal view, running command). Returns the shell's cursor.
pub fn draw_screen(frame: &mut Frame, area: Rect, view: Option<&ShellView>) -> Option<Position> {
    frame.render_widget(Clear, area);
    let Some(view) = view else {
        draw_not_running(frame, area);
        return None;
    };
    draw_rows(frame, area, view.screen, &view.rows);
    let (row, col) = view.cursor()?;
    (!view.screen.hide_cursor() && row < area.height && col < area.width)
        .then(|| Position::new(area.x + col, area.y + row))
}

/// The shell lines just above its cursor line (the prompt, which our command line replaces), for the
/// strip uncovered by Ctrl+Up below the panels.
pub fn draw_above_cursor(frame: &mut Frame, area: Rect, view: Option<&ShellView>) {
    frame.render_widget(Clear, area);
    let Some(view) = view else {
        return;
    };
    let cursor = view
        .cursor()
        .map_or(view.rows.len(), |(row, _)| usize::from(row));
    // Only the lines above the prompt, bottom-aligned; with a fresh shell the top stays blank.
    let above = &view.rows[..cursor];
    let shown = above.len().min(usize::from(area.height));
    let height = u16::try_from(shown).unwrap_or(area.height);
    let strip = Rect::new(area.x, area.y + area.height - height, area.width, height);
    draw_rows(frame, strip, view.screen, &above[above.len() - shown..]);
}

fn draw_not_running(frame: &mut Frame, area: Rect) {
    let message = Paragraph::new(
        "The shell is not running - press any key to start it, Alt+Shift+T to return.",
    )
    .style(Style::new().fg(Color::DarkGray));
    frame.render_widget(message, area);
}

/// Draws screen `rows` one under another from the top of `area`.
fn draw_rows(frame: &mut Frame, area: Rect, screen: &vt100::Screen, rows: &[u16]) {
    let (_, cols) = screen.size();
    let width = area.width.min(cols.get());
    let buffer = frame.buffer_mut();
    for (y, &row) in (0..area.height).zip(rows) {
        for x in 0..width {
            let Some(cell) = screen.cell(row, x) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let symbol = if cell.has_contents() {
                cell.contents()
            } else {
                " "
            };
            if let Some(target) = buffer.cell_mut((area.x + x, area.y + y)) {
                target.set_symbol(symbol).set_style(cell_style(cell));
            }
        }
    }
}

fn cell_style(cell: &vt100::Cell) -> Style {
    let mut style = Style::new()
        .fg(convert(cell.fgcolor()))
        .bg(convert(cell.bgcolor()));
    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }
    style
}

fn convert(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
    }
}
