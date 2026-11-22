//! Files put into an archive's panel (S6.2a, FAR's `PutFilesW` with arclite) through the program, and
//! the program's side of a put: Esc in "Add files to archive" changes nothing and keeps the selection, OK
//! takes it; an archive with one name twice is refused whole; F6 deletes the source once it is in; F4's
//! edited copy goes back through the plugin (its folder gone once it is in, kept and said where when
//! not); one put waited for at a time; a put's end or failure taken by its own plugin only.

use super::*;
use crate::app::plugins::panels::Putting;

/// The names and sizes in the zip `path`.
fn zip_sizes(path: &std::path::Path) -> Vec<(String, u64)> {
    let Some(opened) = archive::support::opened(path) else {
        return Vec::new();
    };
    (opened.items.iter())
        .map(|item| (item.path.join("/"), item.size))
        .collect()
}

/// The `.tmp` files left in `folder`.
fn temps(folder: &std::path::Path) -> usize {
    (std::fs::read_dir(folder).unwrap().flatten())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count()
}

/// The archive `name` of the left panel's folder opened in it.
fn open_archive(app: &mut App, name: &str) {
    point(app, name);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(app, |app| in_archive(app, "/"));
}

/// The file `name` (with `text`) of the right panel's folder selected, the cursor on it, that panel
/// active.
fn select_on_the_right(app: &mut App, out: &std::path::Path, name: &str, text: &str) {
    std::fs::write(out.join(name), text).unwrap();
    app.panels[1].reload();
    tick_until(app, |app| {
        app.panels[1].reading().is_none()
            && (app.panels[1].entries.iter()).any(|entry| entry.name == name)
    });
    app.active = Side::Right;
    let panel = &mut app.panels[1];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == name)
        .unwrap();
    panel.selected.insert(name.to_string());
}

/// F5 (or F6) and the program's copy dialog's OK.
fn copy_or_move(app: &mut App, moving: bool) {
    app.handle_key(KeyEvent::new(
        KeyCode::F(if moving { 6 } else { 5 }),
        KeyModifiers::NONE,
    ));
    assert!(app.overlay.is_some(), "the program's copy dialog");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn the_plugins_dialog(app: &mut App) {
    tick_until(app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(app), Some("Add files to archive"));
}

/// Esc in "Add files to archive": the archive as it was, no new file beside it, the file and its
/// selection kept. Then OK: the file in the archive and the selection gone (FAR's, after `PutFiles`).
#[test]
fn esc_in_the_plugins_dialog_changes_nothing() {
    let (_plugins, files, out, mut app) = app_with_archives("archiveputesc");
    open_archive(&mut app, "a.zip");
    let archive = files.path().join("a.zip");
    let before = std::fs::read(&archive).unwrap();
    select_on_the_right(&mut app, out.path(), "added.txt", "added");
    copy_or_move(&mut app, false);
    the_plugins_dialog(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    tick_until(&mut app, |app| app.plugins.putting.is_none());
    assert_eq!(
        std::fs::read(&archive).unwrap(),
        before,
        "the archive whole"
    );
    assert_eq!(temps(files.path()), 0, "no new file left");
    assert!(app.panels[1].selected.contains("added.txt"), "Esc: kept");
    copy_or_move(&mut app, false);
    the_plugins_dialog(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| app.plugins.putting.is_none());
    assert_eq!(
        zip_sizes(&archive),
        [
            ("docs/readme.txt".to_string(), 5),
            ("top.txt".to_string(), 3),
            ("added.txt".to_string(), 5)
        ]
    );
    assert!(
        app.panels[1].selected.is_empty(),
        "done: the selection goes"
    );
    assert_eq!(temps(files.path()), 0);
}

/// An archive with one name twice takes a file by arclite's dialog as 7-Zip writes it: every item kept,
/// the new one added.
#[test]
fn an_archive_with_one_name_twice_takes_a_file() {
    let (_plugins, files, out, mut app) = app_with_archives("archiveputtwice");
    let archive = files.path().join("twice.zip");
    std::fs::write(&archive, zip_of(&[("a.txt", b"one"), ("a.txt", b"two")])).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| {
        app.panels[0].reading().is_none()
            && (app.panels[0].entries.iter()).any(|entry| entry.name == "twice.zip")
    });
    open_archive(&mut app, "twice.zip");
    select_on_the_right(&mut app, out.path(), "added.txt", "added");
    copy_or_move(&mut app, false);
    tick_until(&mut app, |app| {
        plugin_dialog(app) == Some("Add files to archive")
    });
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.plugins.putting.is_none()
            && zip_sizes(&archive)
                .iter()
                .any(|(name, _)| name == "added.txt")
    });
    let names: Vec<String> = zip_sizes(&archive)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names.iter().filter(|name| *name == "a.txt").count(),
        2,
        "{names:?}"
    );
    assert_eq!(temps(files.path()), 0);
}

