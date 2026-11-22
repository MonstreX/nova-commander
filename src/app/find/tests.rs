//! Alt+F7 as FAR's `FindFiles`: the dialog, the search in the panel's folder, its results dialog, Go to,
//! the viewer over it and Esc's question; every search stays in the test's own folder.

use std::path::Path;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::find::results::ResultLine;
use crate::jobs::find::text as jobs_text;

pub(super) fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

/// `a.txt`, `sub\b.txt` ("hello there") and `sub\c.rs`, the panel read.
pub(super) fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "nothing").unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("sub").join("b.txt"), "hello there").unwrap();
    std::fs::write(temp.path().join("sub").join("c.rs"), "fn main() {}").unwrap();
    let mut app = app_in(temp.path());
    until(&mut app, |app| app.panels[0].entries.len() == 3);
    (temp, app)
}

pub(super) fn until(app: &mut App, mut done: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "it did not happen");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn form(app: &mut App) -> &mut Form {
    let Some(Overlay::Dialog(Dialog::Form {
        form,
        action: FormAction::FindFile,
    })) = &mut app.overlay
    else {
        panic!("no Find file dialog");
    };
    form
}

pub(super) fn results(app: &App) -> &FindResults {
    let Some(Overlay::FindResults(results)) = &app.overlay else {
        panic!("no results");
    };
    results
}

/// Alt+F7 with these masks and text, Find, and the search to its end.
pub(super) fn find(app: &mut App, mask: &str, text: &str) {
    press(app, KeyCode::F(7), KeyModifiers::ALT);
    form(app).set_edit_text(MASK, mask);
    form(app).set_edit_text(TEXT, text);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    until(app, |app| results(app).finalized);
}

