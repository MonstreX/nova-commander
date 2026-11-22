//! A provider's mode drawn (ADR-0016 §13, 5c-3): its columns' own titles over the kinds', each entry's
//! texts in `C0`.. and its description in `Z`, cut to the column with no mark, none left blank.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::app::{PanelModes, ViewMode};
use crate::ui::columns::{Column, ColumnFlags, ColumnKind};
use crate::ui::panel_cells::cell;
use crate::vfs::{EntryTexts, PanelMode, VfsPath};

fn column(kind: ColumnKind, width: usize) -> Column {
    Column {
        kind,
        flags: ColumnFlags::default(),
        width,
    }
}

fn with_texts(name: &str, columns: &[&str], description: &str) -> Entry {
    Entry {
        name: name.to_string(),
        texts: EntryTexts::of(
            columns.iter().map(ToString::to_string).collect(),
            description.to_string(),
        ),
        ..Entry::default()
    }
}

#[test]
fn an_entrys_texts_are_cut_to_their_column_and_none_are_blank() {
    let entry = with_texts("a", &["kind", "a\tb\nc"], "About a long one");
    let text = |kind, width| cell(&entry, &column(kind, width), false, false, false).text;
    assert_eq!(text(ColumnKind::Custom(0), 6), "kind  ");
    assert_eq!(
        text(ColumnKind::Custom(0), 3),
        "kin",
        "no mark where it is cut"
    );
    assert_eq!(
        text(ColumnKind::Custom(1), 6),
        "a b c ",
        "control characters are spaces"
    );
    assert_eq!(text(ColumnKind::Custom(2), 4), "    ", "a text not given");
    assert_eq!(text(ColumnKind::Description, 8), "About a ");
    let plain = Entry::default();
    for kind in [ColumnKind::Custom(0), ColumnKind::Description] {
        let blank = cell(&plain, &column(kind, 5), false, false, false).text;
        assert_eq!(blank, "     ", "{kind:?}: none");
    }
}

#[test]
fn a_providers_mode_draws_its_titles_and_its_entries_texts() {
    let temp = tempfile::tempdir().unwrap();
    let mut panel = Panel::new(VfsPath::local(temp.path()));
    panel.entries = vec![with_texts("readme.txt", &["file"], "About readme")];
    panel.cursor = 0;
    let own = PanelMode {
        columns: "N,C0,Z".to_string(),
        widths: "10,6,0".to_string(),
        titles: vec![None, Some("Kind".to_string())],
        ..PanelMode::default()
    };
    let modes = PanelModes::default();
    let mode = crate::app::ModeSpec::of_provider(&own, modes.spec(ViewMode::Full));
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    terminal
        .draw(|frame| {
            let look = Look {
                focused: true,
                highlighting: None,
                now: std::time::SystemTime::now(),
                mode: &mode,
            };
            draw(frame, frame.area(), &mut panel, &look, 40);
        })
        .unwrap();
    let row = |terminal: &Terminal<TestBackend>, y: u16| -> String {
        (0..40)
            .map(|x| terminal.backend().buffer()[(x, y)].symbol().to_string())
            .collect()
    };
    let titles = row(&terminal, 1);
    assert!(
        titles.contains("  Name   │ Kind │    Description     ║"),
        "its own title, the kinds' where it gives none: {titles}"
    );
    // FAR's `ColumnTitles`: an empty title given replaces the kind's too.
    let blank = PanelMode {
        titles: vec![None, Some("Kind".to_string()), Some(String::new())],
        ..own
    };
    let mode = crate::app::ModeSpec::of_provider(&blank, modes.spec(ViewMode::Full));
    terminal
        .draw(|frame| {
            let look = Look {
                focused: true,
                highlighting: None,
                now: std::time::SystemTime::now(),
                mode: &mode,
            };
            draw(frame, frame.area(), &mut panel, &look, 40);
        })
        .unwrap();
    let titles = row(&terminal, 1);
    assert!(
        titles.contains("  Name   │ Kind │                    ║"),
        "an empty title shown empty: {titles}"
    );
    let first = row(&terminal, 2);
    assert!(first.contains("readme.txt│file  │About readme"), "{first}");
}

#[test]
fn an_empty_column_is_not_drawn_and_has_no_divider() {
    let temp = tempfile::tempdir().unwrap();
    let mut panel = Panel::new(VfsPath::local(temp.path()));
    panel.entries = vec![with_texts("readme.txt", &["file"], "About readme")];
    panel.cursor = 0;
    let own = PanelMode {
        columns: "N,C0,Z".to_string(),
        widths: "10,-6,0".to_string(),
        status_columns: "N,C0".to_string(),
        status_widths: "0,-1".to_string(),
        ..PanelMode::default()
    };
    let mode = crate::app::ModeSpec::of_provider(&own, PanelModes::default().spec(ViewMode::Full));
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    terminal
        .draw(|frame| {
            let look = Look {
                focused: true,
                highlighting: None,
                now: std::time::SystemTime::now(),
                mode: &mode,
            };
            draw(frame, frame.area(), &mut panel, &look, 40);
        })
        .unwrap();
    let row = |y: u16| -> String {
        (0..40)
            .map(|x| terminal.backend().buffer()[(x, y)].symbol().to_string())
            .collect()
    };
    let (titles, first, separator, status) = (row(1), row(2), row(5), row(6));
    assert_eq!(
        separator.matches('┴').count(),
        1,
        "one divider: {separator}"
    );
    assert!(
        titles.contains("  Name   │        Description        ║"),
        "{titles}"
    );
    assert!(first.contains("readme.txt│About readme"), "{first}");
    assert!(!status.contains("file"), "nor in the status line: {status}"); // FAR's `FCTL_GETCOLUMNTYPES` and `FCTL_GETCOLUMNWIDTHS`: the mode's types, the widths drawn.
    assert_eq!(panel.drawn_columns(), ("N,C0,Z", "10,0,27"));
}

