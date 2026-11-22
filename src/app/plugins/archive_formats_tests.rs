//! 7-Zip's formats on panels (S6.3a-b, ADR-0022) through the program and the shipped plugin with
//! 7-Zip's installed library (the development machine has it): Enter on a 7z opens it as arclite does
//! (its title names the format), a tar.gz shows gzip's one item, Ctrl+PgDn opens a 7z of any name by its
//! contents; the info panel's lines are 7-Zip's archive properties; F5 and `arc:x e t` extract and test
//! it through the library, `arc:u d` do not write it yet (S6.3c).

use super::*;

const SEVEN: &[u8] = include_bytes!("../../../plugins/archive/testdata/a.7z");
const TAR_GZ: &[u8] = include_bytes!("../../../plugins/archive/testdata/a.tar.gz");
const LOCKED: &[u8] = include_bytes!("../../../plugins/archive/testdata/locked.7z");

fn run_line(app: &mut App, line: &str) {
    app.command_line.set(line);
    app.run_command_line();
}

fn type_text(app: &mut App, text: &str) {
    for letter in text.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
    }
}

/// Nothing waited for, no window up.
fn settled(app: &App) -> bool {
    app.plugins.key_wait.is_none() && app.plugins.key_late.is_empty() && app.overlay.is_none()
}

/// `arc:`'s commands that write a 7z, through the library (S6.3c): `d` deletes an item, `u` adds a
/// file; its other items kept, still a 7z.
#[test]
fn commands_write_a_7z() {
    let (_plugins, files, _out, mut app) = app_with_7z("sevencmd");
    run_line(&mut app, "arc:d a.7z top.txt");
    tick_until(&mut app, settled);
    run_line(&mut app, "arc:u a.7z plain.txt");
    tick_until(&mut app, settled);
    let bytes = std::fs::read(files.path().join("a.7z")).unwrap();
    assert_eq!(&bytes[..2], b"7z");
    assert_ne!(bytes, SEVEN);
    point(&mut app, "a.7z");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(sorted_names(&app, Side::Left), ["..", "docs", "plain.txt"]);
    assert!(app.panels[0].look.deletes, "a 7z is written");
}

/// `arc:x` extracts a 7z through the library (overwriting, arclite's default), `arc:e` an item of it,
/// `arc:t` tests it.
#[test]
fn commands_extract_and_test_a_7z() {
    let (_plugins, _files, _out, mut app) = app_with_7z("sevenx");
    let out = tempfile::tempdir().unwrap();
    std::fs::write(out.path().join("top.txt"), "old").unwrap();
    let into = out.path().display().to_string();
    run_line(&mut app, &format!(r#"arc:x a.7z "{into}""#));
    let readme = out.path().join("docs").join("readme.txt");
    tick_until(&mut app, |app| settled(app) && readme.exists());
    assert_eq!(std::fs::read(&readme).unwrap(), b"hello");
    assert_eq!(std::fs::read(out.path().join("top.txt")).unwrap(), b"top");
    let only = tempfile::tempdir().unwrap();
    let into = only.path().display().to_string();
    run_line(
        &mut app,
        &format!(r#"arc:e -out:"{into}" a.7z docs\readme.txt"#),
    );
    let alone = only.path().join("readme.txt");
    tick_until(&mut app, |app| settled(app) && alone.exists());
    assert_eq!(std::fs::read(alone).unwrap(), b"hello");
    run_line(&mut app, "arc:t a.7z");
    tick_until(&mut app, |app| !shown(app).is_empty());
    assert!(
        shown(&app).contains("Archive test complete"),
        "{}",
        shown(&app)
    );
}

/// Encrypted headers: arclite's password window; a wrong password asks again (arclite's loop), the
/// right one opens.
#[test]
fn a_wrong_password_is_asked_again() {
    let (_plugins, files, _out, mut app) = app_with_7z("sevenlocked");
    std::fs::write(files.path().join("locked.7z"), LOCKED).unwrap();
    app.panels[0]
        .change_dir(&VfsPath::local(files.path()), None)
        .unwrap();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    point(&mut app, "locked.7z");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| plugin_dialog(app) == Some("Enter password"));
    type_text(&mut app, "wrong");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        plugin_dialog(app) == Some("Enter password") || in_archive(app, "/")
    });
    assert!(!in_archive(&app, "/"), "the wrong password opened nothing");
    type_text(&mut app, "secret");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(sorted_names(&app, Side::Left), ["..", "docs", "top.txt"]);
}

