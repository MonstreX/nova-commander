//! FAR's name flags, the cases of a mode, the extension column and the column modifiers in cells.

use std::time::SystemTime;

use super::super::columns::{Column, ColumnFlags, ColumnKind};
use super::super::panel_cells::{CellLook, cell_in};
use super::*;

fn look() -> CellLook {
    CellLook {
        align_extensions: false,
        folder_align_extensions: false,
        folder_upper_case: false,
        files_lower_case: false,
        upper_to_lower_case: false,
        now: SystemTime::now(),
    }
}

fn entry(name: &str, is_dir: bool) -> Entry {
    Entry {
        name: name.to_string(),
        is_dir,
        ..Entry::default()
    }
}

fn column(kind: ColumnKind, flags: ColumnFlags, width: usize) -> Column {
    Column { kind, flags, width }
}

fn text(entry: &Entry, column: &Column, selected: bool, look: &CellLook) -> String {
    cell_in(entry, column, selected, look, false).text
}

#[test]
fn a_dynamic_mark_takes_room_only_on_a_selected_name() {
    let flags = ColumnFlags {
        mark: true,
        mark_dynamic: true,
        ..ColumnFlags::default()
    };
    let name = column(ColumnKind::Name, flags, 8);
    let file = entry("a.txt", false);
    assert_eq!(text(&file, &name, true, &look()), "√ a.txt ");
    assert_eq!(text(&file, &name, false, &look()), "a.txt   ");
}

#[test]
fn a_forced_name_stands_to_the_right_and_n_leaves_the_extension_out() {
    let force = ColumnFlags {
        right_align_force: true,
        ..ColumnFlags::default()
    };
    let name = column(ColumnKind::Name, force, 8);
    assert_eq!(
        text(&entry("a.txt", false), &name, false, &look()),
        "   a.txt"
    );
    let long = cell_in(&entry("long-name.txt", false), &name, false, &look(), false);
    assert_eq!((long.text.as_str(), long.too_long), ("name.txt", true));
    let bare = ColumnFlags {
        no_extension: true,
        ..ColumnFlags::default()
    };
    let name = column(ColumnKind::Name, bare, 8);
    assert_eq!(
        text(&entry("notes.txt", false), &name, false, &look()),
        "notes   "
    );
    assert_eq!(
        text(&entry("v1.2", true), &name, false, &look()),
        "v1.2    ",
        "a folder keeps it"
    );
}

/// R7 review E14: a name in wide characters is cut by the screen's cells, never running over the
/// column; one crossing the edge is left out and a blank stands in its place.
#[test]
fn a_wide_name_is_cut_by_cells() {
    let name = column(ColumnKind::Name, ColumnFlags::default(), 7);
    let wide = cell_in(
        &entry("日本語の名前.txt", false),
        &name,
        false,
        &look(),
        false,
    );
    assert_eq!((wide.text.as_str(), wide.too_long), ("日本語 ", true));
    assert_eq!(
        text(&entry("日本.c", false), &name, false, &look()),
        "日本.c "
    );
    let right = ColumnFlags {
        right_align: true,
        ..ColumnFlags::default()
    };
    let name = column(ColumnKind::Name, right, 6);
    assert_eq!(
        text(&entry("名前名前.txt", false), &name, false, &look()),
        "前.txt",
        "the end kept"
    );
    let name = column(ColumnKind::Name, right, 5);
    assert_eq!(
        text(&entry("名前名前", false), &name, false, &look()),
        " 名前",
        "the end kept, the crossing one a blank"
    );
}

#[test]
fn the_cases_of_a_mode_change_the_shown_name_only() {
    let name = column(ColumnKind::Name, ColumnFlags::default(), 10);
    let cases = CellLook {
        folder_upper_case: true,
        upper_to_lower_case: true,
        ..look()
    };
    assert_eq!(
        text(&entry("Docs", true), &name, false, &cases),
        "DOCS      "
    );
    assert_eq!(
        text(&entry("README.TXT", false), &name, false, &cases),
        "readme.txt"
    );
    assert_eq!(
        text(&entry("ReadMe.TXT", false), &name, false, &cases),
        "ReadMe.TXT",
        "a mixed name stays"
    );
    let lower = CellLook {
        files_lower_case: true,
        ..look()
    };
    assert_eq!(
        text(&entry("ReadMe.TXT", false), &name, false, &lower),
        "readme.txt"
    );
    let status = cell_in(&entry("Docs", true), &name, false, &cases, true).text;
    assert_eq!(
        status, "Docs      ",
        "the status line keeps the name as it is"
    );
}

#[test]
fn folders_align_their_extensions_with_their_own_flag() {
    let name = column(ColumnKind::Name, ColumnFlags::default(), 12);
    let folders = CellLook {
        folder_align_extensions: true,
        ..look()
    };
    assert_eq!(
        text(&entry("src.old", true), &name, false, &folders),
        "src      old"
    );
    assert_eq!(
        text(&entry("a.txt", false), &name, false, &folders),
        "a.txt       "
    );
    let files = CellLook {
        align_extensions: true,
        ..look()
    };
    assert_eq!(
        text(&entry("src.old", true), &name, false, &files),
        "src.old     "
    );
}

