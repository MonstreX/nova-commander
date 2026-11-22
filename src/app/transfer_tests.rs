//! Background tasks in the app (ADR-0010): cancelling a task that runs or waits, the interrupt question
//! released whatever comes next, the queue moving on, several destinations shown in turn and stopped
//! together, the screen redrawn when the last task ends.

use std::path::Path;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::App;
use super::dialog::{ConfirmAction, Dialog, DialogOutcome};
use super::fileops::tests::{answers, app_in, finish_transfer, select_all_files, submit_dialog};
use super::transfer::{BACKGROUND, ShownTask};
use super::transfer_dialog::Operation;
use super::{Overlay, TaskStatus as Status};
use crate::jobs::tasks::TaskId;

fn tick_until(app: &mut App, mut done: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "waited too long");
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn status(app: &App, id: TaskId) -> Option<Status> {
    app.tasks.get(id).map(super::Task::status)
}

fn ids(app: &App) -> Vec<TaskId> {
    app.tasks.iter().map(|task| task.id).collect()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_transfer_key(KeyEvent::from(code));
}

/// `root/src/a.txt` ("new") selected in the active panel; `root/a.txt` ("old") is in the way.
fn setup() -> (tempfile::TempDir, App, Vec<crate::vfs::VfsPath>) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), "new").unwrap();
    std::fs::write(temp.path().join("a.txt"), "old").unwrap();
    let mut app = app_in(&source);
    select_all_files(&mut app);
    let sources = app.active_panel().targets();
    (temp, app, sources)
}

fn stop_with_yes(app: &mut App) {
    press(app, KeyCode::Esc);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::StopTransfer(_),
            ..
        }))
    ));
    submit_dialog(app, DialogOutcome::Button(0));
}

#[test]
fn a_running_task_is_cancelled_and_its_copy_is_gone() {
    let (temp, mut app, sources) = setup();
    let target = temp.path().join("empty");
    std::fs::create_dir(&target).unwrap();
    app.start_transfer(
        &sources,
        &answers(&target.display().to_string()),
        Operation::Copy,
    );
    let id = ids(&app)[0];
    // Paused before it starts: the job runs and waits at its first step.
    app.tasks.set_paused(id, true);
    tick_until(&mut app, |app| status(app, id) == Some(Status::Paused));
    stop_with_yes(&mut app);
    finish_transfer(&mut app);
    assert!(!target.join("a.txt").exists());
    assert!(app.shown_task.is_none());
}

#[test]
fn with_its_question_off_esc_cancels_a_task_at_once() {
    let (temp, mut app, sources) = setup();
    app.prefs.confirm_interrupt = false;
    let target = temp.path().join("empty");
    std::fs::create_dir(&target).unwrap();
    app.start_transfer(
        &sources,
        &answers(&target.display().to_string()),
        Operation::Copy,
    );
    let id = ids(&app)[0];
    app.tasks.set_paused(id, true);
    tick_until(&mut app, |app| status(app, id) == Some(Status::Paused));
    press(&mut app, KeyCode::Esc);
    assert!(app.overlay.is_none(), "no question");
    finish_transfer(&mut app);
    assert!(!target.join("a.txt").exists());
}

#[test]
fn with_the_exit_question_off_running_tasks_still_ask() {
    let (temp, mut app, sources) = setup();
    app.prefs.confirm_exit = false;
    let target = temp.path().join("empty");
    std::fs::create_dir(&target).unwrap();
    app.start_transfer(
        &sources,
        &answers(&target.display().to_string()),
        Operation::Copy,
    );
    let id = ids(&app)[0];
    app.tasks.set_paused(id, true);
    tick_until(&mut app, |app| status(app, id) == Some(Status::Paused));
    app.shown_task = None;
    app.execute(crate::app::Command::Quit);
    assert!(app.overlay.is_some(), "tasks are never dropped silently");
    assert!(!app.should_quit());
    app.overlay = None;
    app.tasks.cancel_all();
    finish_transfer(&mut app);
}

#[test]
fn a_task_waiting_for_an_answer_is_cancelled_from_the_front() {
    let (temp, mut app, sources) = setup();
    app.start_transfer(&sources, &answers(".."), Operation::Copy);
    press(&mut app, KeyCode::F(2));
    let id = ids(&app)[0];
    tick_until(&mut app, |app| status(app, id) == Some(Status::Waiting));
    app.shown_task = Some(ShownTask {
        id,
        focus: BACKGROUND,
        rest: Vec::new(),
    });
    // Esc before the next tick shows the question: the worker waits on it while it is cancelled.
    stop_with_yes(&mut app);
    tick_until(&mut app, |app| app.shown_task.is_none());
    assert_eq!(read(&temp.path().join("a.txt")), "old");
}