pub(super) fn found_names(app: &App) -> Vec<String> {
    let results = results(app);
    (results.lines.iter())
        .filter_map(|line| match line {
            ResultLine::Found(at) => Some(results.found[*at].entry.name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn alt_f7_opens_fars_dialog() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    let form = form(&mut app);
    assert_eq!(form.title, FIND_TITLE);
    assert_eq!((form.width, form.height), (80, 21));
    let Some(Control::DropDown { lines, chosen, .. }) = form.control(AREA) else {
        panic!("no area");
    };
    assert_eq!(*chosen, 4, "From the current folder");
    let ListLine::Choice(root) = &lines[3] else {
        panic!("no root line");
    };
    assert!(root.starts_with("From the &root of "), "{root}");
    assert!(!root.ends_with('\\'), "{root}");
    assert_eq!(form.focus, MASK);
}

#[test]
fn the_masks_find_files_in_the_folder_and_below() {
    let (temp, mut app) = app();
    find(&mut app, "*.txt", "");
    assert_eq!(found_names(&app), ["a.txt", "b.txt"]);
    let results = results(&app);
    assert_eq!(
        results.lines[0],
        ResultLine::Folder(VfsPath::local(temp.path()))
    );
    assert_eq!(results.form.title, "Find file: *.txt");
}

#[test]
fn folders_are_found_only_when_asked() {
    let (_temp, mut app) = app();
    find(&mut app, "*", "");
    assert!(found_names(&app).contains(&"sub".to_string()));
    app.overlay = None;
    app.find.folders = false;
    find(&mut app, "*", "");
    assert_eq!(found_names(&app), ["a.txt", "b.txt", "c.rs"]);
}

#[test]
fn a_text_finds_the_files_holding_it() {
    let (_temp, mut app) = app();
    find(&mut app, "", "HELLO");
    assert_eq!(found_names(&app), ["b.txt"]);
    assert_eq!(app.find.mask, "*", "an empty mask is FAR's all files");
}

#[test]
fn go_to_puts_the_panel_on_the_found_file() {
    let (temp, mut app) = app();
    find(&mut app, "b.txt", "");
    assert_eq!(results(&app).form.focus, crate::app::find::results::GO_TO);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    let sub = temp.path().join("sub");
    until(&mut app, |app| {
        app.panels[0].path == sub.as_path() && app.panels[0].reading().is_none()
    });
    let current = app.panels[0].current().map(|entry| entry.name.clone());
    assert_eq!(current.as_deref(), Some("b.txt"));
}

#[test]
fn f3_views_a_found_file_and_the_results_come_back() {
    let (temp, mut app) = app();
    find(&mut app, "*.txt", "");
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    let Screen::Viewer(viewer) = &app.screen else {
        panic!("no viewer");
    };
    assert_eq!(viewer.path, temp.path().join("a.txt"));
    assert_eq!(
        viewer.list,
        [
            temp.path().join("a.txt"),
            temp.path().join("sub").join("b.txt")
        ]
    );
    let gray_plus = KeyEvent::new_with_kind_and_state(
        KeyCode::Char('+'),
        KeyModifiers::NONE,
        ratatui::crossterm::event::KeyEventKind::Press,
        ratatui::crossterm::event::KeyEventState::KEYPAD,
    );
    app.handle_key(gray_plus);
    let Screen::Viewer(viewer) = &app.screen else {
        panic!("no viewer");
    };
    assert_eq!(viewer.path, temp.path().join("sub").join("b.txt"));
    assert_eq!(viewer.list.len(), 2, "the list stays");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.tick();
    assert_eq!(found_names(&app), ["a.txt", "b.txt"]);
}

#[test]
fn esc_asks_and_yes_stops_the_search_no_lets_it_go_on() {
    let (_temp, mut app) = app();
    let search = crate::jobs::find::Search::stalled(Vec::new(), false);
    let stalled = FindResults::new(search, "Find file: *".to_string(), None);
    app.overlay = Some(Overlay::FindResults(Box::new(stalled)));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm { title, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(title, "Operation has been interrupted");
    assert_eq!(app.find.results.as_ref().unwrap().told(), (false, true));
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
    assert_eq!(results(&app).told(), (false, false), "No: it goes on");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
    assert_eq!(results(&app).told(), (true, false), "Yes: it stops");
    assert!(
        app.overlay.is_some(),
        "the dialog stays with what was found"
    );
}

#[test]
fn a_wrong_mask_warns_and_the_dialog_comes_back() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).set_edit_text(MASK, " , ");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("no warning");
    };
    assert_eq!(lines, &["File-mask string contains errors"]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
    assert_eq!(form(&mut app).edit_text(MASK), " , ");
}

#[test]
fn the_areas_take_their_folders() {
    let (temp, mut app) = app();
    let mask = FileMask::parse("*").unwrap();
    let folder = VfsPath::local(temp.path());
    app.find.area = Some(Area::Current);
    let current = app.find_request(mask.clone());
    assert_eq!(
        (current.roots, current.recurse),
        (vec![folder.clone()], false)
    );
    app.find.area = Some(Area::Root);
    assert_eq!(app.find_request(mask.clone()).roots, vec![folder.root()]);
    app.find.area = Some(Area::Selected);
    app.panels[0]
        .selected
        .extend(["sub".to_string(), "a.txt".to_string()]);
    assert_eq!(
        app.find_request(mask.clone()).roots,
        vec![folder.join("sub")]
    );
    app.find.area = Some(Area::LocalDrives);
    let drives = app.find_request(mask);
    assert_eq!(
        drives.drives,
        Some(crate::jobs::find::Drives { network: false })
    );
    assert!(drives.roots.is_empty());
}

#[test]
fn path_is_split_as_fars_tokens_with_quotes() {
    let folders = path_folders("C:\\a;;\"C:\\b;c\"; ");
    let expected = [Path::new("C:\\a"), Path::new("C:\\b;c")];
    assert_eq!(folders, expected.map(VfsPath::local));
}

#[test]
fn the_choices_are_kept_and_some_saved() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    let dialog = form(&mut app);
    dialog.set_edit_text(MASK, "*.rs");
    dialog.set_edit_text(TEXT, "fn");
    dialog.focus_item(FOLDERS);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    app.overlay = None;
    let settings = app.to_settings();
    assert_eq!(settings.find.mask, "*.rs");
    assert!(!settings.find.folders);
    assert_eq!(settings.find.area, "fromCurrent");
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    assert_eq!(form(&mut app).edit_text(TEXT), "fn", "kept for the session");
    let again = App::new(_temp.path(), 100, 40, &settings);
    assert_eq!(again.find.text, "", "the text is not saved");
    assert_eq!(again.find.mask, "*.rs");
}

#[test]
fn commands_lists_find_file_first() {
    let menus = crate::app::menu::MENUS;
    let commands = menus.iter().find(|menu| menu.title == "&Commands");
    let first = &commands.unwrap().items[0];
    let crate::app::menu::MenuItem::Item { label, key, .. } = first else {
        panic!("no item");
    };
    assert_eq!((*label, *key), ("&Find file", "Alt+F7"));
    assert_eq!(first.command(), Some(crate::app::Command::FindFile));
}

#[test]
fn drive_opens_the_drive_menu_and_the_dialog_comes_back() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).set_edit_text(MASK, "*.kept");
    form(&mut app).focus_item(DRIVE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    let Some(Overlay::DriveMenu(menu)) = &app.overlay else {
        panic!("no drive menu");
    };
    assert_eq!(menu.side, app.command_side());
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.tick();
    assert_eq!(form(&mut app).edit_text(MASK), "*.kept");
}

