//! Full-screen viewer (F3) and editor (F4).

use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use super::theme;
use crate::app::{EditorScreen, Viewer};
use crate::platform::input::Modifiers;

/// Status line, text, key bar; the status line and the key bar as shown (FAR's `ShowTitleBar` with
/// more than one row, `ShowKeyBar` with more than two), an empty rectangle when not.
fn split(area: Rect, (title, key_bar): (bool, bool)) -> (Rect, Rect, Rect) {
    let title = u16::from(title && area.height > 1);
    let key_bar = u16::from(key_bar && area.height > 2);
    let status = Rect::new(area.x, area.y, area.width, title);
    let body = Rect::new(
        area.x,
        area.y + title,
        area.width,
        area.height.saturating_sub(title + key_bar),
    );
    let keys = Rect::new(
        area.x,
        area.bottom().saturating_sub(key_bar),
        area.width,
        key_bar,
    );
    (status, body, keys)
}

/// FAR's file viewer (`FileViewer::Show`): its status line at the top (the clock at its right when
/// shown), the text, the key bar for the modifiers `held`; the byte editing's cursor (ADR-0027).
pub fn draw_viewer(
    frame: &mut Frame,
    area: Rect,
    viewer: &Viewer,
    (held, clock, switch): (Modifiers, bool, bool),
) -> Option<Position> {
    let bars = (viewer.settings.show_title_bar, viewer.settings.show_key_bar);
    let (status, body, keys) = split(area, bars);
    draw_title(frame, status, clock, |width, clock| {
        viewer.status(width, clock)
    });
    let text = Rect {
        width: viewer.text_width(body.width),
        ..body
    };
    let buffer = frame.buffer_mut();
    super::quick_view::draw_text(buffer, text, viewer.text());
    let paints = viewer.paints(text.width, text.height);
    paint_rows(buffer, text, &paints);
    // What a search found stays on top of the colours.
    let screen = viewer.text().screen(text.width, text.height);
    if let Some(screen) = &screen {
        mark_rows(buffer, text, &screen.marks);
        mark_spans(buffer, text, &screen.spans);
    }
    if viewer.arrows {
        draw_arrows(buffer, text, &viewer.overflow(text.width, text.height));
    }
    if viewer.scrollbar
        && let Some((start, end, size)) = viewer.extent(text.width, text.height)
    {
        draw_scrollbar(buffer, body, (start, end, size));
    }
    if keys.height > 0 {
        let mut labels =
            super::keybar::viewer_labels(held, viewer.labels(), viewer.shift_f2_label());
        hide_screens(&mut labels, held, switch);
        super::keybar::draw(frame, keys, &labels);
    }
    let (row, col) = screen?.cursor?;
    let (x, y) = (u16::try_from(col).ok()?, u16::try_from(row).ok()?);
    Some(Position::new(text.x + x, text.y + y))
}

/// A modal viewer or editor has no F12 "Screen" and no Alt+F11 "ViewHs" (FAR's `InitKeyBar` without
/// `GetCanLoseFocus`).
fn hide_screens(labels: &mut [&str; 12], held: Modifiers, switch: bool) {
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    if !switch && held == Modifiers::default() {
        labels[11] = "";
    }
    if !switch && held == alt {
        labels[10] = "";
    }
}

/// The status line `status(width, clock's width)` with the clock at its right; nothing when hidden.
fn draw_title(frame: &mut Frame, area: Rect, clock: bool, status: impl Fn(usize, usize) -> String) {
    if area.height == 0 {
        return;
    }
    let clock_width = if clock {
        super::format::clock(std::time::SystemTime::now())
            .chars()
            .count()
    } else {
        0
    };
    draw_status(frame, area, &status(usize::from(area.width), clock_width));
    super::draw_clock(frame, area, clock);
}

/// FAR's line numbers (`Editor::ShowEditor`): each row's right-aligned with a space after it, from
/// `top` for `rows` rows (none when the lines are not counted).
fn draw_numbers(buffer: &mut ratatui::buffer::Buffer, column: Rect, (top, rows): (u64, usize)) {
    let style = theme::style(theme::Role::TextNumbers);
    buffer.set_style(column, style);
    if top == 0 {
        return;
    }
    let width = usize::from(column.width).saturating_sub(1);
    for (y, row) in (0..column.height).zip(0..rows) {
        let number = format!("{:>width$} ", top + row as u64);
        buffer.set_string(column.x, column.y + y, number, style);
    }
}