#[test]
fn a_providers_full_screen_mode_takes_the_whole_screen() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = crate::app::App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    app.active = crate::app::Side::Left;
    app.panels[0].set_view_mode(ViewMode::Full);
    let area = ratatui::layout::Rect::new(0, 0, 100, 28);
    let halves = crate::ui::panel_rects(area, &app);
    assert!(
        halves[0].is_some() && halves[1].is_some(),
        "the program's Full"
    );
    let whole = PanelMode {
        columns: "N".to_string(),
        full_screen: true,
        ..PanelMode::default()
    };
    app.panels[0].look = crate::vfs::PanelLook {
        title: None,
        modes: vec![None, None, None, Some(whole)].into(),
        ..crate::vfs::PanelLook::default()
    };
    assert_eq!(
        crate::ui::panel_rects(area, &app),
        [Some(area), None],
        "the plugin's mode at Ctrl+3"
    );
}

#[test]
fn a_name_column_with_o_shows_a_whole_paths_last_name() {
    let column = Column {
        flags: ColumnFlags {
            name_only: true,
            ..ColumnFlags::default()
        },
        ..column(ColumnKind::Name, 10)
    };
    let text = |name: &str, is_dir: bool| {
        let entry = Entry {
            name: name.to_string(),
            is_dir,
            ..Entry::default()
        };
        cell(&entry, &column, false, false, false).text
    };
    assert_eq!(text(r"E:\dev\a.txt", false), "a.txt     ");
    assert_eq!(
        text(r"E:\dev\sub\", true),
        "sub       ",
        "a folder's ending separator"
    );
    assert_eq!(text("plain", false), "plain     ");
    assert_eq!(text("C:", true), "C:        ");
}

#[test]
fn the_divider_before_another_stripe_is_double_past_an_empty_column() {
    let temp = tempfile::tempdir().unwrap();
    let mut panel = Panel::new(VfsPath::local(temp.path()));
    panel.entries = vec![with_texts("readme.txt", &["file"], "About readme")];
    let own = PanelMode {
        columns: "N,C0,N,C0".to_string(),
        widths: "0,-1,0,6".to_string(),
        ..PanelMode::default()
    };
    let mode = crate::app::ModeSpec::of_provider(&own, PanelModes::default().spec(ViewMode::Full));
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    terminal
        .draw(|frame| {
            let look = Look {
                focused: true,
                highlighting: None,
                now: std::time::SystemTime::now(),
                mode: &mode,
            };
            draw(frame, frame.area(), &mut panel, &look, 40);
        })
        .unwrap();
    let separator: String = (0..40)
        .map(|x| terminal.backend().buffer()[(x, 5)].symbol().to_string())
        .collect();
    assert_eq!(
        separator.matches('╨').count(),
        1,
        "the stripes' divider: {separator}"
    );
    assert_eq!(
        separator.matches('┴').count(),
        1,
        "the size column's: {separator}"
    );
}

/// A mode starting with the provider's own column (the process list's friendly names): ".." is in it, not
/// an empty row; a later own column of that row stays empty.
#[test]
fn the_parent_row_shows_its_dots_in_a_first_own_column() {
    let rows = own_mode_rows("12,0");
    assert!(rows[2].starts_with("║..          │ "), "{}", rows[2]);
    assert!(rows[3].starts_with("║Friendly    │x"), "{}", rows[3]);
    let narrow = own_mode_rows("1,0");
    assert!(
        narrow[2].starts_with("║.│ "),
        "cut to its column: {}",
        narrow[2]
    );
}

/// The rows of a panel with ".." and one entry in a plugin's mode "C0,C1" of `widths`.
fn own_mode_rows(widths: &str) -> Vec<String> {
    let temp = tempfile::tempdir().unwrap();
    let mut panel = Panel::new(VfsPath::local(temp.path()));
    panel.entries = vec![
        crate::vfs::Entry::parent(),
        with_texts("a.exe 1", &["Friendly", "x"], ""),
    ];
    panel.cursor = 1;
    let own = PanelMode {
        columns: "C0,C1".to_string(),
        widths: widths.to_string(),
        ..PanelMode::default()
    };
    let modes = PanelModes::default();
    let mode = crate::app::ModeSpec::of_provider(&own, modes.spec(ViewMode::Full));
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    terminal
        .draw(|frame| {
            let look = Look {
                focused: true,
                highlighting: None,
                now: std::time::SystemTime::now(),
                mode: &mode,
            };
            draw(frame, frame.area(), &mut panel, &look, 40);
        })
        .unwrap();
    (0..8)
        .map(|y| {
            (0..40)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}