fn press_alt(app: &mut App, letter: char) {
    press(app, KeyCode::Char(letter), KeyModifiers::ALT);
}

#[test]
fn hex_shows_the_text_as_its_bytes_and_finds_them() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).set_edit_text(TEXT, "AB");
    press_alt(&mut app, 'h');
    let dialog = form(&mut app);
    assert!(dialog.items[TEXT].hidden && !dialog.items[HEX].hidden);
    assert!(
        dialog.masked_text(HEX).starts_with("41 42 "),
        "{:?}",
        dialog.masked_text(HEX)
    );
    assert!(dialog.items[FUZZY].disabled, "a text's switches");
    assert_eq!(dialog.focus, HEX, "the hotkey goes on to the line");
    dialog.set_masked_text(HEX, "68 65 6C 6C 6F");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert_eq!(found_names(&app), ["b.txt"]);
    app.overlay = None;
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    assert!(form(&mut app).is_checked(RADIO_HEX), "kept for the session");
    press_alt(&mut app, 't');
    assert_eq!(form(&mut app).edit_text(TEXT), "hello", "back as text");
}

#[test]
fn fuzzy_finds_a_letter_with_its_mark_as_without() {
    let (temp, mut app) = app();
    std::fs::write(temp.path().join("menu.txt"), "un café noir").unwrap();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).set_edit_text(TEXT, "cafe");
    form(&mut app).focus_item(FUZZY);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert_eq!(found_names(&app), ["menu.txt"]);
}

#[test]
fn advanced_limits_the_bytes_read_and_is_saved() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).set_edit_text(TEXT, "there");
    form(&mut app).focus_item(ADVANCED);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Form {
        form: advanced,
        action: FormAction::FindAdvanced,
    })) = &mut app.overlay
    else {
        panic!("no Advanced");
    };
    assert_eq!(advanced.title, "Find file advanced options");
    advanced.set_edit_text(advanced::SEARCH_FIRST, "6");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
    assert_eq!(
        form(&mut app).edit_text(TEXT),
        "there",
        "the dialog is back as it was"
    );
    assert_eq!(
        form(&mut app).focus,
        ADVANCED,
        "the focus where it was, as FAR's"
    );
    form(&mut app).focus_item(MASK);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert!(
        found_names(&app).is_empty(),
        "\"there\" starts past the first 6 bytes"
    );
    app.overlay = None;
    assert_eq!(app.to_settings().find.first, "6");
}

#[test]
fn a_size_written_wrong_warns_and_advanced_stays() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).focus_item(ADVANCED);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Form { form: advanced, .. })) = &mut app.overlay else {
        panic!("no Advanced");
    };
    advanced.set_edit_text(advanced::SEARCH_FIRST, "12X");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Message { title, lines, .. })) = &app.overlay else {
        panic!("no warning");
    };
    assert_eq!(title, "Find file advanced options");
    assert_eq!(lines, &["File size field is incorrectly filled"]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
    let Some(Overlay::Dialog(Dialog::Form {
        form: advanced,
        action,
    })) = &app.overlay
    else {
        panic!("no Advanced again");
    };
    assert_eq!(*action, FormAction::FindAdvanced);
    assert_eq!(advanced.edit_text(advanced::SEARCH_FIRST), "12X");
    assert_eq!(app.find.first, "", "nothing taken");
}

#[test]
fn alternate_streams_are_found_and_go_to_shows_their_file() {
    let (temp, mut app) = app();
    let file = temp.path().join("a.txt");
    std::fs::write(format!("{}:notes", file.display()), "secret").unwrap();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).set_edit_text(TEXT, "secret");
    form(&mut app).focus_item(STREAMS);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert_eq!(found_names(&app), ["a.txt:notes"]);
    assert!(app.to_settings().find.streams);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let current = app.panels[0].current().map(|entry| entry.name.clone());
    assert_eq!(current.as_deref(), Some("a.txt"));
}

