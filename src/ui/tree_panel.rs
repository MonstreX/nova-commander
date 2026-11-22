//! FAR's tree panel (`TreeList::DisplayTree`, `DisplayTreeName`): the double frame with " Tree ", each
//! folder after FAR's lines (`│ `, `├─`, `└─`; the root as `\`), the other panel's folder in the colour
//! of a selected name, the cursor's line in the cursor's colour when the panel has the focus and in
//! brackets when it has not, a separator and the cursor folder's full path in the status line. While
//! its worker reads, the separator says so with the folders known (ADR-0014: no modal message).

use crate::app::text_cut::cut_right;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType};

use super::theme;
use crate::app::tree_panel::{TreeItem, TreePanel};

/// FAR's `TreeLineSymbol`: a level above whose folder was the last, one that was not, the folder
/// that is the last of its parent, one that is not.
const LINES: [&str; 4] = ["  ", "\u{2502} ", "\u{2514}\u{2500}", "\u{251c}\u{2500}"];

fn name_of(item: &TreeItem) -> String {
    if item.depth == 0 {
        return "\\".to_string();
    }
    item.path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// The lines before a folder's name.
fn prefix(item: &TreeItem, room: usize) -> String {
    let mut text = String::new();
    for (level, last) in item
        .last
        .iter()
        .enumerate()
        .take(item.depth.saturating_sub(1))
    {
        if 3 * level >= room {
            break;
        }
        text.push_str(LINES[usize::from(!*last)]);
    }
    if let Some(last) = item.last.get(item.depth.wrapping_sub(1)) {
        text.push_str(LINES[if *last { 2 } else { 3 }]);
    }
    text
}

fn set_text(buffer: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    super::vmenu::set_text(buffer, x, y, text, style);
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

/// One folder's line at row `y` (FAR's `DisplayTree` loop and `DisplayTreeName`).
fn draw_line(buffer: &mut Buffer, area: Rect, y: u16, tree: &TreePanel, at: usize, focused: bool) {
    let item = &tree.items[at];
    let right = area.right() - 1;
    let start = area.x + 2;
    let lines = prefix(item, usize::from(right.saturating_sub(start + 6)));
    set_text(buffer, start, y, &lines, theme::panel());
    let mut x = (start + width_of(&lines)).min(right.saturating_sub(4));
    let name = name_of(item);
    let work = tree.work == Some(at);
    if at == tree.cursor {
        x -= 1;
        let room = usize::from(right.saturating_sub(x + 3));
        let (text, style) = match (focused, work) {
            (true, _) => (format!(" {} ", cut_right(&name, room)), theme::cursor(work)),
            (false, true) => (format!("[{}]", cut_right(&name, room)), theme::selected()),
            (false, false) => (format!("[{}]", cut_right(&name, room)), theme::panel()),
        };
        set_text(buffer, x, y, &text, style);
    } else {
        let style = if work {
            theme::selected()
        } else {
            theme::panel()
        };
        set_text(
            buffer,
            x,
            y,
            &cut_right(&name, usize::from(right.saturating_sub(x + 1))),
            style,
        );
    }
}

/// The list's rows in a frame at `area`: FAR's height - 4, one less in "Find folder" (`modal`: its
/// search line under the separator).
pub fn rows_of(area: Rect, modal: bool) -> usize {
    usize::from(area.height.saturating_sub(4 + u16::from(modal)))
}

pub fn draw(frame: &mut Frame, area: Rect, tree: &mut TreePanel, focused: bool) {
    tree.set_rows(rows_of(area, false));
    draw_box(frame, area, tree, focused, " Tree ", false);
}

/// The frame, its title, the lines, the separator (saying "Reading" while the worker reads) and the
/// cursor folder's path on the last line; `modal` leaves the line above it to "Find folder"'s search.
pub fn draw_box(
    frame: &mut Frame,
    area: Rect,
    tree: &TreePanel,
    focused: bool,
    title: &str,
    modal: bool,
) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::panel_border())
        .style(theme::panel());
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);
    if area.width < 8 || area.height < 5 + u16::from(modal) {
        return;
    }
    let buffer = frame.buffer_mut();
    let x = area.x + area.width.saturating_sub(width_of(title)) / 2;
    set_text(buffer, x, area.y, title, theme::panel_title(focused));
    let separator_y = area.bottom() - 3 - u16::from(modal);
    let inner = usize::from(area.width - 2);
    let separator = format!("\u{255f}{}\u{2562}", "\u{2500}".repeat(inner));
    set_text(
        buffer,
        area.x,
        separator_y,
        &separator,
        theme::panel_border(),
    );
    if tree.reading {
        let text = format!(" Reading: {} ", tree.count);
        let x = area.x + area.width.saturating_sub(width_of(&text)) / 2;
        set_text(buffer, x, separator_y, &text, theme::panel_border());
    }
    for (row, y) in (area.y + 1..separator_y).enumerate() {
        let at = tree.top + row;
        if at < tree.items.len() {
            draw_line(buffer, area, y, tree, at, focused);
        }
    }
    if let Some(item) = tree.current() {
        let path = cut_right(&item.path.to_string_lossy(), inner);
        set_text(buffer, area.x + 1, area.bottom() - 2, &path, theme::panel());
    }
}

#[cfg(test)]
#[path = "tree_panel_tests.rs"]
mod tests;
