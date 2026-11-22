//! FAR's `VMenu2` placed in the middle of the screen (`SetPosition(-1, -1, 0, 0)`): its box and margin as
//! wide as its lines, as high as they are up to a limit, the view scrolled to the selected line, the box
//! reported for the mouse. A plugin's menu may stand at its own column and row and show fewer lines
//! (FAR's `Menu` with `X`, `Y` and `MaxHeight`), inside the screen whatever they say.

use std::cell::Cell;

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::{self, Line, to_u16};
use crate::app::MenuArea;

/// The box around the menu and the margin: two columns and one row each side.
const MARGIN_COLUMNS: usize = 4;
const MARGIN_ROWS: usize = 4;

/// What to draw.
pub struct Centred<'a> {
    pub lines: &'a [Line<'a>],
    pub selected: usize,
    /// The first line shown and the lines shown, kept for the app.
    pub top: &'a Cell<usize>,
    pub rows: &'a Cell<usize>,
    pub title: &'a str,
    pub bottom_title: &'a str,
    /// The screen's rows the menu leaves free (FAR's `ScrY - n` is `n + 1` here).
    pub rows_left: usize,
}

/// Where a menu stands: its left column and top row on the screen (none: centred) and the most lines it
/// shows (none: as the screen takes).
#[derive(Debug, Clone, Copy, Default)]
pub struct MenuPlace {
    pub x: Option<u16>,
    pub y: Option<u16>,
    pub most: Option<usize>,
}

pub fn draw(frame: &mut Frame, area: Rect, menu: &Centred) -> Option<MenuArea> {
    draw_at(frame, area, menu, MenuPlace::default())
}

/// The menu at `place`.
pub fn draw_at(
    frame: &mut Frame,
    area: Rect,
    menu: &Centred,
    place: MenuPlace,
) -> Option<MenuArea> {
    let all = menu.lines;
    let most = usize::from(area.height)
        .saturating_sub(menu.rows_left)
        .max(1)
        .min(place.most.unwrap_or(usize::MAX).max(1));
    // An empty menu is a box with no lines in it: one blank row here.
    let rows = all.len().clamp(1, most);
    menu.rows.set(rows);
    let top = if all.is_empty() {
        0
    } else {
        crate::app::keep_in_view(menu.top, menu.selected, all.len(), rows)
    };
    let shown = &all[top.min(all.len())..(top + rows).min(all.len())];
    let vmenu = vmenu::Menu {
        lines: shown,
        selected: menu.selected.saturating_sub(top),
        title: menu.title,
        bottom_title: menu.bottom_title,
        scroll: Some((top, all.len())),
    };
    let width = (vmenu.natural_width() + MARGIN_COLUMNS).min(usize::from(area.width));
    let height = (rows + MARGIN_ROWS).min(usize::from(area.height));
    let along = |at: Option<u16>, room: usize, size: usize| match at {
        Some(at) => usize::from(at).min(room - size),
        None => (room - size) / 2,
    };
    let rect = Rect::new(
        area.x + to_u16(along(place.x, usize::from(area.width), width)),
        area.y + to_u16(along(place.y, usize::from(area.height), height)),
        to_u16(width),
        to_u16(height),
    );
    // `draw_in_margin` draws nothing smaller.
    if rect.width < 7 || rect.height < 5 {
        return None;
    }
    vmenu::draw_in_margin(frame.buffer_mut(), rect, &vmenu, area);
    let inner = Rect::new(rect.x + 2, rect.y + 1, rect.width - 4, rect.height - 2);
    Some(super::menu_area(inner, rect, top))
}
