//! F2: the user menu as FAR's `VMenu2` of `ProcessSingleMenu`: in the middle of the screen, at most
//! `ScrY - 4` lines, the keys to change it in its bottom frame, `►` after a submenu's label; a submenu
//! over the menus it came from (FAR's `ProcessSingleMenu` calls itself with the menu above shown).

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::Line;
use crate::app::{MenuArea, USER_MENU_KEYS, UserMenu, UserMenuLevel, UserMenuLine};

/// FAR's `ScrY - 4`.
const ROWS_LEFT: usize = 5;

/// A line of the menu: a submenu has `►` at its end.
fn menu_line(line: &UserMenuLine) -> Line<'_> {
    Line {
        label: &line.text,
        key: if line.submenu { "\u{25ba}" } else { "" },
        check: None,
        separator: line.separator,
        disabled: false,
    }
}

pub fn draw(frame: &mut Frame, area: Rect, menu: &UserMenu) -> Option<MenuArea> {
    for level in &menu.under {
        draw_under(frame, area, level);
    }
    let all: Vec<Line> = menu.lines.iter().map(menu_line).collect();
    super::centred_menu::draw(
        frame,
        area,
        &super::centred_menu::Centred {
            lines: &all,
            selected: menu.selected,
            top: &menu.top,
            rows: &menu.rows,
            title: &menu.title,
            bottom_title: USER_MENU_KEYS,
            rows_left: ROWS_LEFT,
        },
    )
}

/// A menu above the submenu shown, as it was left.
fn draw_under(frame: &mut Frame, area: Rect, level: &UserMenuLevel) {
    let lines: Vec<Line> = level.lines.iter().map(menu_line).collect();
    super::centred_menu::draw(
        frame,
        area,
        &super::centred_menu::Centred {
            lines: &lines,
            selected: level.selected,
            top: &level.top,
            rows: &level.rows,
            title: &level.title,
            bottom_title: USER_MENU_KEYS,
            rows_left: ROWS_LEFT,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(menu: &UserMenu) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, frame.area(), menu);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn line(text: &str, separator: bool, submenu: bool) -> UserMenuLine {
        UserMenuLine {
            text: text.to_string(),
            separator,
            submenu,
        }
    }

    #[test]
    fn hotkeys_labels_separators_submenus_and_the_keys_below() {
        let menu = UserMenu {
            place: crate::app::MenuPlace::Main,
            file: std::path::PathBuf::new(),
            items: Vec::new(),
            open: Vec::new(),
            selected: 0,
            lines: vec![
                line("&A   Archive", false, false),
                line("Tools", true, false),
                line("&T   Tools", false, true),
                line("F5  Build", false, false),
            ],
            title: "Main menu".to_string(),
            top: std::cell::Cell::new(0),
            rows: std::cell::Cell::new(1),
            cursors: Vec::new(),
            under: Vec::new(),
        };
        let text = render(&menu);
        assert!(text.contains(" Main menu "), "{text}");
        assert!(text.contains("A   Archive"), "{text}");
        assert!(text.contains("─ Tools ─"), "{text}");
        let tools = text.lines().find(|row| row.contains("T   Tools")).unwrap();
        assert!(tools.contains('\u{25ba}'), "{tools}");
        assert!(text.contains("F5  Build"), "{text}");
        assert!(text.contains(USER_MENU_KEYS), "{text}");
    }

    #[test]
    fn a_submenu_is_drawn_over_the_menu_it_came_from() {
        let parent = crate::app::UserMenuLevel {
            lines: (0..12)
                .map(|n| {
                    line(
                        &format!("&{n}   A long parent line number {n}"),
                        false,
                        n == 3,
                    )
                })
                .collect(),
            title: "Main menu".to_string(),
            selected: 3,
            top: std::cell::Cell::new(0),
            rows: std::cell::Cell::new(1),
        };
        let menu = UserMenu {
            place: crate::app::MenuPlace::Main,
            file: std::path::PathBuf::new(),
            items: Vec::new(),
            open: vec![3],
            selected: 0,
            lines: vec![line("&B   Build", false, false)],
            title: "Main menu \u{bb} Tools".to_string(),
            top: std::cell::Cell::new(0),
            rows: std::cell::Cell::new(1),
            cursors: vec![3],
            under: vec![parent],
        };
        let text = render(&menu);
        assert!(
            text.contains("B   Build") && text.contains("Tools "),
            "{text}"
        );
        assert!(
            text.contains("A long parent line number 0"),
            "the parent behind: {text}"
        );
    }
}
