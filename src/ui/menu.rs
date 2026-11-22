//! The F9 menu, drawn as FAR draws it: the bar (`HMenu::ShowMenu`) fills the top row, titles start at
//! column 2 as "  Title  " with the hotkey letter highlighted; the pull-down (`VMenu`, `SHORT_DOUBLE_BOX`)
//! opens under its title (see `vmenu`).

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::theme;
use super::vmenu::{self, Line, hi_text, to_u16};
use crate::app::{Command, MENUS, MenuItem, MenuState};

/// `checked(command)` tells which items get FAR's `√` (the current view mode of a panel).
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &MenuState,
    checked: &dyn Fn(Command) -> bool,
) -> Option<crate::app::MenuArea> {
    if area.height == 0 || area.width == 0 {
        return None;
    }
    let buffer = frame.buffer_mut();
    let positions = draw_bar(buffer, area, state.menu);
    let items = state.items();
    let lines: Vec<Line> = items
        .iter()
        .map(|item| match item {
            MenuItem::Item {
                label,
                key,
                command,
            } => Line {
                label,
                key,
                check: checked(*command).then_some('√'),
                separator: false,
                disabled: false,
            },
            MenuItem::Separator => Line {
                label: "",
                key: "",
                check: None,
                separator: true,
                disabled: false,
            },
        })
        .collect();
    let menu = vmenu::Menu {
        lines: &lines,
        selected: state.item,
        title: "",
        bottom_title: "",
        scroll: None,
    };
    let width = menu.natural_width();
    let x = dropdown_x(positions[state.menu], width, area);
    let rect = Rect::new(x, area.y + 1, to_u16(width), to_u16(menu.height())).intersection(area);
    vmenu::draw(buffer, rect, &menu, area);
    Some(super::menu_area(rect, rect, 0))
}

/// The top row; returns the x where each title starts (FAR's `XPos`, also where its menu opens).
fn draw_bar(buffer: &mut Buffer, area: Rect, selected: usize) -> Vec<u16> {
    let bar = Rect::new(area.x, area.y, area.width, 1);
    // `SetScreen(m_Where, L' ', COL_HMENUTEXT)`: the whole row, text included.
    let blank = " ".repeat(usize::from(bar.width));
    buffer.set_stringn(
        bar.x,
        bar.y,
        &blank,
        usize::from(bar.width),
        theme::menu_bar(),
    );
    let positions: Vec<u16> = crate::app::title_positions()
        .into_iter()
        .map(|x| area.x + x)
        .collect();
    for (index, (menu, &x)) in MENUS.iter().zip(&positions).enumerate() {
        let chosen = index == selected;
        let (normal, highlight) = if chosen {
            (theme::menu_highlight(), theme::menu_hotkey(true))
        } else {
            (theme::menu_bar(), theme::menu_bar_hotkey())
        };
        hi_text(
            buffer,
            x,
            area.y,
            &format!("  {}  ", menu.title),
            normal,
            highlight,
        );
    }
    positions
}

/// `VMenu::Show` for a short box: the menu opens at its title and is moved left to stay on screen.
fn dropdown_x(title_x: u16, width: usize, area: Rect) -> u16 {
    let screen_right = usize::from(area.right()) - 1;
    let mut left = usize::from(title_x);
    let right = left + width - 1;
    if right > screen_right.saturating_sub(2) {
        left = (left + screen_right.saturating_sub(4))
            .saturating_sub(right)
            .max(2);
    }
    to_u16(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::vmenu::{cut_visible, visible_length};

    #[test]
    fn accelerator_keys_start_after_the_longest_label() {
        let lines: Vec<Line> = MENUS[1]
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Item { label, key, .. } => Line {
                    label,
                    key,
                    check: None,
                    separator: false,
                    disabled: false,
                },
                MenuItem::Separator => Line {
                    label: "",
                    key: "",
                    check: None,
                    separator: true,
                    disabled: false,
                },
            })
            .collect();
        let texts = vmenu::Menu {
            lines: &lines,
            selected: 0,
            title: "",
            bottom_title: "",
            scroll: None,
        }
        .texts();
        // "Restore selection" is the longest label (17 visible characters).
        assert_eq!(texts[0], "&View              F3");
        assert_eq!(texts[3], "&Rename or move    F6");
        assert_eq!(texts[8], "File &attributes   Ctrl+A");
    }

    #[test]
    fn menus_move_left_to_stay_on_screen() {
        let area = Rect::new(0, 0, 80, 25);
        assert_eq!(dropdown_x(10, 20, area), 10);
        // A 30-wide menu at x 60 would end at 89: moved so its right side is at ScrX - 4 = 75.
        assert_eq!(dropdown_x(60, 30, area), 46);
    }

    #[test]
    fn visible_text_ignores_hotkey_markers() {
        assert_eq!(visible_length("Detai&led"), 8);
        assert_eq!(cut_visible("Detai&led", 6), "Detai&l");
    }
}
