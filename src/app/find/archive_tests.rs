//! Alt+F7's "Search in archives" (S6.7, FAR's `ArchiveSearch`) through the program with the archive
//! plugin as it is shipped: what is inside a zip found under FAR's `D:\a.zip:\dir\` lines, its texts read,
//! the archive closed after; Go to opening it over its file at the found file, F3 / F4 on a copy, Panel
//! handing the archive's file; the box off on a plugin's panel without real names.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::tests::{form, found_names, press, results, until};
use super::*;
use crate::app::Screen;
use crate::app::dialog::Dialog;
use crate::app::fileops::tests::app_in;
use crate::app::find::results::ResultLine;
use crate::app::{Overlay, Side};
use crate::plugin_host::tests::{install_shipped, load};
use archive::support::zip_of;

/// The archive plugin installed as `name` (each test its own: the providers are the process's), the
/// left panel on a folder with `a.zip` (`docs\readme.txt` "hello", `top.txt` "top") and `plain.txt`.
fn app_with_zip(name: &str) -> (tempfile::TempDir, tempfile::TempDir, App) {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "archive", name);
    let files = tempfile::tempdir().unwrap();
    let zip = zip_of(&[("docs/readme.txt", b"hello"), ("top.txt", b"top")]);
    std::fs::write(files.path().join("a.zip"), zip).unwrap();
    std::fs::write(files.path().join("plain.txt"), "plain").unwrap();
    let mut app = app_in(files.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    app.active = Side::Left;
    until(&mut app, |app| {
        app.panels[0].reading().is_none()
            && (app.panels[0].entries.iter()).any(|entry| entry.name == "a.zip")
            && (app.plugins.host.plugins.iter()).any(|plugin| plugin.file_masks == ["*"])
    });
    (plugins, files, app)
}

/// Alt+F7 with "Search in archives" checked, these masks and text, Find, the search to its end.
fn find_in_archives(app: &mut App, mask: &str, text: &str) {
    app.find.archives = true;
    press(app, KeyCode::F(7), KeyModifiers::ALT);
    assert!(form(app).is_checked(dialog::ARCHIVES));
    form(app).set_edit_text(dialog::MASK, mask);
    form(app).set_edit_text(dialog::TEXT, text);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    until(app, |app| results(app).finalized);
}

/// The results' lines: a folder's as the dialog shows it, a found object's name.
fn lines(app: &App, files: &std::path::Path) -> Vec<String> {
    let results = results(app);
    let temp = files.display().to_string();
    (results.lines.iter())
        .filter_map(|line| match line {
            ResultLine::Folder(folder) => Some(folder.shown().replace(&temp, "T")),
            ResultLine::Found(at) => Some(results.found[*at].entry.name.clone()),
            ResultLine::Separator => None,
        })
        .collect()
}

/// The cursor of the results on the found object `name`.
fn point_found(app: &mut App, name: &str) {
    let Some(Overlay::FindResults(results)) = &mut app.overlay else {
        panic!("no results");
    };
    results.cursor = (results.lines.iter())
        .position(
            |line| matches!(line, ResultLine::Found(at) if results.found[*at].entry.name == name),
        )
        .unwrap();
}

#[test]
fn what_is_inside_an_archive_is_found_under_its_place_and_the_archive_closed_after() {
    let (_plugins, files, mut app) = app_with_zip("findarcnames");
    find_in_archives(&mut app, "*.txt", "");
    assert_eq!(
        lines(&app, files.path()),
        [
            r"T\a.zip:\",
            "top.txt",
            r"T\a.zip:\docs",
            "readme.txt",
            "T",
            "plain.txt"
        ]
    );
    assert!(app.find.archives, "kept for the next search");
    // Closed by the plugin once walked: the file is let go (opened sharing nothing, which any other
    // handle of it refuses).
    let deadline = Instant::now() + Duration::from_secs(10);
    while !opens_alone(&files.path().join("a.zip")) {
        assert!(Instant::now() < deadline, "the archive is still open");
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Whether `path` opens with no sharing at all: no other handle of it is open.
#[cfg(windows)]
fn opens_alone(path: &std::path::Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .is_ok()
}

#[cfg(not(windows))]
fn opens_alone(_path: &std::path::Path) -> bool {
    true
}

#[test]
fn a_text_is_looked_for_in_an_archives_files_and_nothing_without_the_box() {
    let (_plugins, files, mut app) = app_with_zip("findarctext");
    find_in_archives(&mut app, "*", "hello");
    // The stored zip holds the bytes itself, then its file inside.
    assert_eq!(
        lines(&app, files.path()),
        ["T", "a.zip", r"T\a.zip:\docs", "readme.txt"]
    );
    app.overlay = None;
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    assert!(form(&mut app).is_checked(dialog::ARCHIVES), "as last given");
    // Its hotkey: "Search in arch&ives".
    press(&mut app, KeyCode::Char('i'), KeyModifiers::ALT);
    assert!(!form(&mut app).is_checked(dialog::ARCHIVES));
    form(&mut app).focus_item(dialog::FIND);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert_eq!(found_names(&app), ["a.zip"], "nothing inside");
}

#[test]
fn go_to_opens_the_archive_over_its_file_on_the_found_file() {
    let (_plugins, files, mut app) = app_with_zip("findarcgoto");
    find_in_archives(&mut app, "readme.txt", "");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| {
        let panel = &app.panels[0];
        panel.reading().is_none()
            && panel
                .path
                .session_path()
                .is_some_and(|(_, at)| at == "/docs")
    });
    let current = app.panels[0].current().map(|entry| entry.name.clone());
    assert_eq!(current.as_deref(), Some("readme.txt"));
    // Up to its root and out: back on the archive's file.
    app.panels[0].cursor = 0;
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| {
        app.panels[0].reading().is_none()
            && app.panels[0]
                .path
                .session_path()
                .is_some_and(|(_, at)| at == "/")
    });
    app.panels[0].cursor = 0;
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| {
        app.panels[0].reading().is_none() && app.panels[0].path == files.path()
    });
    let current = app.panels[0].current().map(|entry| entry.name.clone());
    assert_eq!(current.as_deref(), Some("a.zip"));
}

