use super::super::dialog::DialogOutcome;
use super::super::transfer_dialog::{Operation, TransferAnswers};
use super::super::{App, Overlay};
use super::*;
use crate::jobs::transfer::{Existing, Keep, Skipped};
use crate::vfs::VfsPath;

fn report(failures: usize, not_kept: usize) -> Report {
    let lines = |text: &str, count| (0..count).map(|n| format!("{text} {n}")).collect();
    Report {
        failures: lines("failed", failures),
        not_kept: lines("not kept", not_kept),
        ..Report::default()
    }
}

#[test]
fn a_report_says_what_failed_then_what_was_not_kept() {
    assert_eq!(report_lines(&report(1, 1)), ["failed 0", "not kept 0"]);
    assert!(report_lines(&report(0, 0)).is_empty());
}

#[test]
fn a_long_report_is_cut_and_says_how_much_more_there_is() {
    let lines = report_lines(&report(15, 10));
    assert_eq!(lines.len(), REPORT_LINES + 1);
    assert_eq!(lines[REPORT_LINES], "…and 5 more");
    assert_eq!(report_lines(&report(REPORT_LINES, 0)).len(), REPORT_LINES);
}

#[test]
fn what_was_only_not_kept_is_a_message_not_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());

    app.finish_operation("Copy", &report(0, 1), None);

    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Message { error: false, .. }))
    ));
}

fn a_retry(root: &Path) -> Request {
    let source = VfsPath::local(root.join("a.txt"));
    let target = VfsPath::local(root.join("target"));
    Request {
        sources: vec![source.clone()],
        target: target.clone(),
        move_: false,
        existing: Existing::Ask,
        ask_read_only: true,
        keep: Keep::default(),
        link: None,
        retrying: vec![Skipped {
            source: source.clone(),
            destination: target.join("a.txt"),
            top: source,
            inside_link: false,
        }],
        filter: None,
    }
}

fn the_report(app: &App) -> &Dialog {
    match &app.overlay {
        Some(Overlay::Dialog(dialog)) => dialog,
        _ => panic!("no report"),
    }
}

#[test]
fn a_report_with_something_to_retry_offers_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());

    app.finish_operation("Copy", &report(1, 0), Some(a_retry(temp.path())));

    assert!(matches!(
        the_report(&app),
        Dialog::Confirm { buttons, danger: true, focused: 0, .. }
            if *buttons == ["OK", "Retry skipped"]
    ));
}

#[test]
fn a_report_with_nothing_to_retry_is_the_plain_error() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());

    app.finish_operation("Copy", &report(1, 0), None);

    assert!(matches!(
        the_report(&app),
        Dialog::Message { error: true, .. }
    ));
}

#[test]
fn retry_skipped_starts_a_task_in_front_and_ok_does_not() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "x").unwrap();
    std::fs::create_dir(temp.path().join("target")).unwrap();
    let mut app = app_in(temp.path());
    let action = |root: &Path| ConfirmAction::RetrySkipped(Box::new(a_retry(root)));

    app.finish_confirm(action(temp.path()), 0);
    assert_eq!(app.tasks.len(), 0, "OK starts nothing");

    app.finish_confirm(action(temp.path()), 1);
    assert_eq!(app.tasks.len(), 1);
    let id = app.tasks.iter().next().unwrap().id;
    assert_eq!(app.shown_task.as_ref().map(|shown| shown.id), Some(id));
    finish_transfer(&mut app);
    assert_eq!(
        std::fs::read_to_string(temp.path().join("target").join("a.txt")).unwrap(),
        "x"
    );
}

/// FAR's defaults, except that the drive menu does not ask an optical drive what it can do: the
/// tests send no SCSI commands to a real drive (Ctrl+8, `DriveColumns::cd`).
pub(in crate::app) fn app_in(dir: &Path) -> App {
    let mut app = App::new(dir, 100, 40, &crate::settings::Settings::default());
    app.config.drive_columns.cd = false;
    app
}

pub(in crate::app) fn select_all_files(app: &mut App) {
    let panel = app.active_panel_mut();
    let names: Vec<String> = panel
        .entries
        .iter()
        .filter(|entry| !entry.is_parent)
        .map(|entry| entry.name.clone())
        .collect();
    panel.selected.extend(names);
}