#[test]
fn the_text_hotkey_goes_on_to_the_line_when_text_is_on_already() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    press_alt(&mut app, 't');
    assert_eq!(form(&mut app).focus, TEXT);
}

#[test]
fn empty_column_types_keep_the_columns() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    form(&mut app).focus_item(ADVANCED);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Form { form: advanced, .. })) = &mut app.overlay else {
        panic!("no Advanced");
    };
    advanced.set_edit_text(advanced::COLUMN_TYPES, "");
    advanced.set_edit_text(advanced::COLUMN_WIDTHS, "9");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let settings = app.to_settings();
    assert_eq!(
        (
            settings.find.column_types.as_str(),
            settings.find.column_widths.as_str()
        ),
        ("D,S,A", "0,0,0")
    );
}

fn code_page_lines(app: &mut App) -> Vec<ListLine> {
    match form(app).control(dialog::CODE_PAGE) {
        Some(Control::DropDown { lines, .. }) => lines.clone(),
        _ => panic!("no code page list"),
    }
}

/// The code page list's line `at` chosen.
fn choose_code_page(app: &mut App, at: usize) {
    if let Some(Control::DropDown { chosen, .. }) = form(app).control_mut(dialog::CODE_PAGE) {
        *chosen = at;
    }
}

#[test]
fn code_pages_are_marked_with_space_and_ins_and_the_marks_are_saved() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    press_alt(&mut app, 'g');
    assert_eq!(form(&mut app).focus, dialog::CODE_PAGE);
    press(&mut app, KeyCode::Down, KeyModifiers::ALT);
    // All, then over "System" to ANSI, OEM, over "Unicode" to UTF-8.
    for _ in 0..4 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    let lines = code_page_lines(&mut app);
    assert_eq!(lines[1].label(), Some("Selected code pages"));
    assert!(form(&mut app).line_checked(dialog::CODE_PAGE, 6));
    assert!(form(&mut app).line_checked(dialog::CODE_PAGE, 7));
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(form(&mut app).chosen(dialog::CODE_PAGE), 1);
    // Cancel keeps the marks, as FAR writes them at once, and not the choice.
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert_eq!(app.find.marked, [jobs_text::UTF8]);
    assert_eq!(app.find.code_page, CodePageChoice::Auto);
    let mut settings = Settings::default();
    app.find.code_page = CodePageChoice::Page(jobs_text::UTF16_LE);
    app.find.write_to(&mut settings);
    assert_eq!(settings.find.code_pages, [jobs_text::UTF8]);
    assert_eq!(settings.find.code_page, "1200");
    let again = FindState::saved(&settings);
    assert_eq!(
        (again.code_page, again.marked),
        (app.find.code_page, vec![jobs_text::UTF8])
    );
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    assert_eq!(
        form(&mut app).chosen(dialog::CODE_PAGE),
        7,
        "the saved choice"
    );
    assert_eq!(
        code_page_lines(&mut app)[1].label(),
        Some("Selected code pages")
    );
}

#[test]
fn all_standard_code_pages_find_what_detection_misses() {
    let (temp, mut app) = app();
    // No mark and no ASCII: none of the zero bytes "Automatic detection" tells UTF-16 by.
    let big: Vec<u8> = "привет мир"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    std::fs::write(temp.path().join("big.txt"), big).unwrap();
    find(&mut app, "big.txt", "привет");
    assert!(found_names(&app).is_empty(), "taken for ANSI");
    app.overlay = None;
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    choose_code_page(&mut app, 1);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert_eq!(found_names(&app), ["big.txt"]);
    assert_eq!(app.find.code_page, CodePageChoice::All);
}

#[test]
fn hex_turns_the_text_into_the_chosen_code_pages_bytes() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    choose_code_page(&mut app, 7);
    form(&mut app).set_edit_text(TEXT, "AB");
    press_alt(&mut app, 'h');
    assert_eq!(form(&mut app).masked_text(HEX).trim_end(), "41 00 42 00");
    form(&mut app).set_masked_text(HEX, "41 00 04 04");
    press_alt(&mut app, 't');
    assert_eq!(form(&mut app).edit_text(TEXT), "AЄ");
}

#[path = "open_tests.rs"]
mod open;

#[path = "plugin_tests.rs"]
mod plugin;
