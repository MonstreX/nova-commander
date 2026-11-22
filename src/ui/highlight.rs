//! The colours of a panel entry (FAR's `FileList::GetShowColor`): the panel's colour of the entry's
//! state (normal, selected, under the cursor, selected under the cursor) with the highlighting group's
//! colour of that state over it (`app::highlighting`; its transparent parts leave the panel's), and its
//! mark's colour over the name's. The sixteen colours are the console's, the theme's exactly.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use super::columns::{Column, ColumnKind};
use super::panel_cells::cell;
use super::theme;
use crate::app::{HighlightElement, HighlightState, Paint};

/// FAR's console colours in their order (`C_BLACK` .. `C_WHITE`), as the theme files name them.
const COLORS: [Color; 16] = [
    Color::Black,
    Color::Blue,
    Color::Green,
    Color::Cyan,
    Color::Red,
    Color::Magenta,
    Color::Yellow,
    Color::Gray,
    Color::DarkGray,
    Color::LightBlue,
    Color::LightGreen,
    Color::LightCyan,
    Color::LightRed,
    Color::LightMagenta,
    Color::LightYellow,
    Color::White,
];

/// The panel's own colour of a state.
fn panel_style(state: HighlightState) -> Style {
    match state {
        HighlightState::Normal => theme::panel(),
        HighlightState::Selected => theme::selected(),
        HighlightState::Cursor => theme::cursor(false),
        HighlightState::SelectedCursor => theme::cursor(true),
    }
}

/// `paint` laid over `style`: what it gives replaces, what is transparent stays.
fn over(style: Style, paint: Paint) -> Style {
    let color = |index: u8| COLORS[usize::from(index) % COLORS.len()];
    let style = match paint.fg {
        Some(fg) => style.fg(color(fg)),
        None => style,
    };
    match paint.bg {
        Some(bg) => style.bg(color(bg)),
        None => style,
    }
}

/// What a Name column shows before the name (FAR's `ShowList`): the selection's `√ ` when the column
/// has it, then the highlighting mark when there is room for it (`Width > 1`, its screen width); and the
/// column left for the name.
pub struct NameMarks<'a> {
    pub selection: Option<&'static str>,
    pub mark: Option<&'a str>,
    pub rest: Column,
}

/// A column of the status line: the name's with FAR's marks before it, in the panel's colour.
pub fn status_cell(
    entry: &crate::vfs::Entry,
    column: &Column,
    selected: bool,
    mark: &str,
) -> String {
    match name_marks(column, selected, mark) {
        Some(marks) => {
            let shown = cell(entry, &marks.rest, selected, false, true).text;
            let (selection, mark) = (marks.selection.unwrap_or(""), marks.mark.unwrap_or(""));
            format!("{selection}{mark}{shown}")
        }
        None => cell(entry, column, selected, false, true).text,
    }
}

/// The marks of `column` for an entry with the highlighting mark `mark`; none when the column is not
/// the name's or there is no mark (the cell then shows the selection's mark itself).
pub fn name_marks<'a>(column: &Column, selected: bool, mark: &'a str) -> Option<NameMarks<'a>> {
    if column.kind != ColumnKind::Name || mark.is_empty() {
        return None;
    }
    let mut rest = *column;
    // FAR's `D`: no room for the mark on a name that is not selected.
    let dynamic = column.flags.mark_dynamic && !selected;
    let selection = (column.flags.mark && rest.width > 2 && !dynamic).then(|| {
        rest.width -= 2;
        rest.flags.mark = false;
        if selected { "√ " } else { "  " }
    });
    rest.flags.mark &= !dynamic;
    let mark_width = Span::raw(mark).width();
    let room = rest.width > 1 && rest.width >= mark_width;
    if room {
        rest.width -= mark_width;
    }
    Some(NameMarks {
        selection,
        mark: room.then_some(mark),
        rest,
    })
}