#[test]
fn no_then_background_in_one_burst_releases_the_hold() {
    let (temp, mut app, sources) = setup();
    let target = temp.path().join("empty");
    std::fs::create_dir(&target).unwrap();
    app.start_transfer(
        &sources,
        &answers(&target.display().to_string()),
        Operation::Copy,
    );
    let id = ids(&app)[0];
    app.tasks.set_paused(id, true);
    tick_until(&mut app, |app| status(app, id) == Some(Status::Paused));
    press(&mut app, KeyCode::Esc);
    app.tick();
    app.tasks.set_paused(id, false);
    // "No" and F2 come together, with no tick in between.
    submit_dialog(&mut app, DialogOutcome::Button(1));
    press(&mut app, KeyCode::F(2));
    tick_until(&mut app, |app| status(app, id) == Some(Status::Done));
    assert_eq!(read(&target.join("a.txt")), "new");
}

#[test]
fn a_queued_task_starts_when_the_one_before_it_is_cancelled() {
    let (temp, mut app, sources) = setup();
    app.start_transfer(&sources, &answers(".."), Operation::Copy);
    press(&mut app, KeyCode::F(2));
    let first = ids(&app)[0];
    tick_until(&mut app, |app| status(app, first) == Some(Status::Waiting));
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    app.start_transfer(
        &sources,
        &answers(&other.display().to_string()),
        Operation::Copy,
    );
    let second = ids(&app)[1];
    app.tick();
    assert_eq!(status(&app, second), Some(Status::Queued), "same volume");
    app.stop_transfer(first);
    tick_until(&mut app, |app| app.shown_task.is_none());
    assert_eq!(read(&other.join("a.txt")), "new");
    assert_eq!(status(&app, first), Some(Status::Cancelled));
}

#[test]
fn several_destinations_come_to_the_front_in_turn_and_cancel_stops_them_all() {
    let (temp, mut app, sources) = setup();
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let mut both = answers("..");
    both.targets.push(other.display().to_string());

    app.start_transfer(&sources, &both, Operation::Copy);
    let [first, second] = ids(&app)[..] else {
        panic!("a task per destination");
    };
    tick_until(&mut app, |app| app.overlay.is_some());
    submit_dialog(&mut app, DialogOutcome::Button(8));
    tick_until(&mut app, |app| {
        app.shown_task
            .as_ref()
            .is_some_and(|shown| shown.id == second)
    });
    finish_transfer(&mut app);
    assert_eq!(read(&temp.path().join("a.txt")), "new");
    assert_eq!(read(&other.join("a.txt")), "new");
    assert!(status(&app, first).is_none() && status(&app, second).is_none());

    // Cancel in the first destination's question stops the second too.
    std::fs::write(temp.path().join("a.txt"), "old").unwrap();
    std::fs::remove_file(other.join("a.txt")).unwrap();
    app.start_transfer(&sources, &both, Operation::Copy);
    tick_until(&mut app, |app| app.overlay.is_some());
    submit_dialog(&mut app, DialogOutcome::Cancel);
    finish_transfer(&mut app);
    assert!(!other.join("a.txt").exists(), "the second never ran");
    assert!(app.tasks.is_empty());
}

#[test]
fn the_tick_that_ends_the_last_task_asks_for_a_redraw() {
    let (temp, mut app, sources) = setup();
    let target = temp.path().join("empty");
    std::fs::create_dir(&target).unwrap();
    app.start_transfer(
        &sources,
        &answers(&target.display().to_string()),
        Operation::Copy,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "waited too long");
        let changed = app.tick();
        if app.tasks.is_empty() && app.shown_task.is_none() {
            assert!(changed, "the dialog must be drawn away");
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.tick(), "nothing to draw once nothing runs");
}

