//! Deleting from an archive's panel and making a folder in it (S6.2b, FAR's `DeleteFilesW` and
//! `MakeDirectoryW` with arclite) through the program and the shipped plugin: F8 asks arclite's question
//! (not the program's), OK writes the archive anew without the entry and what is under it, Cancel changes
//! nothing; F7 asks arclite's "Enter directory name:", the folder written and the cursor on it; F6 out of
//! the archive deletes what was extracted.

use super::*;

/// The plugin's window up: its kind's name and title.
fn plugin_window(app: &App) -> Option<(&'static str, String)> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Plugin(kind),
        })) => {
            let name = match kind {
                PluginForm::Message { .. } => "message",
                PluginForm::Input { .. } => "input",
                PluginForm::Dialog => "dialog",
                PluginForm::Busy => "busy",
            };
            Some((name, form.title.clone()))
        }
        _ => None,
    }
}

/// A key pressed and let go (a second press of a key never let go is its auto-repeat: F8's ignored).
fn press(app: &mut App, code: KeyCode) {
    press_with(app, code, KeyModifiers::NONE);
}

/// [`press`] with `modifiers` held.
fn press_with(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    let released = KeyEvent::new_with_kind(
        code,
        modifiers,
        ratatui::crossterm::event::KeyEventKind::Release,
    );
    app.handle_key(released);
}

/// The zip `name` of `files` written beside `a.zip`, the left panel read again showing it.
fn add_zip(
    app: &mut App,
    folder: &std::path::Path,
    name: &str,
    files: &[(&str, &[u8])],
) -> std::path::PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, zip_of(files)).unwrap();
    app.panels[0].reload();
    tick_until(app, |app| {
        app.panels[0].reading().is_none() && names(app).contains(&name.to_string())
    });
    path
}

/// The archive `name` of the left panel opened by Enter, at its root.
fn open_archive(app: &mut App, name: &str) {
    point(app, name);
    press(app, KeyCode::Enter);
    let title = format!("Archive:zip:{name}");
    tick_until(app, |app| {
        in_archive(app, "/") && app.panels[0].look.title.as_deref() == Some(title.as_str())
    });
}

/// The program's error window up: its lines.
fn error_lines(app: &App) -> Option<Vec<String>> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Message {
            lines, error: true, ..
        })) => Some(lines.clone()),
        _ => None,
    }
}

/// Waits for the program's error window and takes it away: its lines as one text.
fn error_told(app: &mut App) -> String {
    tick_until(app, |app| error_lines(app).is_some());
    let told = error_lines(app).unwrap_or_default().join("\n");
    app.overlay = None;
    told
}

/// Waits until no press of the plugin is waited for.
fn settled(app: &mut App) {
    tick_until(app, |app| {
        app.plugins.key_wait.is_none() && app.plugins.key_late.is_empty()
    });
}

#[test]
fn f8_deletes_by_arclites_question() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivedelete");
    point(&mut app, "a.zip");
    press(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert!(app.panels[0].look.deletes && app.panels[0].look.makes_folders);
    let archive = files.path().join("a.zip");
    let before = std::fs::read(&archive).unwrap();
    point(&mut app, "docs");
    press(&mut app, KeyCode::F(8));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    assert_eq!(
        plugin_window(&app),
        Some(("message", "Archive".to_string())),
        "arclite's question, not the program's"
    );
    press(&mut app, KeyCode::Esc);
    settled(&mut app);
    assert_eq!(std::fs::read(&archive).unwrap(), before, "Cancel: nothing");
    point(&mut app, "docs");
    press(&mut app, KeyCode::F(8));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    tick_until(&mut app, |app| {
        in_archive(app, "/") && !names(app).contains(&"docs".to_string())
    });
    assert_eq!(zip_names(&archive), ["top.txt"], "the folder with its file");
}

#[test]
fn f7_makes_a_folder_by_arclites_name_dialog() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivemkdir");
    point(&mut app, "a.zip");
    press(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| in_archive(app, "/"));
    press(&mut app, KeyCode::F(7));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    assert_eq!(
        plugin_window(&app),
        Some(("input", "Archive".to_string())),
        "arclite's input, not the program's"
    );
    for letter in "made".chars() {
        press(&mut app, KeyCode::Char(letter));
    }
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    let archive = files.path().join("a.zip");
    tick_until(&mut app, |app| {
        in_archive(app, "/")
            && app.panels[0]
                .current()
                .is_some_and(|entry| entry.name == "made")
    });
    assert!(zip_names(&archive).contains(&"made".to_string()));
    assert!(app.panels[0].current().is_some_and(|entry| entry.is_dir));
}

#[test]
fn f6_out_of_an_archive_deletes_what_was_extracted() {
    let (_plugins, files, out, mut app) = app_with_archives("archivemoveout");
    point(&mut app, "a.zip");
    press(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| in_archive(app, "/"));
    point(&mut app, "top.txt");
    press(&mut app, KeyCode::F(6));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    assert_eq!(
        plugin_window(&app),
        Some(("dialog", "Extract files".to_string()))
    );
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    let archive = files.path().join("a.zip");
    tick_until(&mut app, |_| {
        std::fs::read(out.path().join("top.txt")).is_ok_and(|data| data == b"top")
            && !zip_names(&archive).contains(&"top.txt".to_string())
    });
    assert_eq!(zip_names(&archive), ["docs/readme.txt"]);
}