/// F6: the file in the archive, then deleted from its folder.
#[test]
fn f6_moves_a_file_into_the_archive() {
    let (_plugins, files, out, mut app) = app_with_archives("archiveputmove");
    open_archive(&mut app, "a.zip");
    select_on_the_right(&mut app, out.path(), "moved.txt", "moved");
    copy_or_move(&mut app, true);
    the_plugins_dialog(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| app.plugins.putting.is_none());
    let archive = files.path().join("a.zip");
    assert!(zip_sizes(&archive).contains(&("moved.txt".to_string(), 5)));
    assert!(!out.path().join("moved.txt").exists(), "moved");
}

/// The new archive not put in the old one's place (the old one held open by another, its deletion not
/// shared): the failure told, the archive whole, the new one gone, the panel read again from it; a move
/// deletes no source then.
#[test]
fn a_replace_that_fails_leaves_the_archive_whole() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_plugins, files, out, mut app) = app_with_archives("archiveputheld");
    open_archive(&mut app, "a.zip");
    let archive = files.path().join("a.zip");
    let before = std::fs::read(&archive).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1)
        .open(&archive)
        .unwrap();
    select_on_the_right(&mut app, out.path(), "moved.txt", "moved");
    copy_or_move(&mut app, true);
    the_plugins_dialog(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.plugins.putting.is_none() && plugin_dialog(app).is_none() && app.overlay.is_some()
    });
    let (lines, error) = message(&app);
    assert!(error, "the failure told");
    assert!(
        lines.len() == 1 && lines[0].ends_with("(os error 5)"),
        "the replace's own failure: {lines:?}"
    );
    drop(held);
    assert_eq!(
        std::fs::read(&archive).unwrap(),
        before,
        "the archive whole"
    );
    assert_eq!(temps(files.path()), 0, "the new one gone");
    assert!(out.path().join("moved.txt").exists(), "not deleted");
    app.overlay = None;
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(names(&app), ["docs", "top.txt"]);
}

/// F4 on a file of the archive, the copy changed and the editor left: the plugin's dialog, its OK.
fn edit_top(app: &mut App) -> (std::path::PathBuf, std::path::PathBuf) {
    open_archive(app, "a.zip");
    edit_here(app, "top.txt")
}

/// F4 on the file `name` of the archive's panel, the copy changed to "edited top" and the editor left:
/// the plugin's dialog up.
fn edit_here(app: &mut App, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    point(app, name);
    app.handle_key(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
    tick_until(app, |app| matches!(app.screen, Screen::Editor(_)));
    let Screen::Editor(editor) = &app.screen else {
        panic!("the editor");
    };
    let copy = editor.path.clone();
    let folder = copy.parent().unwrap().to_path_buf();
    assert_eq!(app.extracted.len(), 1, "the copy kept while edited");
    std::fs::write(&copy, "edited top").unwrap();
    app.screen = Screen::Panels;
    assert!(app.pump_extracted(), "put back");
    the_plugins_dialog(app);
    (folder, copy)
}

/// F4's edited copy goes back through the plugin (FAR's `PutFiles` with `OPM_EDIT`): in the file's place,
/// none added; its folder gone once it is in.
#[test]
fn an_edited_copy_goes_back_into_the_archive() {
    let (_plugins, files, _out, mut app) = app_with_archives("archiveputedit");
    let (folder, _) = edit_top(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| app.plugins.putting.is_none());
    assert_eq!(
        zip_sizes(&files.path().join("a.zip")),
        [
            ("docs/readme.txt".to_string(), 5),
            ("top.txt".to_string(), 10)
        ]
    );
    tick_until(&mut app, |_| !folder.exists());
    assert!(app.overlay.is_none(), "nothing told");
}

/// An edited copy of a file in a folder of the archive goes back into that folder, in its place.
#[test]
fn an_edited_copy_goes_back_into_its_folder() {
    let (_plugins, files, _out, mut app) = app_with_archives("archiveputeditdocs");
    open_archive(&mut app, "a.zip");
    enter(&mut app, Side::Left, "docs");
    tick_until(&mut app, |app| in_archive(app, "/docs"));
    edit_here(&mut app, "readme.txt");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| app.plugins.putting.is_none());
    assert_eq!(
        zip_sizes(&files.path().join("a.zip")),
        [
            ("docs/readme.txt".to_string(), 10),
            ("top.txt".to_string(), 3)
        ]
    );
    tick_until(&mut app, |app| in_archive(app, "/docs"));
    assert_eq!(names(&app), ["readme.txt"]);
}

