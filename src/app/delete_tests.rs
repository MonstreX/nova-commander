//! Delete from the panels: FAR's keys and question, the task and its questions. Panels on `tree://`
//! (in memory) for the Recycle Bin; temporary folders, deleted past it, for the real disk.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::super::dialog::{Dialog, DialogOutcome, FormAction};
use super::super::fileops::tests::{app_in, finish_transfer, submit_dialog, wait_for_question};
use super::super::form::{Control, Form};
use super::super::{App, Overlay};
use crate::vfs::VfsPath;
use crate::vfs::test_tree::{self as tree, exists, file, read_only};

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    app.handle_key(KeyEvent::new_with_kind(
        code,
        modifiers,
        KeyEventKind::Release,
    ));
}

/// An app whose left panel shows `tree://<session>/` with `names` (files), the cursor on `cursor`.
fn app_on_tree(session: &str, names: &[&str], cursor: &str) -> (App, VfsPath) {
    let root = tree::root(session);
    for name in names {
        file(&root.join(name));
    }
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let panel = app.active_panel_mut();
    panel.change_dir(&root, Some(cursor)).unwrap();
    (app, root)
}

fn form(app: &App) -> &Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("expected a form"),
    }
}

fn labels(form: &Form) -> Vec<String> {
    form.items
        .iter()
        .filter_map(|item| match &item.control {
            Control::Label(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn buttons(form: &Form) -> Vec<String> {
    form.items
        .iter()
        .filter_map(|item| match &item.control {
            Control::Button { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

/// The item index of the button `label`.
fn button(form: &Form, label: &str) -> usize {
    form.items
        .iter()
        .position(
            |item| matches!(&item.control, Control::Button { label: text, .. } if text == label),
        )
        .unwrap_or_else(|| panic!("no button {label}"))
}

fn press_button(app: &mut App, label: &str) {
    let index = button(form(app), label);
    submit_dialog(app, DialogOutcome::Button(index));
}

#[test]
fn f8_where_there_is_a_recycle_bin_asks_fars_question_and_moves_to_it() {
    let (mut app, root) = app_on_tree("app-f8", &["a.txt"], "a.txt");

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);

    let question = form(&app);
    assert_eq!(question.title, "Delete");
    assert_eq!(
        labels(question),
        [
            "Do you wish to move the file to the Recycle Bin?",
            "a.txt",
            "0 bytes"
        ]
    );
    assert_eq!(buttons(question), ["Move", "Cancel"]);
    assert!(
        !question.warning,
        "FAR's Recycle Bin question is not a warning"
    );
    press_button(&mut app, "Move");
    finish_transfer(&mut app);
    assert!(!exists(&root.join("a.txt")));
    assert_eq!(
        tree::log("app-f8")
            .iter()
            .filter(|line| line.starts_with("trash"))
            .count(),
        1
    );
}

#[test]
fn shift_del_deletes_past_the_bin_with_a_warning() {
    let (mut app, root) = app_on_tree("app-shift-del", &["a.txt"], "a.txt");

    press(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);

    let question = form(&app);
    assert_eq!(
        labels(question),
        ["Do you wish to delete the file?", "a.txt", "0 bytes"]
    );
    assert!(question.warning);
    press_button(&mut app, "Delete");
    finish_transfer(&mut app);
    assert!(!exists(&root.join("a.txt")));
    assert!(
        tree::log("app-shift-del").contains(&"remove-file tree://app-shift-del/a.txt".to_string())
    );
}

#[test]
fn cancel_in_the_question_deletes_nothing() {
    let (mut app, root) = app_on_tree("app-cancel", &["a.txt"], "a.txt");
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    press_button(&mut app, "Cancel");
    assert!(app.tasks.is_empty());
    assert!(exists(&root.join("a.txt")));
}

#[test]
fn shift_f8_takes_the_file_under_the_cursor_not_the_selection() {
    let (mut app, root) = app_on_tree("app-shift-f8", &["a.txt", "b.txt"], "b.txt");
    app.active_panel_mut().selected.insert("a.txt".to_string());

    press(&mut app, KeyCode::F(8), KeyModifiers::SHIFT);

    assert_eq!(labels(form(&app))[1], "b.txt");
    press_button(&mut app, "Move");
    finish_transfer(&mut app);
    assert!(exists(&root.join("a.txt")));
    assert!(!exists(&root.join("b.txt")));
}

#[test]
fn del_deletes_when_the_command_line_is_empty_and_edits_it_otherwise() {
    let (mut app, _root) = app_on_tree("app-del", &["a.txt"], "a.txt");
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(form(&app).title, "Delete");
    app.overlay = None;

    app.command_line.set("abc");
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert_eq!(app.command_line.text(), "bc");
}

#[test]
fn a_held_delete_key_asks_once() {
    let (mut app, _root) = app_on_tree("app-held", &["a.txt"], "a.txt");
    for (code, modifiers) in [
        (KeyCode::F(8), KeyModifiers::NONE),
        (KeyCode::F(8), KeyModifiers::SHIFT),
        (KeyCode::Delete, KeyModifiers::SHIFT),
        (KeyCode::Delete, KeyModifiers::NONE),
    ] {
        app.handle_key(KeyEvent::new_with_kind(
            code,
            modifiers,
            KeyEventKind::Repeat,
        ));
        assert!(app.overlay.is_none(), "{code:?} {modifiers:?}");
    }
    // Held Del with text in the command line still edits it.
    app.command_line.set("abc");
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new_with_kind(
        KeyCode::Delete,
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    ));
    assert_eq!(app.command_line.text(), "bc");
}

#[test]
fn several_objects_are_listed_as_far_does() {
    let names: Vec<String> = (1..=13).map(|n| format!("{n:02}.txt")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (mut app, _root) = app_on_tree("app-many", &refs, "01.txt");
    app.active_panel_mut()
        .selected
        .extend(names.iter().cloned());

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);

    let shown = labels(form(&app));
    assert_eq!(
        shown[0],
        "Do you wish to move the objects to the Recycle Bin?"
    );
    // The question, the count, ten names, the rest.
    assert_eq!(shown.len(), 1 + 1 + 10 + 1);
    assert_eq!(shown[12], "…and 3 more");
    tick_until(&mut app, |app| labels(form(app))[1] == "13 files, 0 bytes");

    // One more than shown: FAR lists it instead of "…and 1 more".
    app.overlay = None;
    app.active_panel_mut().selected.remove("13.txt");
    app.active_panel_mut().selected.remove("12.txt");
    app.ask_delete(false, false);
    let shown = labels(form(&app));
    assert_eq!(shown.len(), 1 + 1 + 11);
    assert_eq!(shown[12], "11.txt");
}

fn tick_until(app: &mut App, mut done: impl FnMut(&App) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done(app) {
        assert!(std::time::Instant::now() < deadline, "waited too long");
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A panel on `tree://<session>/` with `folder/` holding `a.txt` and `inner/deep.txt`, the cursor on
/// the folder.
fn app_on_folder(session: &str) -> (App, VfsPath) {
    let root = tree::root(session);
    let folder = tree::dir(&root.join("folder"));
    file(&folder.join("a.txt"));
    let inner = tree::dir(&folder.join("inner"));
    file(&inner.join("deep.txt"));
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active_panel_mut()
        .change_dir(&root, Some("folder"))
        .unwrap();
    (app, folder)
}

#[test]
fn the_question_about_a_folder_counts_what_it_holds_while_it_waits() {
    let (mut app, folder) = app_on_folder("app-count");
    app.config.delete_settings.confirm_folders = false;

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);

    assert_eq!(labels(form(&app))[1], "folder");
    // Counting goes on in the open question; the panels and the keys are not held up.
    tick_until(&mut app, |app| {
        labels(form(app))[2] == "2 files and 2 folders, 0 bytes"
    });
    let line = form(&app)
        .items
        .iter()
        .find(|item| item.control == Control::Label("2 files and 2 folders, 0 bytes".to_string()))
        .unwrap();
    let text_width = usize::from(form(&app).width) - 10;
    assert_eq!(
        usize::from(line.x),
        5 + (text_width - "2 files and 2 folders, 0 bytes".len()) / 2,
        "centred again"
    );

    press_button(&mut app, "Move");
    assert!(
        app.work.delete_count.is_none(),
        "Delete hands the count to the deletion"
    );
    finish_transfer(&mut app);
    assert!(!exists(&folder));
}

#[test]
fn closing_the_question_stops_its_count() {
    let (mut app, folder) = app_on_folder("app-count-cancel");
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    assert!(app.work.delete_count.is_some());

    press_button(&mut app, "Cancel");

    assert!(app.work.delete_count.is_none());
    assert!(exists(&folder));
    // A question gone some other way takes its count with it too.
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    app.overlay = None;
    app.tick();
    assert!(app.work.delete_count.is_none());
}

#[test]
fn sizes_read_as_explorer_shows_them() {
    let decimal = crate::platform::locale::number_format().decimal_separator;
    let size = |bytes| super::readable_size(bytes).replace(decimal, ".");
    assert_eq!(size(0), "0 bytes");
    assert_eq!(size(1024), "1.00 KB");
    assert_eq!(size(12_800), "12.5 KB");
    assert_eq!(size(1_023_000), "999 KB");
    assert_eq!(size(1_048_064), "1.00 MB", "past 999 KB into the next unit");
    assert_eq!(size(157_286_400), "150 MB");
    assert_eq!(size(u64::MAX), "16.0 EB");
}

#[test]
fn the_count_reads_as_people_say_it() {
    use crate::jobs::delete::Tally;
    let text = |files, folders, bytes, known| {
        super::tally_text((
            Tally {
                files,
                folders,
                bytes,
            },
            known,
        ))
    };
    let decimal = crate::platform::locale::number_format().decimal_separator;
    assert_eq!(text(0, 0, 0, false), "Counting…");
    assert_eq!(text(1, 0, 1, true), "1 file, 1 byte");
    assert_eq!(text(0, 1, 0, true), "1 folder", "no size without files");
    assert_eq!(
        text(3, 2, 1_342_177_280, false),
        format!("3 files and 2 folders, 1{decimal}25 GB…")
    );
    let separator = crate::platform::locale::number_format().thousands_separator;
    assert_eq!(
        text(1234, 0, 1023, true),
        format!("1{separator}234 files, 1023 bytes")
    );
    // The widest the question is made for fits without being cut.
    let form = super::confirmation(
        &crate::jobs::delete::Request {
            items: Vec::new(),
            mode: crate::jobs::delete::Mode::Remove,
            ask_folders: true,
            ask_read_only: true,
            count_totals: true,
        },
        10,
        true,
    )
    .0;
    assert!(usize::from(form.width) - 10 >= super::WIDEST_TALLY.chars().count());
}

#[test]
fn without_the_confirmation_the_deletion_starts_at_once() {
    let (mut app, root) = app_on_tree("app-no-confirm", &["a.txt"], "a.txt");
    app.config.delete_settings.confirm = false;

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);

    assert!(app.shown_task.is_some(), "the progress is in front");
    finish_transfer(&mut app);
    assert!(!exists(&root.join("a.txt")));
}

#[test]
fn a_question_of_the_deletion_comes_up_and_its_answer_goes_back() {
    let root = tree::root("app-ro");
    let locked = read_only(&root.join("locked.txt"));
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active_panel_mut()
        .change_dir(&root, Some("locked.txt"))
        .unwrap();

    press(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    press_button(&mut app, "Delete");
    wait_for_question(&mut app);

    let question = form(&app);
    assert_eq!(question.title, "Warning");
    assert_eq!(
        labels(question),
        [
            "The file is read only",
            "tree://app-ro/locked.txt",
            "Do you wish to delete it?"
        ]
    );
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::AnswerDelete { .. },
            ..
        }))
    ));
    press_button(&mut app, "&Delete");
    finish_transfer(&mut app);
    assert!(!exists(&locked));
}

#[test]
fn the_question_about_one_file_gives_its_size_and_date() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("sized.txt"), "x".repeat(1234)).unwrap();
    let mut app = app_in(temp.path());
    app.config.delete_settings.to_recycle_bin = false;
    app.active_panel_mut().end();

    app.ask_delete(false, false);

    let shown = labels(form(&app));
    let separator = crate::platform::locale::number_format().thousands_separator;
    let modified = app.active_panel().current().unwrap().modified.unwrap();
    let (date, time) = crate::platform::locale::date_time(modified, false, 5);
    assert_eq!(shown[2], format!("1{separator}234 bytes, {date} {time}"));
}

