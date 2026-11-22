//! A disk's archives and the commands on an archive's file (S6.2d, arclite's `bulk_extract`,
//! `bulk_test`, `test_files` by FAR's `ProcessHostFileW`) through the program and the shipped plugin:
//! Shift+F3 on an archive's panel asks arclite's "Test archive files", tests the entries and says so,
//! the entries unselected; a failure shown with the archive named, the selection kept; Shift+F2 there is
//! F5's copy out; the Plugins menu's "Extract archive(s)" and "Test archive(s)" on a disk's files.

use super::*;
use crate::app::form::Control;

/// The plugin's own menu up (not F11's): its title and lines without their hotkeys' marks.
fn own_menu(app: &App) -> Option<(String, Vec<String>)> {
    match &app.overlay {
        Some(Overlay::PluginMenu(menu)) if menu.commands.is_none() => Some((
            menu.title.clone(),
            (menu.lines.iter())
                .map(|line| line.text.replace('&', ""))
                .collect(),
        )),
        _ => None,
    }
}

/// The plugin's message up: its title and lines.
fn message(app: &App) -> Option<(String, Vec<String>)> {
    let Some(Overlay::Dialog(Dialog::Form {
        form,
        action: FormAction::Plugin(PluginForm::Message { .. }),
    })) = &app.overlay
    else {
        return None;
    };
    let lines = (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Label(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    Some((form.title.clone(), lines))
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn shift(app: &mut App, number: u8) {
    app.handle_key(KeyEvent::new(KeyCode::F(number), KeyModifiers::SHIFT));
}

/// Nothing waited for any more.
fn settled(app: &App) -> bool {
    app.plugins.key_wait.is_none() && app.plugins.key_late.is_empty() && app.overlay.is_none()
}

/// `a.zip` opened on the left panel, `top.txt` and `docs` selected.
fn open_a_zip(app: &mut App) {
    point(app, "a.zip");
    key(app, KeyCode::Enter);
    tick_until(app, |app| in_archive(app, "/"));
    app.panels[0].selected.insert("top.txt".to_string());
    app.panels[0].selected.insert("docs".to_string());
}

/// Shift+F3 on an archive's panel: arclite's menu of one command, its test of the selected entries,
/// "Archive test complete"; then the entries are unselected (arclite clears their `PPIF_SELECTED`,
/// FAR's `PluginClearSelection`), the panel where it was.
#[test]
fn shift_f3_tests_the_selected_entries_and_unselects_them() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archivetest");
    open_a_zip(&mut app);
    shift(&mut app, 3);
    tick_until(&mut app, |app| own_menu(app).is_some());
    let menu = own_menu(&app).unwrap();
    assert_eq!(
        menu,
        (
            "Archive".to_string(),
            vec!["Test archive files".to_string()]
        )
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| message(app).is_some());
    assert_eq!(
        message(&app).unwrap(),
        (
            "Archive".to_string(),
            vec!["Archive test complete".to_string()]
        )
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
    assert!(app.panels[0].selected.is_empty(), "the entries unselected");
    assert!(in_archive(&app, "/"));
}

/// Esc in arclite's menu tests nothing: the selection stays.
#[test]
fn esc_in_the_commands_menu_keeps_the_selection() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archivetestesc");
    open_a_zip(&mut app);
    shift(&mut app, 3);
    tick_until(&mut app, |app| own_menu(app).is_some());
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, settled);
    assert_eq!(app.panels[0].selected.len(), 2);
}

/// A file whose data fails its CRC: arclite's `error_dlg` names the archive, "Errors:", the words, the
/// file and the archive; the selection stays.
#[test]
fn a_failed_test_names_the_archive_and_keeps_the_selection() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivetestbad");
    let path = files.path().join("bad.zip");
    let mut zip = zip_of(&[("data.txt", b"hello world")]);
    let at = zip.windows(11).position(|w| w == b"hello world").unwrap();
    zip[at + 4] = b'X';
    std::fs::write(&path, zip).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    point(&mut app, "bad.zip");
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| in_archive(app, "/"));
    app.panels[0].selected.insert("data.txt".to_string());
    shift(&mut app, 3);
    tick_until(&mut app, |app| own_menu(app).is_some());
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| message(app).is_some());
    let (title, lines) = message(&app).unwrap();
    assert_eq!(title, "Archive");
    let shown = path.display().to_string();
    assert_eq!(
        lines,
        [
            shown.as_str(),
            "Errors:",
            "CRC check failed",
            "data.txt",
            shown.as_str()
        ]
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
    assert_eq!(app.panels[0].selected.len(), 1, "kept");
}

/// Shift+F2 on an archive's panel is F5 (FAR: the plugin names its `HostFile`): arclite's Extract
/// dialog (a disk's panel's Shift+F2 / Shift+F3: `archive_disk_tests`).
#[test]
fn shift_f2_on_an_archives_panel_copies_out() {
    let (_plugins, _files, out, mut app) = app_with_archives("archiveshiftf2");
    point(&mut app, "a.zip");
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| in_archive(app, "/"));
    point(&mut app, "top.txt");
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(&app), Some("Extract files"));
    key(&mut app, KeyCode::Enter);
    let copied = out.path().join("top.txt");
    tick_until(&mut app, |app| {
        settled(app) && std::fs::read(&copied).is_ok_and(|data| data == b"top")
    });
}