/// The program's dialog up, as text (empty when none).
fn shown(app: &App) -> String {
    match &app.overlay {
        Some(Overlay::Dialog(dialog)) => format!("{dialog:?}"),
        _ => String::new(),
    }
}

/// `app_with_archives` with `a.7z`, `a.tar.gz` and `data.bin` (a 7z) beside them, read again.
fn app_with_7z(name: &str) -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir, App) {
    let (plugins, files, out, mut app) = app_with_archives(name);
    std::fs::write(files.path().join("a.7z"), SEVEN).unwrap();
    std::fs::write(files.path().join("a.tar.gz"), TAR_GZ).unwrap();
    std::fs::write(files.path().join("data.bin"), SEVEN).unwrap();
    app.panels[0]
        .change_dir(&VfsPath::local(files.path()), None)
        .unwrap();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    (plugins, files, out, app)
}

/// Enter on a 7z: its folders and files, the title "Archive:7z:a.7z", a folder entered; the info
/// panel's lines are arclite's of a 7z (the path, "Solid", the counts).
#[test]
fn enter_on_a_7z_opens_it() {
    let (_plugins, files, _out, mut app) = app_with_7z("seven");
    point(&mut app, "a.7z");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(sorted_names(&app, Side::Left), ["..", "docs", "top.txt"]);
    assert_eq!(app.panels[0].look.title.as_deref(), Some("Archive:7z:a.7z"));
    let top = (app.panels[0].entries.iter())
        .find(|entry| entry.name == "top.txt")
        .unwrap();
    assert_eq!(top.size, 3);
    let lines: Vec<(String, String)> = (app.panels[0].look.info_lines.iter())
        .map(|line| (line.text.clone(), line.data.clone()))
        .collect();
    let path = files.path().join("a.7z").display().to_string();
    assert_eq!(lines[0], ("Path".to_string(), path));
    assert!(
        lines.contains(&("Solid".to_string(), "Yes".to_string())),
        "{lines:?}"
    );
    assert!(lines.contains(&("File count".to_string(), "2".to_string())));
    assert!(app.panels[0].look.deletes, "written through the library");
    enter(&mut app, Side::Left, "docs");
    tick_until(&mut app, |app| in_archive(app, "/docs"));
    assert_eq!(names(&app), ["readme.txt"]);
}

/// A tar.gz is gzip's: its one item, the tar, named by the archive (arclite's).
#[test]
fn a_tar_gz_shows_its_tar() {
    let (_plugins, _files, _out, mut app) = app_with_7z("tgz");
    point(&mut app, "a.tar.gz");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(names(&app), ["a.tar"]);
    assert_eq!(
        app.panels[0].look.title.as_deref(),
        Some("Archive:gzip:a.tar.gz")
    );
}

/// Ctrl+PgDn on a 7z of a name of no mask: opened by its contents.
#[test]
fn ctrl_pgdn_opens_a_7z_of_any_name() {
    let (_plugins, _files, _out, mut app) = app_with_7z("sevenpgdn");
    point(&mut app, "data.bin");
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(
        app.panels[0].look.title.as_deref(),
        Some("Archive:7z:data.bin")
    );
}

/// F5 on a file of a 7z: arclite's Extract dialog, the file written into the other panel's folder
/// through the library.
#[test]
fn f5_copies_out_of_a_7z() {
    let (_plugins, _files, out, mut app) = app_with_7z("sevencopy");
    point(&mut app, "a.7z");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    point(&mut app, "top.txt");
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(&app), Some("Extract files"));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let copied = out.path().join("top.txt");
    tick_until(&mut app, |app| {
        settled(app) && std::fs::read(&copied).is_ok_and(|data| data == b"top")
    });
}
