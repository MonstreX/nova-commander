//! A run of Shift+F2 / Shift+F3 over a disk's files as the program steps it (S6.2e, FAR's
//! `PluginHostGetFiles` / `ProcessOneHostFile`), each file's end crafted as the plugin's thread sends
//! it for a wait set up as the run sets it: done unselects and makes the plugin's next file silent, not
//! opened goes on with the file kept, not done or a press waited for meanwhile ends the run with both
//! panels read again; a failed call ends the run the same way; the run's start (its destination, its
//! files without the folders) and a file no plugin takes.

use std::collections::BTreeSet;

use super::*;
use crate::app::plugins::keys::WaitKind;
use crate::app::plugins::own_ops::disk_files::DiskRun;
use crate::plugin_host::KeyAnswer;
use crate::plugin_host::disk_files::DiskEnd;

/// Shift+F2's run over `names` of the left panel's folder, waited for as the run waits: its serial.
fn run_wait(app: &mut App, plugin: &str, out: &tempfile::TempDir, names: &[&str]) -> u64 {
    let folder = (app.panels[0].path.as_local()).unwrap().to_path_buf();
    let run = DiskRun {
        destination: Some(out.path().display().to_string()),
        used: Vec::new(),
        folder,
        files: names.iter().map(|name| (*name).to_string()).collect(),
    };
    let kind = WaitKind::DiskFile {
        run,
        next: Vec::new(),
    };
    let key = KeyEvent::new(KeyCode::F(2), KeyModifiers::SHIFT);
    app.wait_for_plugin(kind, plugin, Side::Left, key)
}

/// The run waited for now: the press's serial and the run.
fn waiting_run(app: &App) -> Option<(u64, DiskRun)> {
    let wait = app.plugins.key_wait.as_ref()?;
    match &wait.kind {
        WaitKind::DiskFile { run, .. } => Some((wait.serial, run.clone())),
        _ => None,
    }
}

/// No wait of a run left, waited for or let go.
fn no_run_left(app: &App) -> bool {
    !(app.plugins.key_wait.iter().chain(&app.plugins.key_late))
        .any(|wait| matches!(wait.kind, WaitKind::DiskFile { .. }))
}