#[test]
fn delete_requires_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("doomed.txt"), "x").unwrap();
    let mut app = app_in(temp.path());
    app.config.delete_settings.to_recycle_bin = false;
    app.active_panel_mut().end();

    app.ask_delete(false, false);
    assert!(temp.path().join("doomed.txt").exists());
    press_button(&mut app, "Cancel");
    assert!(temp.path().join("doomed.txt").exists());

    app.ask_delete(false, false);
    press_button(&mut app, "Delete");
    finish_transfer(&mut app);
    assert!(!temp.path().join("doomed.txt").exists());
}

#[test]
fn deleting_a_folder_link_names_its_target_and_keeps_it() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("kept.txt"), "x").unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J", "link", "target"])
        .current_dir(temp.path())
        .output()
        .unwrap()
        .status;
    assert!(status.success());
    let mut app = app_in(temp.path());
    app.config.delete_settings.to_recycle_bin = false;
    app.active_panel_mut().cursor = 1;
    assert_eq!(app.active_panel().current().unwrap().name, "link");

    app.ask_delete(false, false);
    assert_eq!(
        labels(form(&app))[1..],
        [
            "link".to_string(),
            "a link to".to_string(),
            target.display().to_string()
        ]
    );
    press_button(&mut app, "Delete");
    finish_transfer(&mut app);

    assert!(std::fs::symlink_metadata(temp.path().join("link")).is_err());
    assert!(target.join("kept.txt").is_file());
}