#[test]
fn f3_views_a_copy_of_a_file_found_in_an_archive() {
    let (_plugins, _files, mut app) = app_with_zip("findarcview");
    find_in_archives(&mut app, "*.txt", "");
    point_found(&mut app, "top.txt");
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    until(&mut app, |app| matches!(app.screen, Screen::Viewer(_)));
    let Screen::Viewer(viewer) = &app.screen else {
        unreachable!();
    };
    let copy = viewer.path.clone();
    assert_eq!(std::fs::read(&copy).unwrap(), b"top");
    assert_eq!(copy.file_name().unwrap(), "top.txt");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    until(&mut app, |_| !copy.exists());
    assert!(
        matches!(app.overlay, Some(Overlay::FindResults(_))),
        "the results back"
    );
}

#[test]
fn an_edited_copy_of_a_file_found_in_an_archive_is_kept_not_put_back() {
    let (_plugins, files, mut app) = app_with_zip("findarcedit");
    find_in_archives(&mut app, "*.txt", "");
    point_found(&mut app, "readme.txt");
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    until(&mut app, |app| matches!(app.screen, Screen::Editor(_)));
    let Screen::Editor(editor) = &app.screen else {
        unreachable!();
    };
    let copy = editor.path.clone();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&copy, "changed").unwrap();
    app.screen = Screen::Panels;
    app.find.results = None;
    app.overlay = None;
    assert!(app.pump_extracted());
    let Some(Overlay::Dialog(Dialog::Message { title, lines, .. })) = &app.overlay else {
        panic!("FAR's Save as: the copy is the user's");
    };
    assert_eq!(title, "Edit");
    assert_eq!(lines[0], "A file found in an archive is not put back.");
    assert!(lines[1].ends_with(&copy.display().to_string()), "{lines:?}");
    assert_eq!(std::fs::read(&copy).unwrap(), b"changed", "the work kept");
    let inside = archive::support::names_in(&files.path().join("a.zip"));
    assert_eq!(
        inside,
        ["docs/readme.txt", "top.txt"],
        "the archive as it was"
    );
}

#[test]
fn panel_hands_the_archives_file_for_what_was_found_in_it() {
    let (_plugins, files, mut app) = app_with_zip("findarcpanel");
    find_in_archives(&mut app, "*.txt", "");
    let entries = results(&app).panel_entries();
    let names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    let zip = files.path().join("a.zip").display().to_string();
    let plain = files.path().join("plain.txt").display().to_string();
    assert_eq!(names, [zip.clone(), zip, plain]);
    assert!(entries[0].real_path.is_some() && !entries[0].is_dir);
    assert_eq!(entries[0].attributes, Default::default(), "no attributes");
}

/// The search opens archives without any window (FAR's `OPM_FIND`): a 7z of encrypted headers is
/// passed by with no password asked, any other format searched in as a zip is.
#[test]
fn archives_are_opened_without_windows_and_locked_ones_passed_by() {
    let (_plugins, files, mut app) = app_with_zip("findarcquiet");
    let testdata =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/archive/testdata");
    std::fs::copy(testdata.join("locked.7z"), files.path().join("locked.7z")).unwrap();
    std::fs::copy(testdata.join("a.7z"), files.path().join("seven.7z")).unwrap();
    // A password window would hold the search (the plugin waits for its answer): it ends.
    find_in_archives(&mut app, "top.txt", "");
    let shown = lines(&app, files.path());
    assert_eq!(
        shown,
        [r"T\a.zip:\", "top.txt", r"T\seven.7z:\", "top.txt"],
        "the locked one's insides not listed"
    );
}

/// TmpPanel does not open a list for the search (its menu would be asked from the search, "Replace"
/// would empty the common panel): only the disk's file is found, not the list's line.
#[test]
fn a_temporary_panels_list_is_not_opened_by_the_search() {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "tmppanel", "findarclist");
    let files = tempfile::tempdir().unwrap();
    let plain = files.path().join("plain.txt");
    std::fs::write(&plain, "plain").unwrap();
    std::fs::write(files.path().join("list.temp"), plain.display().to_string()).unwrap();
    let mut app = app_in(files.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    until(&mut app, |app| {
        app.panels[0].reading().is_none()
            && (app.panels[0].entries.iter()).any(|entry| entry.name == "list.temp")
            && (app.plugins.host.plugins.iter()).any(|plugin| !plugin.file_masks.is_empty())
    });
    find_in_archives(&mut app, "plain.txt", "");
    assert_eq!(lines(&app, files.path()), ["T", "plain.txt"]);
}

#[test]
fn the_box_is_off_and_disabled_on_a_plugins_panel_without_real_names() {
    let (_plugins, files, mut app) = app_with_zip("findarcbox");
    app.find.archives = true;
    let at = (app.panels[0].entries.iter()).position(|entry| entry.name == "a.zip");
    app.panels[0].cursor = at.unwrap();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| {
        app.panels[0].reading().is_none() && app.panels[0].path.session_path().is_some()
    });
    assert!(
        !opens_alone(&files.path().join("a.zip")),
        "held while its panel is open"
    );
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    let form = form(&mut app);
    assert!(form.items[dialog::ARCHIVES].disabled);
    assert!(!form.is_checked(dialog::ARCHIVES));
}

#[path = "archive_more_tests.rs"]
mod more;