fn selected(app: &App) -> BTreeSet<String> {
    app.panels[0].selected.to_set()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// `late.txt` written into both panels' folders, seen only by a reading of each.
fn late_files(app: &App, files: &tempfile::TempDir, out: &tempfile::TempDir) {
    std::fs::write(files.path().join("late.txt"), "late").unwrap();
    std::fs::write(out.path().join("late.txt"), "late").unwrap();
    assert!(!read_again(app, Side::Left) && !read_again(app, Side::Right));
}

/// The panel `side` read again (or being read) since [`late_files`].
fn read_again(app: &App, side: Side) -> bool {
    let panel = &app.panels[side.index()];
    panel.is_reading() || (panel.entries.iter()).any(|entry| entry.name == "late.txt")
}

fn both_read_again(app: &App) -> bool {
    read_again(app, Side::Left) && read_again(app, Side::Right)
}

/// Done: the file unselected, the next one offered with the plugin among the used (its next file
/// silent, FAR's `UsedPlugins`), named there once however many it did; the last done ends the run.
#[test]
fn a_file_done_is_unselected_and_the_plugins_next_file_is_silent() {
    let name = "diskrundone";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    for file in ["a.zip", "c.zip", "b.docx"] {
        app.panels[0].selected.insert(file.to_string());
    }
    let serial = run_wait(&mut app, name, &out, &["a.zip", "c.zip"]);
    app.disk_file_ended(name, serial, (DiskEnd::Done, None));
    assert_eq!(
        selected(&app),
        set(&["b.docx", "c.zip"]),
        "a.zip unselected"
    );
    let (next, run) = waiting_run(&app).expect("the next file offered");
    assert_ne!(next, serial, "a press of its own");
    assert_eq!(run.files, ["c.zip"]);
    assert_eq!(run.used, [name], "its next file silent");
    late_files(&app, &files, &out);
    app.disk_file_ended(name, next, (DiskEnd::Done, None));
    assert_eq!(selected(&app), set(&["b.docx"]));
    assert!(no_run_left(&app), "none left: the run's end");
    assert!(both_read_again(&app), "both panels read again at its end");
}

/// The plugin named once in the used however many of its files are done.
#[test]
fn a_plugin_is_used_once_however_many_files_it_did() {
    let name = "diskrunusedonce";
    let (_plugins, _files, out, mut app) = app_with_archives(name);
    let serial = run_wait(&mut app, name, &out, &["a.zip", "c.zip", "b.docx"]);
    app.disk_file_ended(name, serial, (DiskEnd::Done, None));
    let (next, _) = waiting_run(&app).unwrap();
    app.disk_file_ended(name, next, (DiskEnd::Done, None));
    let (_, run) = waiting_run(&app).expect("the third file offered");
    assert_eq!(run.files, ["b.docx"]);
    assert_eq!(run.used, [name], "named once");
}

/// Not opened (no panel of the plugin's over it): the file stays selected, the next one is offered, the
/// plugin not among the used (its next file asks again).
#[test]
fn a_file_not_opened_stays_selected_and_the_run_goes_on() {
    let name = "diskrunnotopened";
    let (_plugins, _files, out, mut app) = app_with_archives(name);
    app.panels[0].selected.insert("a.zip".to_string());
    app.panels[0].selected.insert("c.zip".to_string());
    let serial = run_wait(&mut app, name, &out, &["a.zip", "c.zip"]);
    app.disk_file_ended(name, serial, (DiskEnd::NotOpened, None));
    assert_eq!(selected(&app), set(&["a.zip", "c.zip"]));
    let (_, run) = waiting_run(&app).expect("the next file offered");
    assert_eq!(run.files, ["c.zip"]);
    assert!(run.used.is_empty(), "not silent");
}

/// Not done (stopped, nothing done): no file after it, the selection kept, both panels read again.
#[test]
fn a_file_not_done_ends_the_run() {
    let name = "diskrunnotdone";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    app.panels[0].selected.insert("a.zip".to_string());
    app.panels[0].selected.insert("c.zip".to_string());
    let serial = run_wait(&mut app, name, &out, &["a.zip", "c.zip"]);
    late_files(&app, &files, &out);
    app.disk_file_ended(name, serial, (DiskEnd::NotDone, None));
    assert!(no_run_left(&app) && app.plugins.key_wait.is_none());
    assert_eq!(selected(&app), set(&["a.zip", "c.zip"]));
    assert!(both_read_again(&app));
    assert!(app.overlay.is_none(), "nothing to tell");
}

/// A failure: the plugin's reason said under its title, the run ended.
#[test]
fn a_files_failure_is_said_and_ends_the_run() {
    let name = "diskrunfailed";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    let serial = run_wait(&mut app, name, &out, &["a.zip", "c.zip"]);
    late_files(&app, &files, &out);
    app.disk_file_ended(
        name,
        serial,
        (DiskEnd::NotDone, Some("no room".to_string())),
    );
    let title = app.plugin_title(name);
    assert!(!title.is_empty());
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Message {
            title: shown,
            lines,
            error: true,
        })) => assert_eq!(
            (shown, lines.as_slice()),
            (&title, ["no room".to_string()].as_slice())
        ),
        _ => panic!("the failure said"),
    }
    assert!(no_run_left(&app));
    assert!(both_read_again(&app));
}