#[path = "archive_command_tests.rs"]
mod commands;

#[path = "archive_disk_tests.rs"]
mod disk;

/// F11, the plugin's command, then its own menu's item `index`.
fn archive_menu_item(app: &mut App, index: usize) {
    app.handle_key(KeyEvent::from(KeyCode::F(11)));
    key(app, KeyCode::Enter);
    tick_until(app, |app| own_menu(app).is_some());
    let (_, items) = own_menu(app).unwrap();
    assert_eq!(
        items,
        [
            "Open archive",
            "Open (format menu)",
            "Create archive",
            "Extract archive(s)",
            "Test archive(s)",
            "Convert archive into SFX"
        ]
    );
    for _ in 0..index {
        key(app, KeyCode::Down);
    }
    key(app, KeyCode::Enter);
}

/// "Extract archive(s)": the Extract dialog into the passive panel's folder; each archive with more
/// than one entry at its root into a folder of its name (arclite's "separate directory" asked
/// neither way); the active panel read again with its selection gone (arclite's `update_panel`).
#[test]
fn the_menus_extract_takes_every_selected_archive() {
    let (_plugins, _files, out, mut app) = app_with_archives("archivebulkx");
    app.panels[0].selected.insert("a.zip".to_string());
    app.panels[0].selected.insert("b.docx".to_string());
    archive_menu_item(&mut app, 3);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(&app), Some("Extract files"));
    key(&mut app, KeyCode::Enter);
    let last = out.path().join("b").join("top.txt");
    tick_until(&mut app, |app| settled(app) && last.exists());
    let read = |path: &[&str]| {
        let whole = path
            .iter()
            .fold(out.path().to_path_buf(), |at, part| at.join(part));
        std::fs::read(whole).unwrap()
    };
    assert_eq!(read(&["a", "docs", "readme.txt"]), b"hello");
    assert_eq!(read(&["a", "top.txt"]), b"top");
    assert_eq!(read(&["b", "docs", "readme.txt"]), b"hello");
    tick_until(&mut app, |app| app.panels[0].selected.is_empty());
}

/// "Test archive(s)": a file that is no archive goes into the log, offered at the end; archives that
/// pass: "Archive test complete".
#[test]
fn the_menus_test_logs_a_file_that_is_no_archive() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archivebulkt");
    app.panels[0].selected.insert("a.zip".to_string());
    app.panels[0].selected.insert("plain.txt".to_string());
    archive_menu_item(&mut app, 4);
    tick_until(&mut app, |app| message(app).is_some());
    let (title, _) = message(&app).unwrap();
    assert_eq!(title, "Operation log");
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, settled);
    app.panels[0].selected.clear();
    app.panels[0].selected.insert("a.zip".to_string());
    app.panels[0].selected.insert("c.zip".to_string());
    archive_menu_item(&mut app, 4);
    tick_until(&mut app, |app| message(app).is_some());
    assert_eq!(
        message(&app).unwrap(),
        (
            "Archive".to_string(),
            vec!["Archive test complete".to_string()]
        )
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
}

/// "Go to target directory" kept on (arclite's `extract_open_dir`): a lone archive of one entry at its
/// root goes into the passive panel's folder itself and the active panel goes there, onto that entry
/// (arclite's `panel_go_to_file`).
#[test]
fn the_menus_extract_goes_to_a_lone_archives_one_entry() {
    let plugins = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(plugins.path(), "archive", "archivegoto");
    let kept: serde_json::Map<String, serde_json::Value> = [(
        "plugins.archivegoto.settings.extract_open_dir".to_string(),
        serde_json::Value::String("1".to_string()),
    )]
    .into_iter()
    .collect();
    let host = crate::plugin_host::PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &kept,
        &|_| true,
    );
    let files = tempfile::tempdir().unwrap();
    std::fs::write(
        files.path().join("one.zip"),
        zip_of(&[("only.txt", b"only")]),
    )
    .unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut app = app_in(plugins.path());
    app.set_plugins(host);
    app.panels[0]
        .change_dir(&VfsPath::local(files.path()), None)
        .unwrap();
    app.panels[1]
        .change_dir(&VfsPath::local(out.path()), None)
        .unwrap();
    app.active = Side::Left;
    tick_until(&mut app, |app| {
        app.panels.iter().all(|panel| panel.reading().is_none())
            && (app.plugins.host.plugins.iter()).any(|plugin| plugin.file_masks == ["*"])
    });
    point(&mut app, "one.zip");
    archive_menu_item(&mut app, 3);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    key(&mut app, KeyCode::Enter);
    let there = VfsPath::local(out.path());
    tick_until(&mut app, |app| {
        settled(app)
            && shows(app, Side::Left, &there)
            && app.panels[0]
                .current()
                .is_some_and(|entry| entry.name == "only.txt")
    });
    assert_eq!(std::fs::read(out.path().join("only.txt")).unwrap(), b"only");
}
