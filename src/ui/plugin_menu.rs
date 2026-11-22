//! A plugin's menu (FAR's `Menu`): in the middle of the screen or where the plugin puts it, its check
//! marks, grey lines and separators.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::vmenu::Line;
use crate::app::{MenuArea, PLUGIN_MENU_ROWS_LEFT, PluginMenu};

pub fn draw(frame: &mut Frame, area: Rect, menu: &PluginMenu) -> Option<MenuArea> {
    let all: Vec<Line> = (menu.lines.iter())
        .map(|line| Line {
            // The program's code page menu titles its separators (FAR's "System", "Unicode", ...), the
            // syntax type menu its groups (FarColorer's).
            label: if line.separator && !menu.code_pages && menu.colorer.is_none() {
                ""
            } else {
                &line.text
            },
            key: "",
            check: line.checked.then_some('√'),
            separator: line.separator,
            // FAR's `MIF_GRAYED` in grey too (`Menu.GrayText`).
            disabled: line.disabled || line.grayed,
        })
        .collect();
    let place = super::centred_menu::MenuPlace {
        x: menu.x,
        y: menu.y,
        most: menu.max_height,
    };
    let drawn = super::centred_menu::draw_at(
        frame,
        area,
        &super::centred_menu::Centred {
            lines: &all,
            selected: menu.selected,
            top: &menu.top,
            rows: &menu.rows,
            title: &menu.title,
            bottom_title: &menu.bottom,
            rows_left: match menu.tasks {
                Some(_) => crate::app::TASK_LIST_ROWS_LEFT,
                None => PLUGIN_MENU_ROWS_LEFT,
            },
        },
        place,
    )?;
    draw_marks(frame, drawn, menu);
    Some(drawn)
}

/// The lines' marked characters (FAR's annotations: the editor's "All" list's matches) in the
/// menu's highlight colour, cut at the frame.
fn draw_marks(frame: &mut Frame, drawn: MenuArea, menu: &PluginMenu) {
    let inside = drawn.frame;
    let room = usize::from(inside.width.saturating_sub(5));
    for row in 0..inside.height.saturating_sub(2) {
        let index = drawn.first + usize::from(row);
        let Some(Some((from, to))) = menu.marks.get(index) else {
            continue;
        };
        let (from, to) = ((*from).min(room), (*to).min(room));
        if from >= to {
            continue;
        }
        let x = inside.x + 3 + u16::try_from(from).unwrap_or(u16::MAX);
        let width = u16::try_from(to - from).unwrap_or(u16::MAX);
        let style = super::theme::menu_hotkey(index == menu.selected);
        let cells = Rect::new(x, inside.y + 1 + row, width, 1);
        frame.buffer_mut().set_style(cells, style);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::plugin_host::windows::{MenuLine, MenuRequest};

    /// Ctrl+W's task list takes the screen's rows but five (FAR's `VMenu2` with `ScrY - 4`), a plugin's
    /// menu but seven.
    #[test]
    fn the_task_list_takes_more_rows() {
        let lines: Vec<MenuLine> = (0..60)
            .map(|at| MenuLine {
                text: format!("line {at}"),
                ..MenuLine::default()
            })
            .collect();
        let shown = |tasks: bool| {
            let mut menu = PluginMenu::new(MenuRequest {
                title: "Menu".to_string(),
                lines: lines.clone(),
                ..MenuRequest::default()
            });
            if tasks {
                menu.tasks = Some(Default::default());
            }
            let mut terminal = Terminal::new(TestBackend::new(40, 30)).unwrap();
            terminal
                .draw(|frame| {
                    draw(frame, frame.area(), &menu);
                })
                .unwrap();
            menu.rows.get()
        };
        assert_eq!(shown(true), 25);
        assert_eq!(shown(false), 23);
    }

    #[test]
    fn a_separators_text_is_not_shown_and_a_checked_line_has_its_mark() {
        let line = |text: &str| MenuLine {
            text: text.to_string(),
            ..MenuLine::default()
        };
        let menu = PluginMenu::new(MenuRequest {
            title: "Menu".to_string(),
            lines: vec![
                line("First"),
                MenuLine {
                    separator: true,
                    ..line("Hidden")
                },
                MenuLine {
                    checked: true,
                    ..line("Checked")
                },
            ],
            ..MenuRequest::default()
        });
        let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
        terminal
            .draw(|frame| {
                assert!(draw(frame, frame.area(), &menu).is_some());
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..20)
            .map(|y| (0..40).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        let screen = rows.join(
            "
",
        );
        assert!(screen.contains("First"), "{screen}");
        assert!(!screen.contains("Hidden"), "{screen}");
        let checked = rows.iter().find(|row| row.contains("Checked")).unwrap();
        assert!(checked.contains("√ Checked"), "{checked}");
        let first = rows.iter().find(|row| row.contains("First")).unwrap();
        assert!(!first.contains('√'), "{first}");
    }

    #[test]
    fn a_menu_stands_where_its_plugin_puts_it_and_shows_its_most_lines() {
        let lines = (0..10)
            .map(|line| MenuLine {
                text: format!("Line {line}"),
                ..MenuLine::default()
            })
            .collect();
        let menu = PluginMenu::new(MenuRequest {
            title: "Menu".to_string(),
            lines,
            x: Some(3),
            y: Some(2),
            max_height: Some(4),
            ..MenuRequest::default()
        });
        let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
        let mut shown = None;
        terminal
            .draw(|frame| shown = draw(frame, frame.area(), &menu))
            .unwrap();
        let area = shown.unwrap();
        assert_eq!((area.outer.x, area.outer.y), (3, 2), "its margin's corner");
        assert_eq!(menu.rows.get(), 4, "FAR's MaxHeight");
        let far = PluginMenu::new(MenuRequest {
            x: Some(39),
            y: Some(19),
            ..MenuRequest::default()
        });
        terminal
            .draw(|frame| shown = draw(frame, frame.area(), &far))
            .unwrap();
        let area = shown.unwrap();
        assert!(area.outer.x + area.outer.width <= 40 && area.outer.y + area.outer.height <= 20);
    }
}
