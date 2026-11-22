//! A list edited in place (Commands > File associations) as FAR's `VMenu2`: in the middle of the screen,
//! at most `ScrY - 4` lines, its keys in the bottom frame.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::{Line, escape_ampersands};
use crate::app::{EditList, MenuArea};

/// FAR's `ScrY - 4`.
const ROWS_LEFT: usize = 5;

pub fn draw(frame: &mut Frame, area: Rect, list: &EditList) -> Option<MenuArea> {
    let texts: Vec<String> = (list.lines.iter())
        .map(|line| {
            if list.hotkeys {
                line.clone()
            } else {
                escape_ampersands(line)
            }
        })
        .collect();
    let all: Vec<Line> = (texts.iter().enumerate())
        .map(|(at, text)| Line {
            label: text,
            key: "",
            check: None,
            separator: list.separators.contains(&at),
            disabled: false,
        })
        .collect();
    super::centred_menu::draw(
        frame,
        area,
        &super::centred_menu::Centred {
            lines: &all,
            selected: list.selected,
            top: &list.top,
            rows: &list.rows,
            title: &list.title,
            bottom_title: &list.bottom,
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

    fn render(list: &EditList) -> String {
        let mut terminal = Terminal::new(TestBackend::new(70, 20)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, frame.area(), list);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..20)
            .map(|y| (0..70).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn list(lines: &[&str]) -> EditList {
        EditList {
            kind: crate::app::EditListKind::Associations,
            title: "File associations".to_string(),
            items: (0..lines.len()).collect(),
            bottom: crate::app::EDIT_LIST_KEYS.to_string(),
            found: Vec::new(),
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
            selected: 0,
            top: Cell::new(0),
            rows: Cell::new(1),
            separators: Vec::new(),
            hotkeys: false,
        }
    }

    #[test]
    fn an_empty_list_is_a_box_with_its_title_and_keys() {
        let text = render(&list(&[]));
        assert!(text.contains(" File associations "), "{text}");
        assert!(text.contains(crate::app::EDIT_LIST_KEYS), "{text}");
    }

    #[test]
    fn lines_show_their_ampersands_as_they_are() {
        let text = render(&list(&["Tom & Jerry \u{2502} *.tj"]));
        assert!(text.contains("Tom & Jerry \u{2502} *.tj"), "{text}");
    }

    #[test]
    fn a_list_with_hotkeys_shows_them_and_its_separator() {
        let mut modes = list(&["&Brief", "", "Mine"]);
        modes.hotkeys = true;
        modes.separators = vec![1];
        let text = render(&modes);
        assert!(text.contains("Brief") && !text.contains("&Brief"), "{text}");
        let rows: Vec<&str> = text.lines().collect();
        let brief = rows.iter().position(|row| row.contains("Brief")).unwrap();
        assert!(
            rows[brief + 1].contains('\u{255f}'),
            "the separator: {}",
            rows[brief + 1]
        );
        assert!(rows[brief + 2].contains("Mine"));
    }
}
