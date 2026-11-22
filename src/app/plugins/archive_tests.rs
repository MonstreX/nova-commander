//! Archives as panels (S6.1a, FAR's arclite) through the program, the plugin's library as it is shipped:
//! Enter on a zip of arclite's masks opens it over its file, Ctrl+PgDn a zip of any name, Enter on such a
//! name does not; its folders entered, ".." at its root back to the file; a file in it copied out (F5).

use super::*;
use crate::app::Screen;
use crate::app::dialog::FormAction;
use crate::app::plugins::PluginForm;
use crate::app::plugins::getting::Getting;
use crate::plugin_host::tests::install_shipped;
use archive::support::zip_of;

/// The plugin installed as `name` (each test its own: the providers' list is the program's), the left
/// panel on a folder with `a.zip`, `b.docx` (a zip too) and `plain.txt`, the right one on an empty folder.
fn app_with_archives(name: &str) -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir, App) {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "archive", name);
    let files = tempfile::tempdir().unwrap();
    let zip = zip_of(&[("docs/readme.txt", b"hello"), ("top.txt", b"top")]);
    std::fs::write(files.path().join("a.zip"), &zip).unwrap();
    std::fs::write(files.path().join("b.docx"), &zip).unwrap();
    std::fs::write(files.path().join("plain.txt"), "plain").unwrap();
    let nested = zip_of(&[("inner.zip", &zip), ("note.txt", b"note")]);
    std::fs::write(files.path().join("c.zip"), &nested).unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut app = app_in(plugins.path());
    app.set_plugins(load_plugins(plugins.path(), &|_| true));
    app.panels[0]
        .change_dir(&VfsPath::local(files.path()), None)
        .unwrap();
    app.panels[1]
        .change_dir(&VfsPath::local(out.path()), None)
        .unwrap();
    app.active = Side::Left;
    // The plugin started: its masks are the program's to ask about.
    tick_until(&mut app, |app| {
        app.panels.iter().all(|panel| panel.reading().is_none())
            && (app.plugins.host.plugins.iter()).any(|plugin| plugin.file_masks == ["*"])
    });
    (plugins, files, out, app)
}

fn point(app: &mut App, name: &str) {
    let panel = &mut app.panels[0];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == name)
        .unwrap();
}

fn in_archive(app: &App, path: &str) -> bool {
    let panel = &app.panels[0];
    panel.reading().is_none()
        && panel.path.provider().is_some()
        && panel.path.session_path().is_some_and(|(_, at)| at == path)
}

fn names(app: &App) -> Vec<String> {
    (app.panels[0].entries.iter())
        .filter(|entry| !entry.is_parent)
        .map(|entry| entry.name.clone())
        .collect()
}

#[test]
fn enter_on_a_zip_opens_it_and_dot_dot_at_its_root_comes_back_to_it() {
    let (_plugins, files, _out, mut app) = app_with_archives("archiveenter");
    point(&mut app, "a.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(sorted_names(&app, Side::Left), ["..", "docs", "top.txt"]);
    assert_eq!(
        app.panels[0].look.title.as_deref(),
        Some("Archive:zip:a.zip")
    );
    let top = (app.panels[0].entries.iter())
        .find(|entry| entry.name == "top.txt")
        .unwrap();
    assert_eq!((top.size, top.allocated), (3, Some(3)), "the packed size");
    enter(&mut app, Side::Left, "docs");
    tick_until(&mut app, |app| in_archive(app, "/docs"));
    assert_eq!(names(&app), ["readme.txt"]);
    assert_eq!(
        app.panels[0].look.title.as_deref(),
        Some(r"Archive:zip:a.zip:\docs")
    );
    enter(&mut app, Side::Left, "..");
    tick_until(&mut app, |app| in_archive(app, "/"));
    enter(&mut app, Side::Left, "..");
    let back = VfsPath::local(files.path());
    tick_until(&mut app, |app| shows(app, Side::Left, &back));
    assert_eq!(
        app.panels[0].current().map(|entry| entry.name.as_str()),
        Some("a.zip")
    );
}

/// arclite's masks: Ctrl+PgDn opens a zip of any name by its contents; on a file that is no archive it
/// opens nothing, as in FAR (Enter on such a name is the program's own: the plugin's `opening` tests;
/// not pressed here, the system would open the file on the desktop).
#[test]
fn ctrl_pgdn_opens_a_zip_of_any_name() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivepgdn");
    let back = VfsPath::local(files.path());
    point(&mut app, "plain.txt");
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    assert!(app.plugins.key_wait.is_some(), "the plugin asked");
    tick_until(&mut app, |app| app.plugins.key_wait.is_none());
    assert!(shows(&app, Side::Left, &back), "no archive: nothing opens");
    assert!(app.overlay.is_none(), "nothing told, nothing run (FAR's)");
    point(&mut app, "b.docx");
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(
        app.panels[0].look.title.as_deref(),
        Some("Archive:zip:b.docx")
    );
}

