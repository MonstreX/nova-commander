//! Ctrl+I: FAR's "Filters menu" (`VMenu2` of `filters::EditFilters`): in the middle of the screen, at
//! most `ScrY - 6` lines, each line's mark (`+`, `-`, `I`, `X`) in the check column, the keys below.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::Line;
use crate::app::{FILTERS_KEYS, FILTERS_TITLE, FiltersMenu, MenuArea};

/// FAR's `ScrY - 6`.
const ROWS_LEFT: usize = 7;

pub fn draw(frame: &mut Frame, area: Rect, menu: &FiltersMenu) -> Option<MenuArea> {
    let all: Vec<Line> = (menu.lines.iter())
        .map(|line| Line {
            label: &line.text,
            key: "",
            check: line.check,
            separator: line.text.is_empty() && line.check.is_none(),
            disabled: false,
        })
        .collect();
    super::centred_menu::draw(
        frame,
        area,
        &super::centred_menu::Centred {
            lines: &all,
            selected: menu.selected,
            top: &menu.top,
            rows: &menu.rows,
            title: FILTERS_TITLE,
            bottom_title: FILTERS_KEYS,
            rows_left: ROWS_LEFT,
        },
    )
}