#[test]
fn a_queued_task_does_not_take_the_percent_away_from_the_one_that_runs() {
    let (temp, mut app, sources) = setup();
    let (one, two) = (temp.path().join("one"), temp.path().join("two"));
    std::fs::create_dir(&one).unwrap();
    std::fs::create_dir(&two).unwrap();
    app.start_transfer(
        &sources,
        &answers(&one.display().to_string()),
        Operation::Copy,
    );
    let first = ids(&app)[0];
    app.tasks.set_paused(first, true);
    tick_until(&mut app, |app| status(app, first) == Some(Status::Paused));
    app.start_transfer(
        &sources,
        &answers(&two.display().to_string()),
        Operation::Copy,
    );
    let second = ids(&app)[1];
    tick_until(&mut app, |app| {
        app.tasks.get(first).unwrap().progress().totals_known
    });

    assert_eq!(status(&app, second), Some(Status::Queued));
    assert!(app.task_title().unwrap().percent.is_some());
    let label = super::task_label(app.tasks.get(first).unwrap());
    assert!(
        label.starts_with("Copy paused ") && label.ends_with('%'),
        "{label}"
    );

    app.tasks.cancel(first);
    app.tasks.cancel(second);
    tick_until(&mut app, |app| app.tasks.active() == 0);
}

#[test]
fn the_title_shows_the_tasks_and_their_end_is_signalled_once() {
    let (temp, mut app, sources) = setup();
    let target = temp.path().join("empty");
    std::fs::create_dir(&target).unwrap();
    assert_eq!(app.task_title(), None);
    app.start_transfer(
        &sources,
        &answers(&target.display().to_string()),
        Operation::Copy,
    );
    let id = ids(&app)[0];
    app.tasks.set_paused(id, true);
    tick_until(&mut app, |app| status(app, id) == Some(Status::Paused));
    let title = app.task_title().unwrap();
    assert_eq!(title.text, "Copying");
    assert!(title.stalled, "a paused task moves nothing");
    assert!(!app.take_task_ended());
    app.tasks.set_paused(id, false);
    finish_transfer(&mut app);
    assert_eq!(app.task_title(), None);
    assert!(app.take_task_ended());
    assert!(!app.take_task_ended(), "once");
}

#[test]
fn a_target_typed_with_an_end_space_is_that_name_not_its_sibling() {
    use crate::platform::files::exact_path;
    let temp = tempfile::tempdir().unwrap();
    let spaced = exact_path(&temp.path().join("a "));
    std::fs::write(&spaced, "spaced").unwrap();
    std::fs::write(temp.path().join("a"), "plain").unwrap();
    let mut app = app_in(temp.path());
    let sources = [crate::vfs::VfsPath::local(temp.path().join("a "))];

    // Onto itself is refused with no question; taken for `a` it would ask about overwriting that.
    app.start_transfer(&sources, &answers("a "), Operation::Copy);
    finish_transfer(&mut app);

    let Some(Overlay::Dialog(Dialog::Message { lines, error, .. })) = &app.overlay else {
        panic!("the report of the refusal");
    };
    assert!(error);
    assert!(lines[0].contains("onto itself"), "{lines:?}");
    assert_eq!(read(&temp.path().join("a")), "plain");
    assert_eq!(read(&spaced), "spaced");
}

#[test]
fn ctrl_bracket_in_the_copy_dialog_replaces_the_untouched_target_with_a_panel_folder() {
    let (temp, mut app, _) = setup();
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    app.panels[1]
        .change_dir(&crate::vfs::VfsPath::local(&other), None)
        .unwrap();
    app.ask_copy();
    app.handle_key(KeyEvent::new(
        KeyCode::Char('['),
        ratatui::crossterm::event::KeyModifiers::CONTROL,
    ));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the copy dialog");
    };
    let source = temp.path().join("src").display().to_string();
    assert_eq!(
        form.edit_text(super::transfer_dialog::TARGET),
        format!("{source}\\")
    );
}