#[test]
fn a_deleted_object_loses_its_mark_and_a_skipped_one_keeps_it() {
    let root = tree::root("app-marks");
    file(&root.join("1.txt"));
    read_only(&root.join("2.txt"));
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let panel = app.active_panel_mut();
    panel.change_dir(&root, Some("1.txt")).unwrap();
    panel
        .selected
        .extend(["1.txt".to_string(), "2.txt".to_string()]);

    press(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    press_button(&mut app, "Delete");
    wait_for_question(&mut app);
    assert!(
        !app.active_panel().selected.contains("1.txt"),
        "gone before the question about the next one"
    );
    press_button(&mut app, "&Skip");
    finish_transfer(&mut app);

    let marks: Vec<&String> = app.active_panel().selected.iter().collect();
    assert_eq!(marks, ["2.txt"]);
}

#[test]
fn the_error_question_shows_who_holds_the_file() {
    let (form, _) = super::question_dialog(&crate::jobs::delete::Question::Error {
        path: VfsPath::local(r"D:\x"),
        folder: false,
        message: "in use".to_string(),
        kind: std::io::ErrorKind::Other,
        held: vec![
            "Object is being opened in:".to_string(),
            "a.exe (PID 1)".to_string(),
        ],
    });
    let shown: Vec<String> = form
        .items
        .iter()
        .filter_map(|item| match &item.control {
            Control::Label(text) | Control::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    let position = |text: &str| shown.iter().position(|line: &String| line.contains(text));
    assert!(position("a.exe (PID 1)").is_some(), "{shown:?}");
    assert!(
        position("a.exe (PID 1)") < position("in use"),
        "the system's text last: {shown:?}"
    );
}

#[test]
fn the_recycle_bin_is_asked_for_every_item_not_the_first() {
    let item = |path: VfsPath| crate::jobs::delete::Item {
        path,
        entry: crate::vfs::Entry::default(),
    };
    let session = tree::root("trashall");
    let (a, b) = (file(&session.join("a")), file(&session.join("b")));
    assert!(super::all_can_trash(&[item(a.clone()), item(b)]));
    let nowhere = VfsPath::in_session("nowhereplugin", "x", "/c");
    assert!(
        !super::all_can_trash(&[item(a.clone()), item(nowhere.clone())]),
        "one with no bin: none goes there"
    );
    assert!(!super::all_can_trash(&[item(nowhere), item(a)]));
    assert!(!super::all_can_trash(&[]), "nothing: no bin");
}

#[test]
fn the_shell_is_asked_of_the_first_item_on_a_disk_only() {
    // FAR's `can_recycle` probe before the question (review R1): a file on a disk is the shell's to
    // answer, and the probe leaves it in place; nothing else is asked.
    let item = |path: VfsPath| crate::jobs::delete::Item {
        path,
        entry: crate::vfs::Entry::default(),
    };
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("first.txt");
    std::fs::write(&file, "stay").unwrap();
    let asked = super::first_recyclable(&[item(VfsPath::local(&file))]);
    assert!(asked.is_some(), "the shell answered");
    assert!(file.exists(), "the probe deletes nothing");
    let plugin = VfsPath::in_session("nowhereplugin", "x", "/c");
    assert_eq!(super::first_recyclable(&[item(plugin)]), None);
    assert_eq!(super::first_recyclable(&[]), None);
}

#[test]
fn a_file_bigger_than_its_disks_bin_is_not_recycled() {
    // Review R1, over FAR: the shell says it would recycle such a file and deletes it for good (seen on a
    // 64 MB test disk), so the question says "delete". A folder's size is not known before it.
    let item = |path: &std::path::Path, size: u64, is_dir: bool| crate::jobs::delete::Item {
        path: VfsPath::local(path),
        entry: crate::vfs::Entry {
            size,
            is_dir,
            ..crate::vfs::Entry::default()
        },
    };
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("first.txt");
    std::fs::write(&file, "stay").unwrap();
    let probe = super::first_recyclable(&[item(&file, 4, false)]) != Some(false);
    assert_eq!(
        super::recyclable(&[item(&file, 4, false)]),
        probe,
        "a small file: the probe's"
    );
    assert!(!super::recyclable(&[
        item(&file, 4, false),
        item(&file, u64::MAX, false)
    ]));
    assert_eq!(
        super::recyclable(&[item(&file, 4, false), item(temp.path(), u64::MAX, true)]),
        probe,
        "a folder's size is not asked"
    );
    assert!(file.exists(), "nothing deleted");
}