/// The plugin's own dialog up (its title).
fn plugin_dialog(app: &App) -> Option<&str> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Plugin(PluginForm::Dialog),
        })) => Some(form.title.as_str()),
        _ => None,
    }
}

/// F5 from the archive's folder (FAR's `GetFilesW`): arclite's Extract dialog, not the program's; OK
/// writes the file into the other panel's folder, its data whole, and both panels are read again.
#[test]
fn a_file_in_an_archive_is_extracted_by_the_plugins_dialog() {
    let (_plugins, _files, out, mut app) = app_with_archives("archivecopy");
    point(&mut app, "a.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert!(app.panels[0].look.gives_files, "arclite's own GetFilesW");
    enter(&mut app, Side::Left, "docs");
    tick_until(&mut app, |app| in_archive(app, "/docs"));
    point(&mut app, "readme.txt");
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(&app), Some("Extract files"));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let copied = out.path().join("readme.txt");
    tick_until(&mut app, |app| {
        app.plugins.key_wait.is_none()
            && app.plugins.key_late.is_empty()
            && std::fs::read(&copied).is_ok_and(|data| data == b"hello")
    });
}

/// F3 on a file of the archive: written by the plugin into a folder of the program's (no dialog), the
/// viewer on that copy.
#[test]
fn a_file_in_an_archive_is_viewed_through_the_plugin() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archiveview");
    point(&mut app, "a.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    point(&mut app, "top.txt");
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
    tick_until(&mut app, |app| matches!(app.screen, Screen::Viewer(_)));
    assert!(plugin_dialog(&app).is_none(), "no dialog for the viewer");
    assert_eq!(app.extracted.len(), 1, "the copy kept while viewed");
    app.screen = Screen::Panels;
    assert!(!app.pump_extracted(), "nothing to put back");
    assert!(app.extracted.is_empty(), "its folder gone");
}

/// An archive inside an archive (FAR's `ProcessEnter` with `PluginMode`): Enter writes it out by the
/// plugin into a folder of the program's and opens it over the outer archive's panel; ".." at its root
/// comes back with the cursor on it, and its copy goes once its plugin let it go.
#[test]
fn an_archive_inside_an_archive_opens_over_it() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archivenested");
    point(&mut app, "c.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    point(&mut app, "inner.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.panels[0].look.title.as_deref() == Some("Archive:zip:inner.zip") && in_archive(app, "/")
    });
    let copy = (app.panels[0].plugin_stack_mut().last())
        .and_then(|entered| entered.host.as_ref()?.temp.clone())
        .unwrap();
    assert!(
        copy.join("inner.zip").exists(),
        "its copy in the program's folder"
    );
    point(&mut app, "top.txt");
    point(&mut app, "..");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.panels[0].look.title.as_deref() == Some("Archive:zip:c.zip") && in_archive(app, "/")
    });
    assert_eq!(
        app.panels[0].current().map(|entry| entry.name.as_str()),
        Some("inner.zip"),
        "the cursor on it"
    );
    tick_until(&mut app, |_| !copy.exists());
}

