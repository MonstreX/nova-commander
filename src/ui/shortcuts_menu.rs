//! The folder shortcuts' menus as FAR's `VMenu2` (`Shortcuts::Select`, `Shortcuts::Configure`): in the
//! middle of the screen, at most `ScrY - 4` lines, the keys in the bottom frame, `►` after a list of
//! several folders.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::Line;
use crate::app::{MenuArea, ShortcutLine, ShortcutMenu};

/// FAR's `ScrY - 4`.
const ROWS_LEFT: usize = 5;

fn menu_line(line: &ShortcutLine) -> Line<'_> {
    Line {
        label: &line.text,
        key: if line.submenu { "\u{25ba}" } else { "" },
        check: None,
        separator: line.separator,
        disabled: false,
    }
}

pub fn draw(frame: &mut Frame, area: Rect, menu: &ShortcutMenu) -> Option<MenuArea> {
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
            bottom_title: menu.keys(),
            rows_left: ROWS_LEFT,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::cell::Cell;

    fn render(menu: &ShortcutMenu) -> String {
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

    fn line(text: &str, separator: bool, submenu: bool) -> ShortcutLine {
        ShortcutLine {
            text: text.to_string(),
            separator,
            submenu,
        }
    }

    fn menu(list: Option<usize>, lines: Vec<ShortcutLine>) -> ShortcutMenu {
        ShortcutMenu {
            list,
            raw: false,
            title: "Folder shortcuts".to_string(),
            items: (0..lines.len()).collect(),
            lines,
            selected: 0,
            top: Cell::new(0),
            rows: Cell::new(1),
        }
    }

    #[test]
    fn the_ten_show_their_keys_and_a_list_of_several_its_mark() {
        let text = render(&menu(
            None,
            vec![
                line("RCtrl+&0 \u{2502} <none>", false, false),
                line("RCtrl+&1 \u{2502} C:\\Work", false, true),
            ],
        ));
        assert!(text.contains(" Folder shortcuts "), "{text}");
        assert!(text.contains("RCtrl+0 \u{2502} <none>"), "{text}");
        assert!(text.contains("\u{25ba}"), "{text}");
        assert!(text.contains("Ins, Del, F4 "), "{text}");
        assert!(!text.contains("Ctrl+Up"), "{text}");
    }

    #[test]
    fn a_list_has_the_keys_to_move_its_lines() {
        let text = render(&menu(
            Some(3),
            vec![line("&C:\\Work", false, false), line("", true, false)],
        ));
        assert!(text.contains("C:\\Work"), "{text}");
        assert!(text.contains(crate::app::EDIT_LIST_KEYS), "{text}");
    }
}