/// Runs the event loop's periodic work until the copy or move has ended.
pub(in crate::app) fn finish_transfer(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.tasks.active() > 0 || app.shown_task.is_some() || app.is_waiting() {
        assert!(
            std::time::Instant::now() < deadline,
            "the operation did not end"
        );
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Ticks until the running operation asks something.
pub(in crate::app) fn wait_for_question(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.overlay.is_none() {
        assert!(std::time::Instant::now() < deadline, "no question came");
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub(in crate::app) fn answers(target: &str) -> TransferAnswers {
    TransferAnswers {
        targets: vec![target.to_string()],
        existing: 0,
        ask_read_only: true,
        security: 0,
        preserve_timestamps: false,
        link_contents: false,
        verify: false,
        link: None,
        use_filter: false,
    }
}

pub(in crate::app) fn submit_dialog(app: &mut App, outcome: DialogOutcome) {
    let Some(Overlay::Dialog(dialog)) = app.overlay.take() else {
        panic!("expected a dialog");
    };
    app.finish_dialog(dialog, outcome);
    crate::app::waiting::finish_waiting(app);
}

#[test]
fn copy_flow_copies_selection_to_other_panel() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    let target = temp.path().join("dst");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(source.join("a.txt"), "a").unwrap();
    std::fs::write(source.join("b.txt"), "b").unwrap();
    let mut app = app_in(&source);
    app.panels[1]
        .change_dir(&crate::vfs::VfsPath::local(&target), None)
        .unwrap();
    select_all_files(&mut app);

    app.ask_copy();
    submit_dialog(
        &mut app,
        DialogOutcome::Button(super::super::transfer_dialog::COPY_BUTTON),
    );
    assert!(
        app.shown_task.is_some(),
        "the copy runs as a task, in front"
    );
    finish_transfer(&mut app);

    assert!(target.join("a.txt").exists());
    assert!(target.join("b.txt").exists());
    assert!(app.overlay.is_none());
    assert_eq!(app.panels[1].entries.len(), 3);
}

#[test]
fn move_flow_moves_files_and_folders_to_the_other_panel() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    let target = temp.path().join("dst");
    std::fs::create_dir_all(source.join("folder")).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(source.join("a.txt"), "a").unwrap();
    std::fs::write(source.join("folder").join("b.txt"), "b").unwrap();
    let mut app = app_in(&source);
    app.panels[1]
        .change_dir(&crate::vfs::VfsPath::local(&target), None)
        .unwrap();
    select_all_files(&mut app);

    app.ask_move();
    submit_dialog(
        &mut app,
        DialogOutcome::Button(super::super::transfer_dialog::COPY_BUTTON),
    );
    finish_transfer(&mut app);

    assert!(app.overlay.is_none(), "no error");
    assert!(target.join("a.txt").exists() && !source.join("a.txt").exists());
    assert!(target.join("folder").join("b.txt").exists());
    assert!(!source.join("folder").exists());
}

#[test]
fn an_existing_file_is_asked_about_and_esc_keeps_it() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), "new").unwrap();
    std::fs::write(temp.path().join("a.txt"), "old").unwrap();
    let mut app = app_in(&source);
    select_all_files(&mut app);

    let sources = app.active_panel().targets();
    app.start_transfer(&sources, &answers(".."), Operation::Copy);
    wait_for_question(&mut app);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("expected the question");
    };
    assert!(form.warning);
    submit_dialog(&mut app, DialogOutcome::Cancel);
    finish_transfer(&mut app);

    assert_eq!(
        std::fs::read_to_string(temp.path().join("a.txt")).unwrap(),
        "old"
    );
    assert!(
        app.overlay.is_none(),
        "a cancelled operation reports nothing"
    );
}

#[test]
fn esc_during_an_operation_asks_and_yes_stops_it() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), "new").unwrap();
    std::fs::write(temp.path().join("a.txt"), "old").unwrap();
    let mut app = app_in(&source);
    select_all_files(&mut app);
    let sources = app.active_panel().targets();
    app.start_transfer(&sources, &answers(".."), Operation::Copy);

    app.handle_transfer_key(ratatui::crossterm::event::KeyEvent::from(
        ratatui::crossterm::event::KeyCode::Esc,
    ));
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::StopTransfer(_),
            ..
        }))
    ));
    submit_dialog(&mut app, DialogOutcome::Button(0));
    finish_transfer(&mut app);

    assert_eq!(
        std::fs::read_to_string(temp.path().join("a.txt")).unwrap(),
        "old"
    );
}

#[test]
fn make_dir_puts_cursor_on_new_directory() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("z.txt"), "x").unwrap();
    let mut app = app_in(temp.path());

    app.make_dir("fresh");
    crate::app::waiting::finish_waiting(&mut app);

    assert!(temp.path().join("fresh").is_dir());
    let current = app.active_panel().current().map(|entry| entry.name.clone());
    assert_eq!(current.as_deref(), Some("fresh"));
}

#[test]
fn several_items_need_an_existing_target_directory() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::write(temp.path().join("b.txt"), "b").unwrap();
    let mut app = app_in(temp.path());
    select_all_files(&mut app);

    let sources = app.active_panel().targets();
    app.start_transfer(&sources, &answers("backup"), Operation::Move);

    assert!(!temp.path().join("backup").exists());
    assert!(temp.path().join("a.txt").exists());
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Message { error: true, .. }))
    ));
}

#[test]
fn operations_on_parent_entry_do_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.ask_delete(false, false);
    app.ask_copy();
    assert!(app.overlay.is_none());
}

/// R7 review D3: one item moved or copied to a bare name is renamed beside itself, and the cursor goes
/// onto the new name (FAR's `GoToFile(strRenamedName)`); into a folder there it stays.
#[test]
fn the_cursor_goes_onto_the_new_name() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::create_dir(temp.path().join("box")).unwrap();
    let mut app = app_in(temp.path());
    let current = |app: &App| app.active_panel().current().map(|entry| entry.name.clone());
    app.active_panel_mut().go_to_name("a.txt");
    for (target, operation) in [("z.txt", Operation::Move), ("b.txt", Operation::Copy)] {
        let sources = app.active_panel().targets();
        app.start_transfer(&sources, &answers(target), operation);
        finish_transfer(&mut app);
        assert_eq!(current(&app).as_deref(), Some(target), "{operation:?}");
    }
    let sources = app.active_panel().targets();
    app.start_transfer(&sources, &answers("box"), Operation::Copy);
    finish_transfer(&mut app);
    assert!(temp.path().join("box").join("b.txt").exists());
    assert_eq!(current(&app).as_deref(), Some("b.txt"), "into a folder");
}
