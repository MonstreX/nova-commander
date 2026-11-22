//! Marks come off object by object while an operation runs (FAR's `ClearLastGetSelection`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::App;
use super::fileops::tests::{answers, app_in, finish_transfer, select_all_files};
use super::transfer_dialog::Operation;
use crate::jobs::tasks::TaskId;
use crate::jobs::transfer::{Answer, Reply};
use crate::vfs::VfsPath;

/// `root/src` with `a.txt`, `b.txt`, `c.txt`, all marked in the active panel; `root/out` holds `b.txt`.
fn setup() -> (tempfile::TempDir, App, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    let out = temp.path().join("out");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(source.join(name), "new").unwrap();
    }
    std::fs::write(out.join("b.txt"), "old").unwrap();
    let mut app = app_in(&source);
    select_all_files(&mut app);
    (temp, app, out)
}

fn marks(app: &App) -> Vec<String> {
    let mut names: Vec<String> = app.active_panel().selected.iter().cloned().collect();
    names.sort();
    names
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn start_copy(app: &mut App, targets: &[&Path]) -> TaskId {
    let sources = app.active_panel().targets();
    let mut request = answers("");
    request.targets = targets.iter().map(|target| text(target)).collect();
    app.start_transfer(&sources, &request, Operation::Copy);
    app.shown_task.as_ref().unwrap().id
}

/// Runs until the task `id` asks; the dialog it brings up is put away so the test answers directly.
fn until_asked(app: &mut App, id: TaskId) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.tasks.question(id).is_none() {
        assert!(Instant::now() < deadline, "no question came");
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    // The question is the worker's only stop: everything before it has been reported.
    app.tick();
    app.overlay = None;
}

#[test]
fn a_mark_comes_off_as_soon_as_its_object_is_copied_and_a_skipped_one_keeps_it() {
    let (_temp, mut app, out) = setup();

    let id = start_copy(&mut app, &[&out]);
    until_asked(&mut app, id);
    assert_eq!(
        marks(&app),
        ["b.txt", "c.txt"],
        "a.txt is done, b.txt asked"
    );

    app.tasks.answer(id, Reply::once(Answer::Skip));
    finish_transfer(&mut app);
    assert_eq!(marks(&app), ["b.txt"]);
}

#[test]
fn cancelling_keeps_the_marks_of_what_was_not_done() {
    let (_temp, mut app, out) = setup();

    let id = start_copy(&mut app, &[&out]);
    until_asked(&mut app, id);
    app.tasks.answer(id, Reply::once(Answer::Cancel));
    finish_transfer(&mut app);

    assert_eq!(marks(&app), ["b.txt", "c.txt"]);
}

#[test]
fn what_failed_stays_marked() {
    let (temp, mut app, _out) = setup();

    start_copy(&mut app, &[&temp.path().join("src")]);
    finish_transfer(&mut app);

    assert_eq!(marks(&app), ["a.txt", "b.txt", "c.txt"], "onto itself");
}

#[test]
fn of_several_destinations_only_the_last_takes_marks_off() {
    let (temp, mut app, out) = setup();
    let second = temp.path().join("second");
    std::fs::create_dir(&second).unwrap();

    let id = start_copy(&mut app, &[&out, &second]);
    until_asked(&mut app, id);
    assert_eq!(marks(&app), ["a.txt", "b.txt", "c.txt"]);

    app.tasks.answer(id, Reply::once(Answer::Overwrite));
    finish_transfer(&mut app);
    assert!(marks(&app).is_empty());
}

#[test]
fn the_marks_follow_their_panel_when_the_panels_change_places() {
    let (_temp, mut app, out) = setup();

    let id = start_copy(&mut app, &[&out]);
    until_asked(&mut app, id);
    app.swap_panels();
    app.tasks.answer(id, Reply::once(Answer::Overwrite));
    finish_transfer(&mut app);

    assert!(marks(&app).is_empty(), "{:?}", marks(&app));
}

#[test]
fn the_other_panels_marks_stay() {
    let (_temp, mut app, out) = setup();
    let folder = app.active_panel().path.clone();
    let other = 1 - app.active.index();
    app.panels[other].change_dir(&folder, None).unwrap();
    app.panels[other].selected.insert("a.txt".to_string());
    std::fs::remove_file(out.join("b.txt")).unwrap();

    start_copy(&mut app, &[&out]);
    finish_transfer(&mut app);

    assert!(marks(&app).is_empty());
    assert!(app.panels[other].selected.contains("a.txt"));
}

#[test]
fn a_panel_that_left_the_folder_is_not_touched() {
    let (temp, mut app, out) = setup();
    let source = VfsPath::local(temp.path().join("src")).join("a.txt");
    let panel = app.active_panel_mut();
    panel.change_dir(&VfsPath::local(&out), None).unwrap();
    panel.selected.insert("a.txt".to_string());

    panel.unmark(&source);

    assert!(panel.selected.contains("a.txt"));
}