/// The program's folder the plugin is writing the file under the cursor into for Enter / Ctrl+PgDn.
fn open_folder(app: &App) -> Option<std::path::PathBuf> {
    match &app.plugins.key_wait.as_ref()?.kind {
        crate::app::plugins::keys::WaitKind::Get(Getting::Open { folder, .. }) => {
            Some(folder.clone())
        }
        _ => None,
    }
}

/// Ctrl+PgDn on an archive inside an archive opens it over the outer one too; its copy is among those
/// the program deletes as it ends while the panel is open. Ctrl+PgDn on a file no plugin takes: nothing
/// opens and its copy goes (FAR's `ProcessEnter`: no system's program for Ctrl+PgDn).
#[test]
fn ctrl_pgdn_inside_an_archive_opens_an_archive_and_lets_another_file_go() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archivenestedpgdn");
    point(&mut app, "c.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    let outer = app.panels[0].path.clone();
    point(&mut app, "note.txt");
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    let note = open_folder(&app).expect("written out by the plugin");
    tick_until(&mut app, |app| {
        app.plugins.key_wait.is_none() && app.plugins.key_late.is_empty()
    });
    assert!(shows(&app, Side::Left, &outer), "nothing opened over it");
    assert!(app.overlay.is_none(), "nothing told");
    tick_until(&mut app, |_| !note.exists());
    point(&mut app, "inner.zip");
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    let inner = open_folder(&app).expect("written out by the plugin");
    tick_until(&mut app, |app| {
        app.panels[0].look.title.as_deref() == Some("Archive:zip:inner.zip") && in_archive(app, "/")
    });
    let copies = app.plugin_copies();
    assert_eq!(copies, std::slice::from_ref(&inner), "kept for the end");
    assert!(inner.join("inner.zip").exists());
    // The program's end as `main` has it: the plugins end with it, then the copies go.
    drop(app);
    crate::app::remove_copies_now(&copies);
    assert!(!inner.exists(), "the plugin let it go, the copy gone");
}

/// A file's plugin's call failing while a copy waits for it: the copy goes with its wait.
#[test]
fn a_failed_call_lets_its_copy_go() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archivecopyfails");
    let folder = tempfile::tempdir().unwrap().keep();
    std::fs::write(folder.join("x.zip"), b"x").unwrap();
    let kind = crate::app::plugins::keys::WaitKind::File {
        path: folder.join("x.zip"),
        name: "x.zip".to_string(),
        alternative: false,
        next: Vec::new(),
        temp: Some(folder.clone()),
    };
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.wait_for_plugin(kind, "archivecopyfails", Side::Left, enter);
    app.plugin_key_failed("archivecopyfails", None);
    assert!(app.plugins.key_wait.is_none());
    tick_until(&mut app, |_| !folder.exists());
}

/// The program's end: the copies' folders deleted with what is in them, one already gone let be.
#[test]
fn copies_are_deleted_as_the_program_ends() {
    let temp = tempfile::tempdir().unwrap();
    let copy = temp.path().join("copy");
    std::fs::create_dir_all(copy.join("deep")).unwrap();
    std::fs::write(copy.join("deep").join("a.zip"), b"zip").unwrap();
    let gone = temp.path().join("gone");
    crate::app::remove_copies_now(&[gone, copy.clone()]);
    assert!(!copy.exists());
}

/// The names in the zip `path`.
fn zip_names(path: &std::path::Path) -> Vec<String> {
    let Some(opened) = archive::support::opened(path) else {
        return Vec::new();
    };
    (opened.items.iter())
        .map(|item| item.path.join("/"))
        .collect()
}