/// The rows' marked cells (what a search found) in FAR's `COL_VIEWERSELECTEDTEXT`.
fn mark_rows(buffer: &mut ratatui::buffer::Buffer, body: Rect, marks: &[Option<(usize, usize)>]) {
    for (y, mark) in (0..body.height).zip(marks) {
        if let Some((from, to)) = mark {
            let from = u16::try_from(*from).unwrap_or(u16::MAX).min(body.width);
            let to = u16::try_from(*to).unwrap_or(u16::MAX).min(body.width);
            let marked = Rect::new(body.x + from, body.y + y, to.saturating_sub(from), 1);
            buffer.set_style(marked, theme::text_selected());
        }
    }
}

/// More cells of each row marked as the rows' marks are (the hex mode's bytes changed, ADR-0027).
fn mark_spans(buffer: &mut ratatui::buffer::Buffer, body: Rect, spans: &[Vec<(usize, usize)>]) {
    for (y, row) in (0..body.height).zip(spans) {
        let line = Rect::new(body.x, body.y + y, body.width, 1);
        for mark in row {
            mark_rows(buffer, line, &[Some(*mark)]);
        }
    }
}

/// FAR's `«` at a row's left and `»` at its right where its line goes on (`COL_VIEWERARROWS`).
fn draw_arrows(buffer: &mut ratatui::buffer::Buffer, body: Rect, overflow: &[(bool, bool)]) {
    if body.width == 0 {
        return;
    }
    for (y, &(left, right)) in (0..body.height).zip(overflow) {
        if right {
            buffer.set_string(
                body.right() - 1,
                body.y + y,
                "»",
                theme::style(theme::Role::TextArrows),
            );
        }
        if left {
            buffer.set_string(
                body.x,
                body.y + y,
                "«",
                theme::style(theme::Role::TextArrows),
            );
        }
    }
}

/// FAR's `DrawScrollbar`: the bar down the room's last column for bytes `start..end` of `size`.
fn draw_scrollbar(
    buffer: &mut ratatui::buffer::Buffer,
    body: Rect,
    (start, end, size): (u64, u64, u64),
) {
    let rows = usize::from(body.height);
    let as_cells = |bytes: u64| usize::try_from(bytes).unwrap_or(usize::MAX);
    let bar = super::form::scroll_bar(rows, as_cells(start), as_cells(end), as_cells(size).max(1));
    for (y, ch) in (0..body.height).zip(bar) {
        let style = theme::style(theme::Role::TextScrollbar);
        buffer.set_string(
            body.right().saturating_sub(1),
            body.y + y,
            ch.to_string(),
            style,
        );
    }
}

/// The syntax highlighting's colours laid over the rows of `body` (ADR-0024 §3).
fn paint_rows(
    buffer: &mut ratatui::buffer::Buffer,
    body: Rect,
    paints: &[Vec<crate::editor::syntax::highlight::Paint>],
) {
    for (y, row) in (0..body.height).zip(paints) {
        for paint in row {
            let Some(style) = theme::syntax(paint.class) else {
                continue;
            };
            let from = u16::try_from(paint.from)
                .unwrap_or(u16::MAX)
                .min(body.width);
            let to = u16::try_from(paint.to).unwrap_or(u16::MAX).min(body.width);
            buffer.set_style(
                Rect::new(body.x + from, body.y + y, to.saturating_sub(from), 1),
                style,
            );
        }
    }
}

/// The code mode's ruler (owner 2026-11-22): the column `ruler` (from 1; 0: none) down the rows, the
/// first column shown being `left`.
fn draw_ruler(buffer: &mut ratatui::buffer::Buffer, body: Rect, ruler: usize, left: usize) {
    let Some(col) = ruler
        .checked_sub(1)
        .and_then(|col| col.checked_sub(left))
        .and_then(|col| u16::try_from(col).ok())
        .filter(|col| *col < body.width)
    else {
        return;
    };
    let style = theme::style(theme::Role::TextRuler);
    buffer.set_style(Rect::new(body.x + col, body.y, 1, body.height), style);
}

