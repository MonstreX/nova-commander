//! Pop-up menus (FAR's `VMenu2` outside the menu bar, with its margin), e.g. Ctrl+F12 "Sort by".

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::{self, Line, to_u16};
use crate::app::{Popup, PopupAnchor, Side};

pub fn draw(frame: &mut Frame, area: Rect, popup: &Popup) -> Option<crate::app::MenuArea> {
    let lines: Vec<Line> = popup
        .items
        .iter()
        .map(|item| Line {
            label: &item.label,
            key: &item.key,
            check: item.check,
            separator: item.separator,
            disabled: false,
        })
        .collect();
    let menu = vmenu::Menu {
        lines: &lines,
        selected: popup.selected,
        title: &popup.title,
        bottom_title: &popup.bottom_title,
        scroll: None,
    };
    // FAR's `VMenu2`: the box two columns and a row inside a margin of the menu's colour.
    let outer = place(
        area,
        popup.anchor,
        menu.natural_width() + 4,
        menu.height() + 2,
    );
    if outer.width < 7 || outer.height < 5 {
        return None;
    }
    vmenu::draw_in_margin(frame.buffer_mut(), outer, &menu, area);
    let inner = Rect::new(outer.x + 2, outer.y + 1, outer.width - 4, outer.height - 2);
    Some(super::menu_area(inner, outer, 0))
}

/// FAR's `SetPosition({ m_Where.left + 4, -1, ... })`: four columns into the panel, centred
/// vertically, kept on screen.
fn place(area: Rect, anchor: PopupAnchor, width: usize, height: usize) -> Rect {
    let PopupAnchor::Panel(side) = anchor;
    let panel_x = match side {
        Side::Left => area.x,
        Side::Right => area.x + area.width / 2,
    };
    let width = to_u16(width).min(area.width);
    let height = to_u16(height).min(area.height);
    let x = (panel_x + 4).min(area.right().saturating_sub(width));
    let screen_bottom = area.height.saturating_sub(1);
    let y = area.y + screen_bottom.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_menu_opens_four_columns_into_its_panel_and_centred() {
        let area = Rect::new(0, 0, 100, 30);
        assert_eq!(
            place(area, PopupAnchor::Panel(Side::Left), 30, 16),
            Rect::new(4, 6, 30, 16)
        );
        assert_eq!(place(area, PopupAnchor::Panel(Side::Right), 30, 16).x, 54);
        // Too wide for the right side: kept on screen.
        assert_eq!(place(area, PopupAnchor::Panel(Side::Right), 60, 16).x, 40);
    }
}