/// Esc in the plugin's dialog for an edited copy: the archive as it was, the copy kept and said where.
#[test]
fn an_edited_copy_not_taken_is_kept() {
    let (_plugins, files, _out, mut app) = app_with_archives("archiveputeditesc");
    let archive = files.path().join("a.zip");
    let before = std::fs::read(&archive).unwrap();
    let (_, copy) = edit_top(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.plugins.putting.is_none() && plugin_dialog(app).is_none() && app.overlay.is_some()
    });
    let (lines, error) = message(&app);
    assert!(error);
    assert_eq!(
        lines,
        [format!("The edited copy is kept: {}", copy.display())]
    );
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "edited top");
    assert_eq!(std::fs::read(&archive).unwrap(), before);
    assert_eq!(temps(files.path()), 0);
}

/// A folder with an edited copy in it, as the program keeps one.
fn edited_copy(temp: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let folder = temp.path().join("copy");
    std::fs::create_dir_all(&folder).unwrap();
    let copy = folder.join("top.txt");
    std::fs::write(&copy, "edited").unwrap();
    (folder, copy)
}

/// An edited copy while another put is waited for: not sent, kept and said where; the put waited for
/// stays as it was.
#[test]
fn one_put_is_waited_for_at_a_time() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let first = Putting {
        plugin: "first".to_string(),
        selection: Some(Side::Right),
        edited: None,
        saved: None,
    };
    app.plugins.putting = Some(first.clone());
    let (folder, copy) = edited_copy(&temp);
    let target = VfsPath::in_session("second", "1", "/");
    app.put_edited_back(&target, folder.clone(), copy.clone());
    assert_eq!(app.plugins.putting, Some(first), "the first one's");
    let (lines, error) = message(&app);
    assert!(error);
    assert_eq!(
        lines,
        [format!("The edited copy is kept: {}", copy.display())]
    );
    assert!(copy.exists() && folder.exists());
}

/// A put's end is its own plugin's (whatever the case of its name): the selection goes and an edited
/// copy's folder goes when done; when not, both stay and the copy is said kept. Another plugin's end
/// takes nothing.
#[test]
fn a_puts_end_is_its_plugins() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (folder, copy) = edited_copy(&temp);
    app.plugins.putting = Some(Putting {
        plugin: "Taker".to_string(),
        selection: Some(Side::Right),
        edited: Some((folder.clone(), copy.clone())),
        saved: None,
    });
    app.panels[1].selected.insert("a.txt".to_string());
    app.put_ended("other", true);
    assert!(app.plugins.putting.is_some(), "not its end");
    assert!(folder.exists() && !app.panels[1].selected.is_empty());
    app.put_ended("taker", false);
    assert!(app.plugins.putting.is_none());
    assert!(copy.exists(), "not taken: kept");
    assert!(app.panels[1].selected.contains("a.txt"), "not done: kept");
    let (lines, _) = message(&app);
    assert_eq!(
        lines,
        [format!("The edited copy is kept: {}", copy.display())]
    );
    app.overlay = None;
    app.plugins.putting = Some(Putting {
        plugin: "taker".to_string(),
        selection: Some(Side::Right),
        edited: Some((folder.clone(), copy)),
        saved: None,
    });
    app.put_ended("TAKER", true);
    assert!(app.plugins.putting.is_none());
    assert!(!folder.exists(), "taken: its folder gone");
    assert!(
        app.panels[1].selected.is_empty(),
        "done: the selection goes"
    );
    assert!(app.overlay.is_none(), "nothing told");
}

/// A put's failure: its own plugin's put goes, with the line telling where an edited copy is kept (none
/// for files put by F5 / F6); another plugin's failure takes nothing.
#[test]
fn a_puts_failure_tells_where_an_edited_copy_is() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (folder, copy) = edited_copy(&temp);
    app.plugins.putting = Some(Putting {
        plugin: "taker".to_string(),
        selection: None,
        edited: Some((folder.clone(), copy.clone())),
        saved: None,
    });
    assert_eq!(app.put_failed("other"), None);
    assert!(app.plugins.putting.is_some(), "not its failure");
    assert_eq!(
        app.put_failed("Taker"),
        Some(format!("The edited copy is kept: {}", copy.display()))
    );
    assert!(app.plugins.putting.is_none());
    assert!(copy.exists() && folder.exists(), "kept");
    app.plugins.putting = Some(Putting {
        plugin: "taker".to_string(),
        selection: Some(Side::Left),
        edited: None,
        saved: None,
    });
    assert_eq!(app.put_failed("taker"), None, "no copy");
    assert!(app.plugins.putting.is_none());
}