/// F5 from a disk into an archive's panel (FAR's `PutFiles`): the program's copy dialog with the
/// format's target, then arclite's "Add files to archive"; OK writes the archive anew with the file in
/// the panel's folder, and the panel read again shows it.
#[test]
fn a_file_is_added_to_an_archive_by_the_plugins_dialog() {
    let (_plugins, files, out, mut app) = app_with_archives("archiveadd");
    point(&mut app, "a.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert!(app.panels[0].look.takes_paths, "arclite's PutFilesW");
    enter(&mut app, Side::Left, "docs");
    tick_until(&mut app, |app| in_archive(app, "/docs"));
    std::fs::write(out.path().join("added.txt"), "added").unwrap();
    app.panels[1].reload();
    tick_until(&mut app, |app| {
        app.panels[1].reading().is_none()
            && (app.panels[1].entries.iter()).any(|entry| entry.name == "added.txt")
    });
    app.active = Side::Right;
    let panel = &mut app.panels[1];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "added.txt")
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    assert!(app.overlay.is_some(), "the program's copy dialog");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(&app), Some("Add files to archive"));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let archive = files.path().join("a.zip");
    tick_until(&mut app, |app| {
        app.plugins.putting.is_none() && zip_names(&archive).contains(&"docs/added.txt".to_string())
    });
    assert_eq!(
        zip_names(&archive),
        ["docs/readme.txt", "top.txt", "docs/added.txt"]
    );
    tick_until(&mut app, |app| {
        in_archive(app, "/docs") && names(app).contains(&"added.txt".to_string())
    });
    assert!(out.path().join("added.txt").exists(), "copied, not moved");
}

#[path = "archive_put_tests.rs"]
mod put;

#[path = "archive_delete_tests.rs"]
mod delete;

#[path = "archive_create_tests.rs"]
mod create;

#[path = "archive_host_file_tests.rs"]
mod host_file;

#[path = "archive_processed_tests.rs"]
mod processed;

#[path = "archive_formats_tests.rs"]
mod formats;

/// Shift+F5 (FAR's `ReturnCurrentFile`): the entry under the cursor alone through the plugin's dialog,
/// the selection kept; Esc in the dialog extracts nothing and keeps it too.
#[test]
fn shift_f5_extracts_the_current_entry_and_keeps_the_selection() {
    let (_plugins, _files, out, mut app) = app_with_archives("archiveshift");
    point(&mut app, "a.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    app.panels[0].selected.insert("docs".to_string());
    point(&mut app, "top.txt");
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.plugins.key_wait.is_none() && app.plugins.key_late.is_empty()
    });
    assert!(!out.path().join("docs").exists(), "Esc: nothing written");
    assert!(
        app.panels[0].selected.contains("docs"),
        "Esc: the selection kept"
    );
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::SHIFT));
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(&app), Some("Extract files"));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let top = out.path().join("top.txt");
    tick_until(&mut app, |app| {
        app.plugins.key_wait.is_none()
            && app.plugins.key_late.is_empty()
            && std::fs::read(&top).is_ok_and(|data| data == b"top")
    });
    assert!(
        !out.path().join("docs").exists(),
        "the selected folder not taken"
    );
    assert!(
        app.panels[0].selected.contains("docs"),
        "the selection kept"
    );
}

/// A disk image of two partitions (arclite's partition mode): its partitions named by their file
/// systems; Enter on one opens it over the disk's panel (`MBR→FAT`, the partition's index in the
/// title), ".." at its root back to the disk with the cursor on it.
#[test]
fn a_partition_is_entered_from_its_disk() {
    let (_plugins, files, _out, mut app) = app_with_archives("archiveparts");
    archive::support::mbr_disk(&files.path().join("disk.mbr")).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    point(&mut app, "disk.mbr");
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(names(&app), ["0.mbr.fat", "1.mbr.fat"]);
    let disk_title = app.panels[0].look.title.clone();
    assert_eq!(disk_title.as_deref(), Some("Archive:MBR:disk.mbr"));
    point(&mut app, "1.mbr.fat");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        in_archive(app, "/") && app.panels[0].look.title != disk_title
    });
    assert_eq!(names(&app), ["HELLO.TXT"]);
    assert_eq!(
        app.panels[0].look.title.as_deref(),
        Some("Archive:MBR→FAT:disk.mbr:1")
    );
    enter(&mut app, Side::Left, "..");
    tick_until(&mut app, |app| {
        in_archive(app, "/") && app.panels[0].look.title == disk_title
    });
    assert_eq!(
        app.panels[0].current().map(|entry| entry.name.as_str()),
        Some("1.mbr.fat")
    );
    point(&mut app, "..");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let back = VfsPath::local(files.path());
    tick_until(&mut app, |app| shows(app, Side::Left, &back));
}
