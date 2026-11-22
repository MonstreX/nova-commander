//! The results dialog as FAR's `FindDlgProc` and `AddMenuRecord` keep it.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::clipboard::Memory;
use crate::vfs::Entry;
use std::path::PathBuf;

fn found(folder: &str, name: &str, is_dir: bool) -> Found {
    Found {
        folder: VfsPath::local(folder),
        entry: Entry {
            name: name.to_string(),
            is_dir,
            ..Entry::default()
        },
    }
}

fn sample() -> Vec<Found> {
    vec![
        found("C:\\a", "one.txt", false),
        found("c:\\A", "two.txt", false),
        found("C:\\b", "sub", true),
    ]
}

fn results(items: Vec<Found>, over: bool) -> FindResults {
    let mut results = FindResults::new(Search::stalled(items, over), "Find file: *".into(), None);
    results.pump();
    results
}

fn press(results: &mut FindResults, code: KeyCode, modifiers: KeyModifiers) -> ResultsOutcome {
    results.handle_key(KeyEvent::new(code, modifiers), &mut Memory::default())
}

#[test]
fn found_objects_come_under_their_folder_case_aside() {
    let results = results(sample(), false);
    assert_eq!(
        results.lines,
        vec![
            ResultLine::Folder(VfsPath::local("C:\\a")),
            ResultLine::Found(0),
            ResultLine::Found(1),
            ResultLine::Separator,
            ResultLine::Folder(VfsPath::local("C:\\b")),
            ResultLine::Found(2),
        ]
    );
    assert_eq!((results.files, results.folders), (2, 1));
    assert_eq!(results.cursor, 1, "the first found, not its folder");
    assert_eq!(
        results.count_text().as_deref(),
        Some("Files: 2, folders: 1")
    );
}

#[test]
fn the_first_find_moves_the_focus_to_go_to_unless_the_user_moved_it() {
    let results = results(sample(), false);
    assert_eq!(results.form.focus, GO_TO);
    assert!(!results.form.items[GO_TO].disabled && !results.form.items[VIEW].disabled);
    let mut moved = FindResults::new(Search::stalled(sample(), false), "t".into(), None);
    press(&mut moved, KeyCode::Right, KeyModifiers::NONE);
    moved.pump();
    assert_eq!(
        moved.form.focus, BACKGROUND,
        "Right went past the disabled buttons"
    );
}

#[test]
fn background_and_the_detach_keys_send_a_search_that_goes_on_away() {
    let mut results = results(sample(), false);
    results.form.focus = BACKGROUND;
    assert_eq!(
        press(&mut results, KeyCode::Enter, KeyModifiers::NONE),
        ResultsOutcome::Background
    );
    assert_eq!(
        press(&mut results, KeyCode::Char('b'), KeyModifiers::ALT),
        ResultsOutcome::Background
    );
    let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    for code in [KeyCode::BackTab, KeyCode::Tab, KeyCode::F(12)] {
        assert_eq!(
            press(&mut results, code, ctrl_shift),
            ResultsOutcome::Background,
            "{code:?}"
        );
    }
    assert_eq!(results.search.told(), (false, false), "the search goes on");
}

#[test]
fn a_search_that_is_over_does_not_go_to_the_background() {
    let mut results = FindResults::new(Search::stalled(Vec::new(), true), "t".into(), None);
    results.form.focus = BACKGROUND;
    results.pump();
    assert!(results.form.items[BACKGROUND].disabled);
    assert_eq!(results.form.focus, STOP, "the focus leaves the button");
    let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    assert_eq!(
        press(&mut results, KeyCode::BackTab, ctrl_shift),
        ResultsOutcome::Stay
    );
    assert_eq!(
        press(&mut results, KeyCode::Char('b'), KeyModifiers::ALT),
        ResultsOutcome::Stay
    );
}

#[test]
fn a_search_told_to_stop_does_not_go_to_the_background() {
    let mut results = results(sample(), false);
    results.form.focus = BACKGROUND;
    results.stop();
    assert!(results.form.items[BACKGROUND].disabled);
    assert_eq!(results.form.focus, STOP);
    let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    assert_eq!(
        press(&mut results, KeyCode::F(12), ctrl_shift),
        ResultsOutcome::Stay
    );
}