#[test]
fn shift_f5_copies_the_entry_under_the_cursor_alone_under_its_own_name() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = (temp.path().join("src"), temp.path().join("dst"));
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    for name in ["a.txt", "b.txt", "c;d.txt"] {
        std::fs::write(source.join(name), name).unwrap();
    }
    let mut app = app_in(&source);
    (app.panels[1].change_dir(&crate::vfs::VfsPath::local(&target), None)).unwrap();
    select_all_files(&mut app);
    let panel = app.active_panel_mut();
    panel.cursor = (panel.entries.iter())
        .position(|e| e.name == "c;d.txt")
        .unwrap();
    app.handle_key(KeyEvent::new(
        KeyCode::F(5),
        ratatui::crossterm::event::KeyModifiers::SHIFT,
    ));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the copy dialog");
    };
    let line = (form.items.iter())
        .find_map(|item| match &item.control {
            super::form::Control::Edit(input) => Some(input.text().to_string()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        line, "\"c;d.txt\"",
        "its own name, quoted for a copy (FAR's)"
    );
    let texts: Vec<String> = (form.items.iter())
        .filter_map(|item| match &item.control {
            super::form::Control::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts[0].contains("c;d.txt"),
        "one item, not the selection: {texts:?}"
    );
    app.overlay = None;
    app.handle_key(KeyEvent::new(
        KeyCode::F(6),
        ratatui::crossterm::event::KeyModifiers::SHIFT,
    ));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the move dialog");
    };
    let line = (form.items.iter())
        .find_map(|item| match &item.control {
            super::form::Control::Edit(input) => Some(input.text().to_string()),
            _ => None,
        })
        .unwrap();
    assert_eq!(line, "c;d.txt", "not quoted for a move");
}

#[test]
fn with_the_info_panel_passive_f5_offers_the_active_panels_folder() {
    let temp = tempfile::tempdir().unwrap();
    let (source, other) = (temp.path().join("src"), temp.path().join("other"));
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    for name in ["a.txt", "b.txt"] {
        std::fs::write(source.join(name), name).unwrap();
    }
    let mut app = app_in(&source);
    (app.panels[1].change_dir(&crate::vfs::VfsPath::local(&other), None)).unwrap();
    app.show_info_panel_on(super::Side::Right);
    let panel = app.active_panel_mut();
    panel.cursor = (panel.entries.iter())
        .position(|e| e.name == "a.txt")
        .unwrap();
    app.ask_copy();
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the copy dialog");
    };
    let line = (form.items.iter())
        .find_map(|item| match &item.control {
            super::form::Control::Edit(input) => Some(input.text().to_string()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        line, "a.txt",
        "FAR's info panel: the folder beside it, so the name itself"
    );
}

/// The form in front.
fn shown_form(app: &App) -> &crate::app::form::Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("a form: {:?}", app.overlay.is_some()),
    }
}

/// The task's question in front.
fn until_question(app: &mut App, title: &str) {
    tick_until(
        app,
        |app| matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Form { form, .. })) if form.title == title),
    );
}

#[test]
fn rename_asks_the_new_name_cancel_brings_the_question_back_and_a_taken_name_is_asked_about() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), "new").unwrap();
    std::fs::write(temp.path().join("a.txt"), "old").unwrap();
    std::fs::write(temp.path().join("b.txt"), "taken").unwrap();
    let mut app = app_in(&source);
    select_all_files(&mut app);
    let sources = app.active_panel().targets();
    app.start_transfer(&sources, &answers(".."), Operation::Copy);
    until_question(&mut app, "Warning");
    let rename = (shown_form(&app).items.iter())
        .position(|item| matches!(&item.control, crate::app::form::Control::Button { label, .. } if label == "R&ename"))
        .unwrap();
    submit_dialog(&mut app, DialogOutcome::Button(rename));
    assert_eq!(shown_form(&app).title, "Rename");
    let offered = shown_form(&app).edit_text(1).to_string();
    assert_eq!(
        Path::new(&offered),
        temp.path().join("a (2).txt"),
        "FAR's GenerateName, whole"
    );
    submit_dialog(&mut app, DialogOutcome::Cancel);
    until_question(&mut app, "Warning");
    // Rename again, to a name that is taken: the question comes about that one; Skip leaves it.
    submit_dialog(&mut app, DialogOutcome::Button(rename));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("Rename");
    };
    form.set_edit_text(1, "b.txt");
    submit_dialog(&mut app, DialogOutcome::Button(5));
    until_question(&mut app, "Warning");
    let shown: Vec<String> = (shown_form(&app).items.iter())
        .filter_map(|item| match &item.control {
            crate::app::form::Control::Text(text) | crate::app::form::Control::Label(text) => {
                Some(text.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        shown.iter().any(|line| line.ends_with("b.txt")),
        "{shown:?}"
    );
    submit_dialog(&mut app, DialogOutcome::Button(rename));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("Rename");
    };
    form.set_edit_text(1, "c.txt");
    submit_dialog(&mut app, DialogOutcome::Button(5));
    finish_transfer(&mut app);
    assert_eq!(
        std::fs::read_to_string(temp.path().join("c.txt")).unwrap(),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("a.txt")).unwrap(),
        "old"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("b.txt")).unwrap(),
        "taken"
    );
}
