//! Find file's results dialog (`app::find::results`): the form draws the frame, the status and the
//! buttons; the list of found objects goes in rows 2 to `DlgHeight-7`, a line per found object as
//! FAR's `AddMenuRecord` makes it with Advanced's columns (FAR's `SearchOutFormat`, by default "D,S,A":
//! the date, the size and the attributes, each with `│` after it, then the name) and the folder's line
//! above them; the count on the double line under the list while the search goes on.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::columns::{Column, ColumnFlags, ColumnKind};
use super::panel_cells::cell;
use super::theme;
use super::{form, vmenu};
use crate::app::{DialogArea, FindResults, ResultLine};
use crate::vfs::{Entry, VfsPath};

/// FAR's column codes this list shows and their default widths (`GetDefaultWidth`), two-letter
/// codes first. The name always comes last; description, owner, links and streams are not shown.
const CODES: [(&str, ColumnKind, usize); 8] = [
    ("DM", ColumnKind::WriteTime, 14),
    ("DC", ColumnKind::CreationTime, 14),
    ("DA", ColumnKind::AccessTime, 14),
    ("S", ColumnKind::Size, 6),
    ("P", ColumnKind::Allocated, 6),
    ("D", ColumnKind::Date, 8),
    ("T", ColumnKind::Time, 5),
    ("A", ColumnKind::Attributes, 6),
];

/// FAR's `DeserialiseViewSettings` of the find columns: each code (its modifiers after it let go) with
/// its width, 0 or none for the default; codes it does not show are left out.
pub fn find_columns(types: &str, widths: &str) -> Vec<(ColumnKind, usize)> {
    let mut widths = widths
        .split(',')
        .map(|width| width.trim().parse::<usize>().unwrap_or(0));
    let mut columns = Vec::new();
    for code in types.split(',') {
        let code = code.trim().to_uppercase();
        let width = widths.next().unwrap_or(0);
        let known = (CODES.iter()).find(|(prefix, ..)| code.starts_with(prefix));
        // DE (the change time) and the others have no column here.
        if let Some(&(_, kind, default)) = known.filter(|_| !code.starts_with("DE")) {
            columns.push((kind, if width == 0 { default } else { width }));
        }
    }
    columns
}

pub fn draw(frame: &mut Frame, area: Rect, results: &FindResults) -> DialogArea {
    let (_, drawn) = form::draw(frame, area, &results.form);
    let box_rect = drawn.dialog.frame;
    let inside = Rect::new(
        box_rect.x + 3,
        box_rect.y + 1,
        box_rect.width.saturating_sub(6),
        box_rect.height.saturating_sub(2),
    );
    let height = results.form.height;
    let columns = find_columns(&results.columns.0, &results.columns.1);
    let buffer = frame.buffer_mut();
    draw_list(buffer, inside, results, &columns);
    let mut area = drawn.dialog;
    // The list's rows for the mouse, over the box's (the controls are searched from the last).
    area.controls.push(crate::app::DialogControl {
        cells: crate::app::Cells {
            x: inside.x + 1,
            y: inside.y + 1,
            width: inside.width.saturating_sub(2),
            height: u16::try_from(results.rows).unwrap_or(0),
        },
        index: usize::MAX,
        kind: crate::app::ControlKind::List,
    });
    let line = theme::look()
        .dialog_frame
        .double_separator(usize::from(inside.width));
    let y = box_rect.y + height.saturating_sub(6);
    buffer.set_string(inside.x, y, line, theme::dialog(false));
    if let Some(count) = results.count_text() {
        let text = format!(" {count} ");
        let width = u16::try_from(text.chars().count()).unwrap_or(0);
        let x = inside.x + inside.width.saturating_sub(width) / 2;
        buffer.set_string(x, y, text, theme::dialog(false));
    }
    area
}

/// The list from the box's second row, a column in from each side (FAR's `DIF_LISTNOBOX` list).
fn draw_list(
    buffer: &mut Buffer,
    inside: Rect,
    results: &FindResults,
    columns: &[(ColumnKind, usize)],
) {
    let x = inside.x + 1;
    let width = usize::from(inside.width.saturating_sub(2));
    let lines = results.lines.iter().enumerate().skip(results.top);
    for (row, (index, line)) in lines.take(results.rows).enumerate() {
        let y = inside.y + 1 + u16::try_from(row).unwrap_or(0);
        let selected = index == results.cursor;
        let style = theme::dialog_listbox(selected);
        let text = match line {
            ResultLine::Folder(folder) => format!(" {}", with_end_slash(folder)),
            ResultLine::Separator => {
                let line = "\u{2500}".repeat(width);
                buffer.set_string(x, y, line, theme::dialog_listbox(false));
                continue;
            }
            ResultLine::Found(at) => found_line(&results.found[*at].entry, columns),
        };
        let shown = vmenu::cut_visible(&vmenu::escape_ampersands(&text), width);
        let padded = format!("{shown:<width$}");
        buffer.set_string(x, y, padded, style);
    }
}