#[test]
fn the_subject_is_the_masks_and_the_text() {
    let mut results = FindResults::new(Search::stalled(sample(), false), "t".into(), None);
    results.mask = "*.rs".to_string();
    results.pump();
    assert_eq!(
        (results.subject(), results.found_count()),
        ("*.rs".to_string(), 3)
    );
    let mut with_text = FindResults::new(
        Search::stalled(Vec::new(), false),
        "t".into(),
        Some("hi".into()),
    );
    with_text.mask = "*".to_string();
    assert_eq!(with_text.subject(), "* \"hi\"");
}

#[test]
fn the_end_of_the_search_says_what_was_found_and_stop_becomes_cancel() {
    let mut results = results(sample(), true);
    assert!(results.finalized);
    assert_eq!(results.count_text(), None);
    let Control::Label(status) = &results.form.items[STATUS].control else {
        panic!("no status");
    };
    assert_eq!(status, "Search done. Found files: 2, folders: 1");
    let Control::Button { label, .. } = &results.form.items[STOP].control else {
        panic!("no stop");
    };
    assert_eq!(label, "&Cancel");
    assert_eq!(
        press(&mut results, KeyCode::Esc, KeyModifiers::NONE),
        ResultsOutcome::Close
    );
}

#[test]
fn esc_while_it_searches_asks() {
    let mut results = results(sample(), false);
    assert_eq!(
        press(&mut results, KeyCode::Esc, KeyModifiers::NONE),
        ResultsOutcome::AskStop
    );
    assert_eq!(
        press(&mut results, KeyCode::F(10), KeyModifiers::NONE),
        ResultsOutcome::AskStop
    );
}

