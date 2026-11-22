//! Options > Plugins (P6.0e, the owner's look, 2026-11-15): a menu in the menus' colours as F11's, each
//! plugin's name at the left and its state as a word at the right edge, all states in one column, the
//! list's keys in the bottom frame. The state's colour: loaded as the menu's text, starting as its hotkeys,
//! turned off or not loaded grey (the theme's roles; the cursor's line as it is).

use std::cell::Cell;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::centred_menu::{Centred, MenuPlace, draw_at};
use super::theme;
use super::vmenu::Line;
use crate::app::{MenuArea, PLUGIN_MENU_ROWS_LEFT, PLUGINS_KEYS, PluginRow, PluginState};

/// The list `items` (the rows' lines, which size it) of `rows`, the cursor on `cursor`, the first line
/// shown kept in `top`.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    (items, rows): (&[String], &[PluginRow]),
    (cursor, top): (usize, &Cell<usize>),
) -> Option<MenuArea> {
    // A name's `&` is no hotkey.
    let labels: Vec<String> = items.iter().map(|item| item.replace('&', "&&")).collect();
    let lines: Vec<Line> = (labels.iter())
        .map(|label| Line {
            label,
            key: "",
            check: None,
            separator: false,
            disabled: false,
        })
        .collect();
    let shown = Cell::new(0);
    let menu = Centred {
        lines: &lines,
        selected: cursor,
        top,
        rows: &shown,
        title,
        bottom_title: PLUGINS_KEYS,
        rows_left: PLUGIN_MENU_ROWS_LEFT,
    };
    let drawn = draw_at(frame, area, &menu, MenuPlace::default())?;
    states(frame.buffer_mut(), drawn, rows, (cursor, shown.get()));
    Some(drawn)
}

/// Each shown row's state at the box's right edge (a name too long cut with `»`).
fn states(
    buffer: &mut Buffer,
    drawn: MenuArea,
    rows: &[PluginRow],
    (cursor, shown): (usize, usize),
) {
    let frame = drawn.frame;
    // The text runs from the third column to the scroll marker's (`vmenu::draw_item`).
    let (left, end) = (frame.x + 3, frame.x + frame.width.saturating_sub(2));
    let room = usize::from(end.saturating_sub(left));
    for (row, plugin) in rows.iter().enumerate().skip(drawn.first).take(shown) {
        let selected = row == cursor;
        let base = match selected {
            true => theme::menu_highlight(),
            false => theme::menu_text(),
        };
        let style = match (selected, plugin.state) {
            (true, _) | (false, PluginState::Loaded) => base,
            (false, PluginState::Starting) => theme::menu_hotkey(false),
            (false, PluginState::Disabled | PluginState::NotLoaded) => theme::menu_disabled(false),
        };
        let word = plugin.state.shown();
        let at = room.saturating_sub(word.len());
        let y = frame.y + 1 + to_u16(row - drawn.first);
        let name: String = match plugin.name.chars().count() < at {
            true => plugin.name.clone(),
            false => (plugin.name.chars().take(at.saturating_sub(2)))
                .chain(['»'])
                .collect(),
        };
        let blank = at.saturating_sub(name.chars().count());
        let text = format!("{name}{}", " ".repeat(blank));
        buffer.set_stringn(left, y, text, at, base);
        buffer.set_stringn(left + to_u16(at), y, &word, room - at, style);
    }
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}