/// The name's style and the mark's of an entry in `state`; `None` for no highlighting.
pub fn entry_styles(element: Option<&HighlightElement>, state: HighlightState) -> (Style, Style) {
    let base = panel_style(state);
    let Some(element) = element else {
        return (base, base);
    };
    let name = over(base, element.name(state));
    (name, over(name, element.mark(state)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::HighlightGroups;
    use crate::vfs::{Entry, FileMask};

    fn far() -> crate::app::Highlighting {
        let mask = |text: &str| FileMask::parse(&text.replace("<exec>", "*.exe"));
        crate::app::Highlighting::make(&HighlightGroups::far_defaults(), &mask)
    }

    #[test]
    fn selection_beats_highlighting_and_the_cursor_takes_the_groups_cursor_colour() {
        let exe = Entry {
            name: "run.exe".into(),
            ..Entry::default()
        };
        let element = far().element(&exe, std::time::SystemTime::now());
        let style = |state| entry_styles(element.as_ref(), state).0;
        assert_eq!(style(HighlightState::Normal).fg, Some(Color::LightGreen));
        assert_eq!(
            style(HighlightState::Normal).bg,
            theme::panel().bg,
            "the panel's background"
        );
        assert_eq!(style(HighlightState::Selected), theme::selected());
        let cursor = style(HighlightState::Cursor);
        assert_eq!(cursor.fg, Some(Color::LightGreen));
        assert_eq!(cursor.bg, theme::cursor(false).bg);
        assert_eq!(style(HighlightState::SelectedCursor), theme::cursor(true));
    }

    #[test]
    fn no_group_leaves_the_panels_colours() {
        let (name, mark) = entry_styles(None, HighlightState::Cursor);
        assert_eq!((name, mark), (theme::cursor(false), theme::cursor(false)));
    }

    #[test]
    fn the_mark_takes_the_names_colour_under_its_own() {
        let element = HighlightElement {
            names: [Paint {
                fg: Some(10),
                bg: None,
            }; 4],
            marks: [Paint {
                fg: None,
                bg: Some(1),
            }; 4],
            mark: "*".into(),
        };
        let (name, mark) = entry_styles(Some(&element), HighlightState::Normal);
        assert_eq!(name.fg, Some(Color::LightGreen));
        assert_eq!(
            (mark.fg, mark.bg),
            (Some(Color::LightGreen), Some(Color::Blue))
        );
    }

    #[test]
    fn the_mark_stands_before_the_name_in_its_own_colour() {
        use crate::app::Panel;
        use crate::settings::{HighlightColors, HighlightGroup};
        use crate::vfs::VfsPath;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("main.rs"), "x").unwrap();
        let mut panel = Panel::new(VfsPath::local(temp.path()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while panel.entries.len() != 2 || panel.reading().is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "the panel was not read"
            );
            panel.poll();
        }
        let groups = HighlightGroups {
            first: vec![HighlightGroup {
                filter: crate::settings::Filter {
                    mask: "*.rs".to_string(),
                    ..crate::settings::Filter::default()
                },
                colors: HighlightColors {
                    mark_normal: "yellow".to_string(),
                    ..HighlightColors::default()
                },
                mark: "*".to_string(),
                mark_inherit: false,
                ..HighlightGroup::default()
            }],
            ..HighlightGroups::default()
        };
        let highlighting = crate::app::Highlighting::make(&groups, &|text| FileMask::parse(text));
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let modes = crate::app::PanelModes::default();
        terminal
            .draw(|frame| {
                let look = super::super::panel::Look {
                    focused: false,
                    highlighting: Some(&highlighting),
                    now: std::time::SystemTime::now(),
                    mode: modes.spec(panel.view_mode),
                };
                super::super::panel::draw(frame, frame.area(), &mut panel, &look, 40);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row: String = (0..40)
            .map(|x| buffer[(x, 3)].symbol().to_string())
            .collect();
        assert!(row.starts_with("║*main.rs"), "{row:?}");
        assert_eq!(buffer[(1, 3)].fg, Color::LightYellow, "the mark's colour");
        assert_eq!(Some(buffer[(2, 3)].fg), theme::panel().fg, "the name's");
    }

    #[test]
    fn the_selection_mark_comes_first_and_a_wide_mark_takes_its_columns() {
        use super::super::columns::ColumnFlags;
        let column = Column {
            kind: ColumnKind::Name,
            flags: ColumnFlags {
                mark: true,
                ..ColumnFlags::default()
            },
            width: 10,
        };
        let marks = name_marks(&column, true, "\u{2605}").unwrap();
        assert_eq!(
            (marks.selection, marks.mark),
            (Some("\u{221a} "), Some("\u{2605}"))
        );
        assert_eq!(marks.rest.width, 7);
        assert!(!marks.rest.flags.mark, "the cell does not put it again");
        let wide = name_marks(&column, false, "\u{6f22}").unwrap();
        assert_eq!(wide.rest.width, 6, "a wide character takes two columns");
        let narrow = Column { width: 3, ..column };
        let none = name_marks(&narrow, false, "ab").unwrap();
        assert_eq!(
            (none.selection, none.mark),
            (Some("  "), None),
            "no room left"
        );
        assert!(name_marks(&column, false, "").is_none());
    }
}