/// The code mode's marks: the word under the cursor where it stands, the bracket and its pair.
fn draw_code_marks(
    buffer: &mut ratatui::buffer::Buffer,
    body: Rect,
    shown: &crate::editor::view::edit_view::EditScreen,
) {
    let cell = |row: usize, col: usize| {
        let (x, y) = (u16::try_from(col).ok()?, u16::try_from(row).ok()?);
        (x < body.width && y < body.height).then_some((body.x + x, body.y + y))
    };
    let style = theme::style(theme::Role::TextOccurrence);
    for &(row, from, to) in &shown.words {
        if let Some((x, y)) = cell(row, from) {
            let width = u16::try_from(to - from).unwrap_or(0).min(body.right() - x);
            buffer.set_style(Rect::new(x, y, width, 1), style);
        }
    }
    let style = theme::style(theme::Role::TextPair);
    for &(row, col) in &shown.pairs {
        if let Some((x, y)) = cell(row, col) {
            buffer.set_style(Rect::new(x, y, 1, 1), style);
        }
    }
}

/// FAR's file editor (`FileEditor::Show`): its status line at the top (the clock at its right when
/// shown), the rows with the block, the key bar for the modifiers `held`; where the cursor stands.
pub fn draw_editor(
    frame: &mut Frame,
    area: Rect,
    screen: &EditorScreen,
    (held, clock, switch): (Modifiers, bool, bool),
) -> Option<Position> {
    let bars = (screen.settings.show_title_bar, screen.settings.show_key_bar);
    let (status, body, keys) = split(area, bars);
    draw_title(frame, status, clock, |width, clock| {
        screen.status(width, clock)
    });
    if keys.height > 0 {
        let hex = screen.text.hex_extent(0).is_some();
        let mut labels: [&str; 12] =
            super::keybar::editor_labels(held, screen.next_page_label(), hex);
        hide_screens(&mut labels, held, switch);
        // A plugin's own over them (FAR's `ECTL_SETKEYBAR`).
        for (number, modifiers, label) in &screen.plugin_bars.labels {
            let slot = usize::from(*number).wrapping_sub(1);
            if *modifiers == held
                && let Some(slot) = labels.get_mut(slot)
            {
                *slot = label;
            }
        }
        super::keybar::draw(frame, keys, &labels);
    }
    let numbers = screen.text.number_width().min(body.width);
    let bar = u16::from(screen.text.scrollbar).min(body.width - numbers);
    let whole = body;
    let body = Rect {
        x: body.x + numbers,
        width: body.width - numbers - bar,
        ..body
    };
    screen.text.room.set((body.width, body.height));
    screen.text.origin.set((body.x, body.y));
    let buffer = frame.buffer_mut();
    buffer.set_style(whole, theme::text_body());
    if let Some(error) = &screen.text.failed {
        buffer.set_stringn(
            body.x,
            body.y,
            error,
            usize::from(body.width),
            theme::text_body(),
        );
        return None;
    }
    let Some(shown) = screen.text.screen(body.width, body.height) else {
        if let Some(reading) = screen.text.reading_for() {
            let notice = super::quick_view::reading_notice(reading);
            buffer.set_stringn(
                body.x,
                body.y,
                &notice,
                usize::from(body.width),
                theme::text_body(),
            );
        }
        return None;
    };
    let paints = screen.text.paints(body.width, body.height);
    let width = usize::from(body.width);
    for (y, row) in (0..body.height).zip(&shown.rows) {
        buffer.set_stringn(body.x, body.y + y, row, width, theme::text_body());
    }
    paint_rows(buffer, body, &paints);
    let ruler = (screen.settings.code_editing).then_some(screen.settings.ruler_column);
    draw_ruler(buffer, body, ruler.unwrap_or(0), shown.left);
    mark_rows(buffer, body, &shown.marks);
    mark_spans(buffer, body, &shown.spans);
    draw_code_marks(buffer, body, &shown);
    let (top, total) = screen.text.lines();
    if numbers > 0 {
        let column = Rect::new(whole.x, whole.y, numbers, whole.height);
        draw_numbers(buffer, column, (top, shown.rows.len()));
    }
    if bar > 0 {
        let first = top.saturating_sub(1);
        let lines = (first, first + u64::from(body.height), total.max(1));
        let span = screen.text.hex_extent(body.height).unwrap_or(lines);
        draw_scrollbar(buffer, whole, span);
    }
    let (row, col) = shown.cursor?;
    let (x, y) = (u16::try_from(col).ok()?, u16::try_from(row).ok()?);
    Some(Position::new(body.x + x, body.y + y))
}

fn draw_status(frame: &mut Frame, area: Rect, text: &str) {
    let buffer = frame.buffer_mut();
    buffer.set_style(area, theme::status_bar());
    buffer.set_stringn(
        area.x,
        area.y,
        text,
        usize::from(area.width),
        theme::status_bar(),
    );
}
