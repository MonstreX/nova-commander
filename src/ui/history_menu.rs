//! The history menus (Alt+F8, Alt+F11, Alt+F12) as FAR's `VMenu2` of `History::Select`: in the middle of
//! the screen, at most `ScrY - 8` lines, the dates as titles of separators, `√` before a locked line.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::{Line, escape_ampersands};
use crate::app::{HistoryLine, HistoryMenu, MenuArea, history_title};

/// FAR's `Height = ScrY - 8`.
const ROWS_LEFT: usize = 9;

pub fn draw(frame: &mut Frame, area: Rect, menu: &HistoryMenu) -> Option<MenuArea> {
    let texts: Vec<String> = (menu.lines.iter())
        .map(|line| match line {
            HistoryLine::Date(date) => date.clone(),
            HistoryLine::Entry { text, .. } => escape_ampersands(text),
        })
        .collect();
    let all: Vec<Line> = (menu.lines.iter().zip(&texts))
        .map(|(line, text)| Line {
            label: text,
            key: "",
            check: matches!(line, HistoryLine::Entry { locked: true, .. }).then_some('√'),
            separator: matches!(line, HistoryLine::Date(_)),
            disabled: false,
        })
        .collect();
    let title = (menu.filter.as_ref()).map_or_else(
        || history_title(menu.list).to_string(),
        |filter| filter.title(history_title(menu.list)),
    );
    super::centred_menu::draw(
        frame,
        area,
        &super::centred_menu::Centred {
            lines: &all,
            selected: menu.selected,
            top: &menu.top,
            rows: &menu.rows,
            title: &title,
            bottom_title: "",
            rows_left: ROWS_LEFT,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::history::List;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::cell::Cell;

    fn render(menu: &HistoryMenu, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, frame.area(), menu);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    fn entry(index: usize, text: &str, locked: bool) -> HistoryLine {
        HistoryLine::Entry {
            index,
            text: text.to_string(),
            locked,
            time: 0,
        }
    }

    fn menu(lines: Vec<HistoryLine>, selected: usize) -> HistoryMenu {
        HistoryMenu {
            list: List::Folders,
            lines,
            selected,
            top: Cell::new(0),
            rows: Cell::new(1),
            filter: None,
        }
    }

    #[test]
    fn dates_title_their_separators_and_locked_lines_are_checked() {
        let shown = menu(
            vec![
                HistoryLine::Date("09.11.2026".into()),
                entry(0, r"C:\R&D", true),
                HistoryLine::Date("10.11.2026".into()),
                entry(1, r"D:\work", false),
            ],
            3,
        );
        let screen = render(&shown, 60, 20);
        let text = screen.join("\n");
        assert!(text.contains(" Folders history "), "{text}");
        let date = screen
            .iter()
            .find(|row| row.contains("09.11.2026"))
            .unwrap();
        assert!(
            date.contains("╟") && date.contains("─ 09.11.2026 ─"),
            "{date}"
        );
        assert!(text.contains(r"√ C:\R&D"), "an `&` is itself: {text}");
        assert_eq!(shown.rows.get(), 4);
    }

    #[test]
    fn the_title_shows_the_filter_open_or_locked() {
        let mut shown = menu(vec![entry(0, r"D:\work", false)], 0);
        shown.filter = Some(crate::app::menu_filter::MenuFilter {
            text: "wo".into(),
            locked: false,
        });
        assert!(
            render(&shown, 60, 20)
                .join(
                    "
"
                )
                .contains(" Folders history [wo] ")
        );
        shown.filter.as_mut().unwrap().locked = true;
        assert!(
            render(&shown, 60, 20)
                .join(
                    "
"
                )
                .contains(" Folders history <wo> ")
        );
    }

    #[test]
    fn a_long_history_scrolls_to_the_selected_line() {
        let lines: Vec<HistoryLine> = (0..40)
            .map(|index| entry(index, &format!("line {index}"), false))
            .collect();
        let shown = menu(lines, 39);
        // FAR's `ScrY - 8` lines: 11 on a screen of 20 rows.
        let text = render(&shown, 60, 20).join("\n");
        assert_eq!(shown.rows.get(), 11);
        assert_eq!(shown.top.get(), 29);
        assert!(
            text.contains("line 39") && !text.contains("line 28 "),
            "{text}"
        );
    }
}