/// Another press waited for when the file at hand ends (its wait let go by a window of the plugin's,
/// the keys after it went on): the run ends rather than take that press's place, which stays waited for.
#[test]
fn a_press_waited_for_meanwhile_ends_the_run_when_its_file_ends() {
    let name = "diskrunpressed";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    let serial = run_wait(&mut app, name, &out, &["a.zip", "c.zip"]);
    let wait = app.plugins.key_wait.take().unwrap();
    app.plugins.key_late.push(wait);
    let other = app.wait_for_plugin(
        WaitKind::Key,
        name,
        Side::Left,
        KeyEvent::from(KeyCode::Char('x')),
    );
    late_files(&app, &files, &out);
    app.disk_file_ended(name, serial, (DiskEnd::Done, None));
    assert!(no_run_left(&app), "c.zip not offered");
    assert_eq!(
        (app.plugins.key_wait.as_ref()).map(|wait| wait.serial),
        Some(other),
        "the other press still waited for"
    );
    assert!(both_read_again(&app));
}

/// The same when the plugin passes the file on (the next plugin's turn): another press waited for ends
/// the run.
#[test]
fn a_press_waited_for_meanwhile_ends_the_run_when_its_file_is_passed_on() {
    let name = "diskrunpassed";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    let serial = run_wait(&mut app, name, &out, &["a.zip", "c.zip"]);
    let wait = app.plugins.key_wait.take().unwrap();
    app.plugins.key_late.push(wait);
    let other = app.wait_for_plugin(
        WaitKind::Key,
        name,
        Side::Left,
        KeyEvent::from(KeyCode::Char('x')),
    );
    late_files(&app, &files, &out);
    let pass = KeyAnswer {
        pass: true,
        ..KeyAnswer::default()
    };
    app.plugin_key_done(name, 0, serial, pass);
    assert!(no_run_left(&app), "c.zip not offered");
    assert_eq!(
        (app.plugins.key_wait.as_ref()).map(|wait| wait.serial),
        Some(other)
    );
    assert!(both_read_again(&app));
}

/// Passed on with nothing else waited for: the next file offered (the plugin's only one asked).
#[test]
fn a_file_passed_on_goes_to_the_next_file() {
    let name = "diskrunpassnext";
    let (_plugins, _files, out, mut app) = app_with_archives(name);
    let serial = run_wait(&mut app, name, &out, &["b.docx", "c.zip"]);
    let pass = KeyAnswer {
        pass: true,
        ..KeyAnswer::default()
    };
    app.plugin_key_done(name, 0, serial, pass);
    let (_, run) = waiting_run(&app).expect("the next file offered");
    assert_eq!(run.files, ["c.zip"]);
}

/// An end no longer waited for (Esc let the wait go): the panels read again, nothing else.
#[test]
fn an_end_not_waited_for_reads_the_panels_again() {
    let name = "diskrunlate";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    app.panels[0].selected.insert("a.zip".to_string());
    late_files(&app, &files, &out);
    app.disk_file_ended(name, 999, (DiskEnd::Done, None));
    assert!(both_read_again(&app));
    assert_eq!(selected(&app), set(&["a.zip"]), "not its to unselect");
}

/// The plugin's call failing (the job not run): a run it was in ends with both panels read again, the
/// wait waited for or let go; a failing call of another kind reads nothing again.
#[test]
fn a_failed_call_ends_a_run_with_the_panels_read_again() {
    let name = "diskrunkeyfailed";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    let serial = run_wait(&mut app, name, &out, &["a.zip"]);
    late_files(&app, &files, &out);
    app.plugin_key_failed(name, Some(serial));
    assert!(app.plugins.key_wait.is_none() && no_run_left(&app));
    assert!(both_read_again(&app), "waited for");
    tick_until(&mut app, |app| {
        app.panels.iter().all(|panel| !panel.is_reading())
    });
    std::fs::remove_file(files.path().join("late.txt")).unwrap();
    std::fs::remove_file(out.path().join("late.txt")).unwrap();
    app.panels[0].reload();
    app.panels[1].reload();
    tick_until(&mut app, |app| {
        app.panels.iter().all(|panel| !panel.is_reading())
    });
    // A wait of a run let go, the plugin's every wait failing.
    run_wait(&mut app, name, &out, &["a.zip"]);
    let wait = app.plugins.key_wait.take().unwrap();
    app.plugins.key_late.push(wait);
    late_files(&app, &files, &out);
    app.plugin_key_failed(name, None);
    assert!(no_run_left(&app));
    assert!(both_read_again(&app), "let go");
}