#[test]
fn the_extension_column_shows_a_files_extension() {
    let left = column(ColumnKind::Extension, ColumnFlags::default(), 4);
    assert_eq!(
        text(&entry("notes.txt", false), &left, false, &look()),
        "txt "
    );
    assert_eq!(text(&entry("src.old", true), &left, false, &look()), "    ");
    let right = column(
        ColumnKind::Extension,
        ColumnFlags {
            right_align: true,
            ..ColumnFlags::default()
        },
        4,
    );
    assert_eq!(
        text(&entry("notes.txt", false), &right, false, &look()),
        " txt"
    );
    let long = cell_in(&entry("a.backup", false), &left, false, &look(), false);
    assert_eq!((long.text.as_str(), long.too_long), ("bac…", true));
    let long_right = cell_in(&entry("a.backup", false), &right, false, &look(), false);
    assert_eq!(
        (long_right.text.as_str(), long_right.too_long),
        ("bac…", true),
        "FAR's fit_to_right keeps the start too"
    );
}

#[test]
fn a_name_without_its_extension_is_the_one_its_case_is_told_by() {
    let bare = ColumnFlags {
        no_extension: true,
        ..ColumnFlags::default()
    };
    let name = column(ColumnKind::Name, bare, 8);
    let cases = CellLook {
        upper_to_lower_case: true,
        ..look()
    };
    assert_eq!(
        text(&entry("README.Txt", false), &name, false, &cases),
        "readme  "
    );
}

#[test]
fn a_date_too_long_for_its_column_keeps_its_start() {
    let stamp = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let dated = Entry {
        modified: Some(stamp),
        ..entry("a", false)
    };
    let month = column(
        ColumnKind::WriteTime,
        ColumnFlags {
            month: true,
            ..ColumnFlags::default()
        },
        15,
    );
    let shown = text(&dated, &month, false, &look());
    assert_eq!(shown.chars().count(), 15);
    assert!(shown.ends_with('…'), "{shown:?}");
    assert!(!shown.starts_with(' '), "the day stays whole: {shown:?}");
}

#[test]
fn size_and_date_modifiers_reach_the_cells() {
    let size = column(
        ColumnKind::Size,
        ColumnFlags {
            thousand: true,
            economic: true,
            ..ColumnFlags::default()
        },
        4,
    );
    let file = Entry {
        size: 2_000_000,
        ..entry("a", false)
    };
    assert_eq!(text(&file, &size, false, &look()), "  2m");
    let stamp = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let dated = Entry {
        modified: Some(stamp),
        changed: Some(stamp),
        ..entry("a", false)
    };
    let brief = column(
        ColumnKind::WriteTime,
        ColumnFlags {
            brief: true,
            ..ColumnFlags::default()
        },
        11,
    );
    let old = CellLook {
        now: stamp + std::time::Duration::from_secs(400 * 86_400),
        ..look()
    };
    let shown = text(&dated, &brief, false, &old);
    assert!(
        shown.ends_with(" 2023"),
        "an old date shows its year: {shown:?}"
    );
    assert_eq!(shown.chars().count(), 11);
    let change = column(ColumnKind::ChangeTime, ColumnFlags::default(), 14);
    let written = column(ColumnKind::WriteTime, ColumnFlags::default(), 14);
    assert_eq!(
        text(&dated, &change, false, &look()),
        text(&dated, &written, false, &look()),
        "DE shows the change time as DM the write time"
    );
}

#[test]
fn a_cut_name_kept_to_the_right_has_fars_bracket_on_its_left() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a-rather-long-name.txt"), "x").unwrap();
    let mut panel = crate::app::Panel::new(crate::vfs::VfsPath::local(temp.path()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while panel.entries.len() != 2 || panel.reading().is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the panel was not read"
        );
        panel.poll();
    }
    let mode = crate::app::ModeSpec::make(&crate::settings::PanelMode {
        columns: "NR".to_string(),
        widths: "0".to_string(),
        status_columns: "N".to_string(),
        status_widths: "0".to_string(),
        ..crate::settings::PanelMode::default()
    });
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(16, 8)).unwrap();
    terminal
        .draw(|frame| {
            let look = super::super::panel::Look {
                focused: false,
                highlighting: None,
                now: std::time::SystemTime::now(),
                mode: &mode,
            };
            super::super::panel::draw(frame, frame.area(), &mut panel, &look, 16);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..16)
        .map(|x| buffer[(x, 3)].symbol().to_string())
        .collect();
    assert_eq!(
        row, "{-long-name.txt║",
        "the tail, the left bracket on the frame, the right one as it is"
    );
}