/// FAR's folder line ends with a slash (`AddEndSlash`); a folder in an archive is FAR's
/// `D:\a.zip:\dir\`.
fn with_end_slash(folder: &VfsPath) -> String {
    let text = folder.shown();
    if text.ends_with(['\\', '/']) {
        return text;
    }
    let slash = if folder.as_local().is_some() || folder.file_path().is_some() {
        std::path::MAIN_SEPARATOR
    } else {
        '/'
    };
    format!("{text}{slash}")
}

/// FAR's `MenuText`: ` ` and each column with `│` after it, then the name.
pub fn found_line(entry: &Entry, columns: &[(ColumnKind, usize)]) -> String {
    let mut text = " ".to_string();
    for &(kind, width) in columns {
        let column = Column {
            kind,
            flags: ColumnFlags::default(),
            width,
        };
        text.push_str(&cell(entry, &column, false, false, false).text);
        text.push('\u{2502}');
    }
    text.push_str(&entry.name);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_has_the_colours_of_a_list_in_a_dialog() {
        use crate::jobs::find::{Found, Search};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::style::Color;

        theme::set_current(theme::Theme::far());
        let found = |name: &str| Found {
            folder: VfsPath::local("C:\\x"),
            entry: Entry {
                name: name.to_string(),
                ..Entry::default()
            },
        };
        let search = Search::stalled(vec![found("one.txt"), found("two.txt")], true);
        let mut results = FindResults::new(search, "Find file: *".to_string(), None);
        results.pump();
        results.fit(100, 30);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut area = None;
        terminal
            .draw(|frame| area = Some(draw(frame, frame.area(), &results)))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // The list's rows for the mouse: from its first line, as wide as the lines drawn.
        let list = (area.unwrap().controls.into_iter())
            .find(|control| control.kind == crate::app::ControlKind::List)
            .expect("the list for the mouse");
        assert_eq!(usize::from(list.cells.height), results.rows);
        let first: String = (list.cells.x..list.cells.x + list.cells.width)
            .map(|x| buffer[(x, list.cells.y)].symbol().to_string())
            .collect();
        assert!(first.contains(r"C:\x"), "the folder's line first: {first}");
        let cell_of = |name: &str| {
            let rows: Vec<String> = (0..30)
                .map(|y| (0..100).map(|x| buffer[(x, y)].symbol()).collect())
                .collect();
            let y = rows.iter().position(|row| row.contains(name)).unwrap();
            let x = rows[y].find(name).unwrap();
            let x = u16::try_from(rows[y][..x].chars().count()).unwrap();
            buffer[(x, u16::try_from(y).unwrap())].clone()
        };
        let current = cell_of("one.txt");
        assert_eq!(
            (current.fg, current.bg),
            (Color::White, Color::Black),
            "under the cursor"
        );
        let other = cell_of("two.txt");
        assert_eq!(
            (other.fg, other.bg),
            (Color::Black, Color::Gray),
            "as the dialog"
        );
    }

    #[test]
    fn the_columns_are_fars_codes_with_their_widths() {
        assert_eq!(
            find_columns("D,S,A", "0,0,0"),
            [
                (ColumnKind::Date, 8),
                (ColumnKind::Size, 6),
                (ColumnKind::Attributes, 6)
            ]
        );
        assert_eq!(
            find_columns("dm, sc ,z,N,de", "20,9,1,1,5"),
            [(ColumnKind::WriteTime, 20), (ColumnKind::Size, 9)],
            "modifiers let go, the unknown left out"
        );
        assert_eq!(
            find_columns("T,P", ""),
            [(ColumnKind::Time, 5), (ColumnKind::Allocated, 6)]
        );
        assert!(find_columns("", "").is_empty());
    }

    #[test]
    fn a_folders_line_ends_with_a_slash() {
        assert_eq!(with_end_slash(&VfsPath::local("C:\\x")), "C:\\x\\");
        assert_eq!(with_end_slash(&VfsPath::local("C:\\")), "C:\\");
        let archive = VfsPath::parse(r"archive:C:\a.zip!/");
        assert_eq!(
            with_end_slash(&archive),
            r"C:\a.zip:\",
            "FAR's archive's root"
        );
        assert_eq!(with_end_slash(&archive.join("d")), r"C:\a.zip:\d\");
        let session = VfsPath::in_session("tmp", "1", "/d");
        assert_eq!(with_end_slash(&session), "tmp://1/d/");
    }

    #[test]
    fn a_found_line_has_fars_columns_then_the_name() {
        let entry = Entry {
            name: "a.txt".to_string(),
            size: 12,
            ..Entry::default()
        };
        let line = found_line(&entry, &find_columns("D,S,A", "0,0,0"));
        assert!(line.starts_with(' '), "{line:?}");
        assert_eq!(line.matches('\u{2502}').count(), 3, "{line:?}");
        assert!(line.ends_with("\u{2502}a.txt"), "{line:?}");
        assert!(line.contains("    12\u{2502}"), "{line:?}");
    }
}