#[test]
fn a_failed_call_of_no_run_reads_nothing_again() {
    let name = "diskrunkeyother";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    let serial = app.wait_for_plugin(
        WaitKind::Key,
        name,
        Side::Left,
        KeyEvent::from(KeyCode::Char('x')),
    );
    late_files(&app, &files, &out);
    app.plugin_key_failed(name, Some(serial));
    assert!(app.plugins.key_wait.is_none());
    assert!(!read_again(&app, Side::Left) && !read_again(&app, Side::Right));
}

/// Shift+F2 with nothing selected and the other panel hidden: the destination is the file's name
/// without its last extension only (FAR's `strDestPath`); none to cut: the whole name.
#[test]
fn shift_f2s_destination_cuts_only_the_last_extension() {
    let name = "diskrunstem";
    let (_plugins, files, _out, mut app) = app_with_archives(name);
    std::fs::copy(files.path().join("a.zip"), files.path().join("arch.v1.zip")).unwrap();
    std::fs::copy(files.path().join("a.zip"), files.path().join("noext")).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    app.panels[1].visible = false;
    for (file, stem) in [("arch.v1.zip", "arch.v1"), ("noext", "noext")] {
        point(&mut app, file);
        app.start_disk_run(true);
        let (_, run) = waiting_run(&app).expect("offered");
        assert_eq!(run.destination.as_deref(), Some(stem), "{file}");
        assert_eq!(run.files, [file]);
        app.plugins.key_wait = None;
    }
    // Shift+F3: no destination.
    point(&mut app, "a.zip");
    app.start_disk_run(false);
    let (_, run) = waiting_run(&app).expect("offered");
    assert_eq!(run.destination, None);
    assert_eq!(run.folder, files.path());
}

/// The run's files are the selected ones but the folders, in the panel's order; the selection saved
/// first (Ctrl+M brings it back).
#[test]
fn a_run_takes_the_selected_files_but_the_folders_and_saves_the_selection() {
    let name = "diskrunfiles";
    let (_plugins, files, _out, mut app) = app_with_archives(name);
    std::fs::create_dir(files.path().join("sub.zip")).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    for file in ["c.zip", "sub.zip", "a.zip"] {
        app.panels[0].selected.insert(file.to_string());
    }
    app.start_disk_run(false);
    let (_, run) = waiting_run(&app).expect("offered");
    let mut taken = run.files.clone();
    taken.sort();
    assert_eq!(taken, ["a.zip", "c.zip"], "no folder");
    app.panels[0].selected.clear();
    app.panels[0].restore_selection();
    assert_eq!(selected(&app), set(&["a.zip", "c.zip", "sub.zip"]));
}

/// No plugin opens files: every file passed over, nothing waited for, the panels read again.
#[test]
fn with_no_plugin_for_the_files_nothing_is_asked() {
    let name = "diskrunnone";
    let (_plugins, files, out, mut app) = app_with_archives(name);
    let none = tempfile::tempdir().unwrap();
    app.set_plugins(load_plugins(none.path(), &|_| true));
    app.panels[0].selected.insert("a.zip".to_string());
    app.panels[0].selected.insert("c.zip".to_string());
    late_files(&app, &files, &out);
    app.start_disk_run(true);
    assert!(app.plugins.key_wait.is_none() && no_run_left(&app));
    assert!(both_read_again(&app));
    assert_eq!(selected(&app), set(&["a.zip", "c.zip"]));
}