#[test]
fn the_cursor_steps_over_separators() {
    let mut results = results(sample(), true);
    for _ in 0..2 {
        press(&mut results, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(results.cursor, 4, "past the separator to the folder's line");
    press(&mut results, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(results.cursor, 2);
    press(&mut results, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(results.cursor, 5);
    press(&mut results, KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(results.cursor, 0);
}

#[test]
fn f3_and_f4_open_a_found_file_and_not_a_folder() {
    let mut results = results(sample(), false);
    let file = VfsPath::local("C:\\a\\one.txt");
    // FAR's `GetFiletypeOpenMode`: the key's association first, Ctrl+Shift the program's own at once.
    for (modifiers, view, edit) in [
        (
            KeyModifiers::NONE,
            Some(AssocMode::View),
            Some(AssocMode::Edit),
        ),
        (
            KeyModifiers::ALT,
            Some(AssocMode::AltView),
            Some(AssocMode::AltEdit),
        ),
        (KeyModifiers::CONTROL | KeyModifiers::SHIFT, None, None),
    ] {
        assert_eq!(
            press(&mut results, KeyCode::F(3), modifiers),
            ResultsOutcome::View(file.clone(), view)
        );
        assert_eq!(
            press(&mut results, KeyCode::F(4), modifiers),
            ResultsOutcome::Edit(file.clone(), edit)
        );
    }
    assert_eq!(
        press(&mut results, KeyCode::F(4), KeyModifiers::CONTROL),
        ResultsOutcome::Stay,
        "not one of FAR's"
    );
    results.cursor = 5;
    assert_eq!(
        press(&mut results, KeyCode::F(3), KeyModifiers::NONE),
        ResultsOutcome::Stay
    );
    results.cursor = 0;
    assert_eq!(
        press(&mut results, KeyCode::F(4), KeyModifiers::NONE),
        ResultsOutcome::Stay
    );
}

#[test]
fn go_to_shows_a_file_in_its_folder_and_a_folders_line_in_the_one_above() {
    let mut results = results(sample(), true);
    results.form.focus = GO_TO;
    let go = |results: &mut FindResults| press(results, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        go(&mut results),
        ResultsOutcome::GoTo(VfsPath::local("C:\\a"), Some("one.txt".to_string()))
    );
    results.cursor = 0;
    assert_eq!(
        go(&mut results),
        ResultsOutcome::GoTo(VfsPath::local("C:\\"), Some("a".to_string()))
    );
}

#[test]
fn stop_drops_what_the_search_still_hands_over() {
    let mut results = FindResults::new(Search::stalled(sample(), false), "t".into(), None);
    results.form.focus = STOP;
    assert_eq!(
        press(&mut results, KeyCode::Enter, KeyModifiers::NONE),
        ResultsOutcome::Stay
    );
    assert_eq!(results.search.told(), (true, false));
    results.pump();
    assert!(results.lines.is_empty(), "{:?}", results.lines);
}

#[test]
fn new_search_is_the_default_button() {
    let mut results = FindResults::new(Search::stalled(Vec::new(), true), "t".into(), None);
    results.pump();
    assert_eq!(
        press(&mut results, KeyCode::Enter, KeyModifiers::NONE),
        ResultsOutcome::NewSearch
    );
}

#[test]
fn the_status_says_the_text_cut_to_ten_and_the_place() {
    let text = Some("abcdefghijkl".to_string());
    let mut results = FindResults::new(Search::stalled(Vec::new(), false), "t".into(), text);
    results.pump();
    let Control::Label(status) = &results.form.items[STATUS].control else {
        panic!("no status");
    };
    assert_eq!(status, "Searching \"abcdefghi\u{2026}\" in the place");
}

#[test]
fn a_long_text_gives_way_in_its_middle() {
    assert_eq!(
        crate::app::text_cut::truncate_center("abcdefghij", 5),
        "ab\u{2026}ij"
    );
    assert_eq!(crate::app::text_cut::truncate_center("abc", 5), "abc");
    assert_eq!(
        crate::app::text_cut::truncate_center("abc", 0),
        "",
        "no room"
    );
}

#[test]
fn the_dialog_fills_the_console_but_a_border() {
    let mut results = results(sample(), false);
    results.fit(100, 30);
    assert_eq!((results.form.width, results.form.height), (98, 28));
    assert_eq!(results.rows, 20);
    assert_eq!(results.form.items[STATUS].y, 23);
    assert_eq!(
        results.form.items[PERCENT].y, 23,
        "the percent beside the status"
    );
    assert_eq!(results.form.items[PERCENT].x, 89);
    assert_eq!(results.form.items[STOP].y, 25);
}

#[test]
fn the_percent_of_the_file_read_comes_with_a_text() {
    let search = Search::stalled(Vec::new(), false);
    let mut with_text = FindResults::new(search, "t".into(), Some("x".to_string()));
    with_text.pump();
    let Control::Label(percent) = &with_text.form.items[PERCENT].control else {
        panic!("no percent");
    };
    assert_eq!(percent, " 42%");
    assert!(!with_text.form.items[PERCENT].hidden);
    let names_only = results(Vec::new(), false);
    assert!(names_only.form.items[PERCENT].hidden);
}

#[test]
fn go_to_on_a_stream_shows_its_file() {
    let mut results = results(vec![found("C:\\a", "one.txt:notes", false)], true);
    results.form.focus = GO_TO;
    assert_eq!(
        press(&mut results, KeyCode::Enter, KeyModifiers::NONE),
        ResultsOutcome::GoTo(VfsPath::local("C:\\a"), Some("one.txt".to_string()))
    );
}

/// The results of `sample` with a plugin that takes found files, folders looked for or not.
fn for_panel(panel_folders: bool) -> FindResults {
    let mut results = FindResults::new(Search::stalled(sample(), false), "t".into(), None);
    results.find_list = true;
    results.panel_folders = panel_folders;
    results.pump();
    results
}

#[test]
fn panel_is_off_without_a_plugin_that_takes_found_files() {
    let mut results = results(sample(), false);
    assert!(results.form.items[PANEL].disabled, "FAR's AnySetFindList");
    results.form.focus = PANEL;
    assert_eq!(
        press(&mut results, KeyCode::Enter, KeyModifiers::NONE),
        ResultsOutcome::Stay
    );
    let empty = FindResults::new(Search::stalled(Vec::new(), true), "t".into(), None);
    assert!(empty.form.items[PANEL].disabled, "nothing found: off");
}

#[test]
fn panel_hands_the_files_and_folders_when_they_were_looked_for() {
    let mut results = for_panel(false);
    assert!(
        !results.form.items[PANEL].disabled,
        "on once something is found"
    );
    results.form.focus = PANEL;
    let files = vec![
        std::path::PathBuf::from(r"C:\a\one.txt"),
        std::path::PathBuf::from(r"c:\A\two.txt"),
    ];
    assert_eq!(
        panel_of(press(&mut results, KeyCode::Enter, KeyModifiers::NONE)),
        files.clone(),
        "folders only when they were looked for (FAR's FindFolders)"
    );
    let mut with_folders = for_panel(true);
    with_folders.form.focus = PANEL;
    let mut all = files;
    all.push(std::path::PathBuf::from(r"C:\b\sub"));
    assert_eq!(
        panel_of(press(&mut with_folders, KeyCode::Enter, KeyModifiers::NONE)),
        all
    );
}

#[test]
fn panel_hands_a_plugins_entries_by_their_real_names_and_nothing_without_them() {
    let entry = |name: &str, real: Option<&str>| Found {
        folder: VfsPath::in_session("tmppanel", "common", "/"),
        entry: Entry {
            name: name.to_string(),
            real_path: real.map(std::path::PathBuf::from),
            ..Entry::default()
        },
    };
    // A search in a temporary panel: its entries' real names (FAR's OPIF_REALNAMES).
    let items = vec![entry(r"C:\a\one.txt", Some(r"C:\a\one.txt"))];
    let mut results = FindResults::new(Search::stalled(items, false), "t".into(), None);
    results.find_list = true;
    results.pump();
    results.form.focus = PANEL;
    assert_eq!(
        panel_of(press(&mut results, KeyCode::Enter, KeyModifiers::NONE)),
        vec![std::path::PathBuf::from(r"C:\a\one.txt")]
    );
    // In safe mode (no real names): nothing to hand, no plugin's panel emptied.
    let items = vec![entry(r"C:\a\one.txt", None)];
    let mut results = FindResults::new(Search::stalled(items, false), "t".into(), None);
    results.find_list = true;
    results.pump();
    results.form.focus = PANEL;
    assert_eq!(
        press(&mut results, KeyCode::Enter, KeyModifiers::NONE),
        ResultsOutcome::Stay
    );
}

#[test]
fn panel_hands_an_archives_file_only_for_what_has_no_real_name_in_it() {
    // Found inside a file the search opened: an archive's item, and a list's (a real name).
    let inside = |name: &str, real: Option<&str>| Found {
        folder: VfsPath::parse(r"archive:C:\a.zip!/docs"),
        entry: Entry {
            name: name.to_string(),
            real_path: real.map(std::path::PathBuf::from),
            is_dir: real.is_none(),
            ..Entry::default()
        },
    };
    let items = vec![
        inside("sub", None),
        inside("one.txt", Some(r"C:\b\one.txt")),
    ];
    let mut results = FindResults::new(Search::stalled(items, false), "t".into(), None);
    results.find_list = true;
    results.pump();
    results.form.focus = PANEL;
    assert_eq!(
        panel_of(press(&mut results, KeyCode::Enter, KeyModifiers::NONE)),
        [r"C:\a.zip", r"C:\b\one.txt"].map(PathBuf::from),
        "the archive's file even for a folder in it; the real name itself"
    );
}

#[test]
fn panel_hands_a_folder_found_in_an_archive_as_the_archives_file_with_no_attributes() {
    let folder = Found {
        folder: VfsPath::parse(r"archive:C:\a.zip!/"),
        entry: Entry {
            name: "docs".to_string(),
            is_dir: true,
            attributes: crate::platform::files::Attributes::from_windows_bits(0x10),
            ..Entry::default()
        },
    };
    let entries = results(vec![folder], true).panel_entries();
    assert_eq!(entries.len(), 1, "though folders were not looked for");
    assert_eq!(entries[0].real_path, Some(PathBuf::from(r"C:\a.zip")));
    assert!(!entries[0].is_dir, "a file");
    assert_eq!(entries[0].attributes, Default::default());
}

/// What Panel handed: each found object by its whole path, which is its name and its real name.
fn panel_of(outcome: ResultsOutcome) -> Vec<std::path::PathBuf> {
    let ResultsOutcome::Panel(found) = outcome else {
        panic!("not Panel: {outcome:?}");
    };
    (found.into_iter())
        .map(|entry| {
            assert_eq!(
                entry
                    .real_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                Some(entry.name.clone())
            );
            PathBuf::from(entry.name)
        })
        .collect()
}

#[test]
fn f3_on_a_plugins_file_with_a_real_name_opens_it_and_lists_it_so() {
    let on_plugin = |name: &str, real: Option<&str>| Found {
        folder: VfsPath::in_session("tmppanel", "common", "/"),
        entry: Entry {
            name: name.to_string(),
            real_path: real.map(PathBuf::from),
            ..Entry::default()
        },
    };
    let mut results = results(
        vec![
            on_plugin("a.txt", Some(r"C:\real\a.txt")),
            on_plugin("b.txt", None),
        ],
        true,
    );
    let real = VfsPath::local(r"C:\real\a.txt");
    let ResultsOutcome::View(path, _) = press(&mut results, KeyCode::F(3), KeyModifiers::NONE)
    else {
        panic!("viewed");
    };
    assert_eq!(path, real, "by its real name");
    assert_eq!(
        results.found_files(),
        [real, VfsPath::in_session("tmppanel", "common", "/b.txt")],
        "the viewer's list as F3 opens them"
    );
}