/// An archive with several items of one name is written anew as 7-Zip writes it (every item kept): F8
/// deletes another entry by arclite's question, both items of the name stay; F7 makes a folder.
#[test]
fn f8_and_f7_on_an_archive_with_items_of_one_name_keep_them_all() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivedupdelete");
    let twice: &[(&str, &[u8])] = &[("d.txt", b"one"), ("d.txt", b"two"), ("top.txt", b"top")];
    let archive = add_zip(&mut app, files.path(), "dup.zip", twice);
    open_archive(&mut app, "dup.zip");
    point(&mut app, "top.txt");
    press(&mut app, KeyCode::F(8));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    tick_until(&mut app, |_| {
        !zip_names(&archive).contains(&"top.txt".to_string())
    });
    assert_eq!(zip_names(&archive), ["d.txt", "d.txt"], "both kept");
    tick_until(&mut app, |app| in_archive(app, "/"));
    press(&mut app, KeyCode::F(7));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    for letter in "made".chars() {
        press(&mut app, KeyCode::Char(letter));
    }
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    tick_until(&mut app, |_| {
        zip_names(&archive).contains(&"made".to_string())
    });
    assert_eq!(
        zip_names(&archive)
            .iter()
            .filter(|name| *name == "d.txt")
            .count(),
        2
    );
}

/// Shift+F8 (FAR's `DeleteCurrent`): the entry under the cursor alone goes, whatever is selected; the
/// selected one stays in the archive and selected.
#[test]
fn shift_f8_deletes_the_entry_under_the_cursor_only() {
    let (_plugins, files, _out, mut app) = app_with_archives("archiveshiftdelete");
    open_archive(&mut app, "a.zip");
    app.panels[0].selected.insert("docs".to_string());
    point(&mut app, "top.txt");
    press_with(&mut app, KeyCode::F(8), KeyModifiers::SHIFT);
    tick_until(&mut app, |app| plugin_window(app).is_some());
    assert_eq!(
        plugin_window(&app),
        Some(("message", "Archive".to_string()))
    );
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    let archive = files.path().join("a.zip");
    tick_until(&mut app, |app| {
        in_archive(app, "/") && !names(app).contains(&"top.txt".to_string())
    });
    assert_eq!(
        zip_names(&archive),
        ["docs/readme.txt"],
        "the selected kept"
    );
    assert!(names(&app).contains(&"docs".to_string()));
    assert!(
        app.panels[0].selected.contains("docs"),
        "the selection kept"
    );
}

/// F7's folder made (FAR's `Update(UPDATE_KEEP_SELECTION)` and `GoToFile`): the selection stays as it
/// was, the cursor on the new folder.
#[test]
fn f7_keeps_the_selection_and_puts_the_cursor_on_the_folder_made() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivemkdirselected");
    open_archive(&mut app, "a.zip");
    app.panels[0].selected.insert("top.txt".to_string());
    point(&mut app, "top.txt");
    press(&mut app, KeyCode::F(7));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    for letter in "new".chars() {
        press(&mut app, KeyCode::Char(letter));
    }
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    tick_until(&mut app, |app| {
        in_archive(app, "/") && names(app).contains(&"new".to_string())
    });
    let panel = &app.panels[0];
    assert_eq!(
        panel.current().map(|entry| entry.name.as_str()),
        Some("new")
    );
    let selected: Vec<&str> = panel.selected.iter().map(String::as_str).collect();
    assert_eq!(selected, ["top.txt"]);
    assert!(zip_names(&files.path().join("a.zip")).contains(&"new".to_string()));
}

/// A file and a folder of one name (the zip's "x" and "x/y.txt"): F8 on the name is refused - the other
/// might go in its place -, the archive as it was; another name of it is deleted as any.
#[test]
fn f8_on_a_name_of_a_file_and_a_folder_is_refused() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivetwindelete");
    let twins: &[(&str, &[u8])] = &[("x/y.txt", b"1"), ("x", b"2"), ("z", b"3")];
    let archive = add_zip(&mut app, files.path(), "twins.zip", twins);
    let before = std::fs::read(&archive).unwrap();
    open_archive(&mut app, "twins.zip");
    point(&mut app, "x");
    press(&mut app, KeyCode::F(8));
    let told = error_told(&mut app);
    assert!(told.contains("both named \"x\""), "{told}");
    settled(&mut app);
    assert_eq!(std::fs::read(&archive).unwrap(), before, "unchanged");
    point(&mut app, "z");
    press(&mut app, KeyCode::F(8));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    tick_until(&mut app, |_| {
        !zip_names(&archive).contains(&"z".to_string())
    });
    assert_eq!(zip_names(&archive), ["x/y.txt", "x"]);
}

#[path = "archive_move_tests.rs"]
mod moving;
