//! "Process subfolders" in FAR's "Attributes" dialog: shown when a folder is among the objects, its turn
//! of the boxes to `[?]` and of a folder's times and owner to blank (what was changed by hand stays), and
//! "Set" through everything inside (only in folders the tests make).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::layout::{self, FIRST_FLAG, OWNER, SET, SUBFOLDERS};
use super::*;
use crate::app::dialog::DialogOutcome;
use crate::app::fileops::tests::{app_in, finish_transfer, submit_dialog};
use crate::app::form::Control;
use crate::app::{Overlay, Side};
use crate::jobs::attributes::{Request, TimeEdit};

const HIDDEN: usize = FIRST_FLAG + 2;

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn dialog(app: &App) -> (&Form, &Session) {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Attributes(session),
        })) => (form, session),
        _ => panic!("no Attributes dialog"),
    }
}

fn focus(app: &mut App, index: usize) {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form.focus = index,
        _ => panic!("no form"),
    }
}

/// An app on a new folder holding `folder\inner\deep.txt` and `folder\top.txt`, the cursor on `folder`.
fn app_on_folder() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("folder");
    std::fs::create_dir_all(folder.join("inner")).unwrap();
    std::fs::write(folder.join("inner").join("deep.txt"), "d").unwrap();
    std::fs::write(folder.join("top.txt"), "t").unwrap();
    let mut app = app_in(temp.path());
    let panel = app.active_panel_mut();
    panel.cursor = (panel.entries.iter())
        .position(|e| e.name == "folder")
        .unwrap();
    (temp, app)
}

fn hidden(path: &std::path::Path) -> bool {
    (files::file_properties(path).unwrap().attributes).has(Flag::Hidden)
}

#[test]
fn a_folder_brings_the_box_two_rows_lower_than_the_owner_and_a_file_does_not() {
    let (temp, mut app) = app_on_folder();
    press_ctrl_a(&mut app);
    let form = dialog(&app).0;
    let at = |index: usize| (form.items[index].x, form.items[index].y);
    assert_eq!(form.height, 23);
    assert_eq!(
        form.control(SUBFOLDERS),
        Some(&Control::Check {
            label: "Process sub&folders".into(),
            checked: false
        })
    );
    assert!(!form.items[SUBFOLDERS].hidden && !form.items[SUBFOLDERS].disabled);
    assert_eq!((at(SUBFOLDERS - 1), at(SUBFOLDERS)), ((0, 17), (5, 18)));
    assert_eq!(at(SET).1, 20);
    assert!(
        form.items
            .contains(&crate::app::form::Item::new(30, 4, Control::VRule(14)))
    );

    let file = temp.path().join("folder").join("top.txt");
    let session = Session::read(vec![file], String::new(), None, TimeFormat::current()).unwrap();
    let form = layout::main_form(&session);
    assert_eq!(form.height, 21);
    assert!(form.items[SUBFOLDERS].hidden && form.items[SUBFOLDERS].disabled);
}

fn press_ctrl_a(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
}

#[test]
fn the_box_turns_untouched_boxes_to_unknown_and_blanks_a_folders_times_and_owner() {
    let (_temp, mut app) = app_on_folder();
    press_ctrl_a(&mut app);
    let (form, session) = dialog(&app);
    let (date, owner) = (form.masked_text(layout::date(0)), session.owner.clone());
    focus(&mut app, HIDDEN);
    press(&mut app, KeyCode::Char(' '));
    focus(&mut app, SUBFOLDERS);
    press(&mut app, KeyCode::Char(' '));

    let (form, session) = dialog(&app);
    assert_eq!(form.tristate(FIRST_FLAG), None, "untouched: [?]");
    assert_eq!(form.tristate(HIDDEN), Some(true), "changed by hand: kept");
    assert!(session.three.iter().all(|three| *three));
    assert_eq!(session.advanced, [None; 10]);
    let format = session.format;
    let blank = |mask: &str| crate::app::masked_input::MaskedInput::new(mask, "").text();
    assert_eq!(
        form.masked_text(layout::date(0)),
        blank(&format.date_mask())
    );
    assert_eq!(form.edit_text(OWNER), "");

    press(&mut app, KeyCode::Char(' '));
    let (form, session) = dialog(&app);
    assert_eq!(form.tristate(FIRST_FLAG), session.flags[0]);
    assert_eq!(form.tristate(HIDDEN), Some(true));
    assert!(
        session.three.iter().all(|three| !*three),
        "a folder: two states again"
    );
    assert_eq!(form.masked_text(layout::date(0)), date);
    assert_eq!(form.edit_text(OWNER), owner);
}

#[test]
fn set_with_the_box_changes_everything_inside_and_keeps_each_ones_times() {
    let (temp, mut app) = app_on_folder();
    let folder = temp.path().join("folder");
    let deep = folder.join("inner").join("deep.txt");
    let before = files::file_properties(&deep).unwrap().times;
    press_ctrl_a(&mut app);
    focus(&mut app, SUBFOLDERS);
    press(&mut app, KeyCode::Char(' '));
    focus(&mut app, HIDDEN);
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(dialog(&app).0.tristate(HIDDEN), Some(true));
    submit_dialog(&mut app, DialogOutcome::Button(SET));
    finish_transfer(&mut app);

    for path in [
        &folder,
        &folder.join("inner"),
        &deep,
        &folder.join("top.txt"),
    ] {
        assert!(hidden(path), "{}", path.display());
    }
    let after = files::file_properties(&deep).unwrap().times;
    assert_eq!(
        (after.modified, after.created),
        (before.modified, before.created)
    );
}

#[test]
fn set_from_the_drive_menu_brings_the_menu_back_when_it_ends_in_front() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    std::fs::write(&path, "a").unwrap();
    let mut app = app_in(temp.path());
    let mut wanted = [None; 20];
    wanted[2] = Some(true);
    let request = Request {
        items: vec![path.clone()],
        wanted,
        owner: String::new(),
        times: std::array::from_fn(|_| TimeEdit::default()),
        utc: false,
        format: TimeFormat::current(),
        subfolders: false,
        link_target: None,
    };
    let place = crate::app::drive_actions::MenuPlace {
        side: Side::Left,
        selected: 0,
    };
    app.start_attributes(request, Some(place));
    finish_transfer(&mut app);
    assert!(hidden(&path));
    assert!(matches!(&app.overlay, Some(Overlay::DriveMenu(menu)) if menu.side == Side::Left));
}

fn set_owner_text(app: &mut App, text: &str) {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form.set_edit_text(OWNER, text),
        _ => panic!("no form"),
    }
}

#[test]
fn what_was_changed_by_hand_stays_when_the_box_turns() {
    let (_temp, mut app) = app_on_folder();
    press_ctrl_a(&mut app);
    // A time typed into the write line, an owner typed, a box changed in "More".
    focus(&mut app, layout::date(0));
    // A digit other than the one there (today's date may begin with it).
    let before = dialog(&app).0.masked_text(layout::date(0));
    let digit = if before.starts_with('1') { '2' } else { '1' };
    press(&mut app, KeyCode::Char(digit));
    let typed = dialog(&app).0.masked_text(layout::date(0));
    assert_ne!(typed, before);
    set_owner_text(&mut app, "SOMEONE");
    press(&mut app, KeyCode::End);
    submit_dialog(&mut app, DialogOutcome::Button(layout::MORE));
    focus(&mut app, 1);
    press(&mut app, KeyCode::Char(' '));
    submit_dialog(&mut app, DialogOutcome::Button(layout::MORE_OK));
    let virtual_box = dialog(&app).1.advanced[1];
    focus(&mut app, SUBFOLDERS);
    press(&mut app, KeyCode::Char(' '));

    let (form, session) = dialog(&app);
    assert_eq!(form.masked_text(layout::date(0)), typed);
    assert_ne!(
        form.masked_text(layout::date(1)),
        typed,
        "an untouched line went blank"
    );
    assert_eq!(form.edit_text(OWNER), "SOMEONE");
    assert_eq!(session.advanced[1], virtual_box, "changed in More: kept");
    assert_eq!(session.advanced[0], None);
}

#[test]
fn original_makes_the_times_untouched_and_current_touches_them() {
    let (_temp, mut app) = app_on_folder();
    press_ctrl_a(&mut app);
    submit_dialog(&mut app, DialogOutcome::Button(layout::CURRENT));
    let now = dialog(&app).0.masked_text(layout::date(2));
    focus(&mut app, SUBFOLDERS);
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(
        dialog(&app).0.masked_text(layout::date(2)),
        now,
        "Current: by hand"
    );
    press(&mut app, KeyCode::Char(' '));

    submit_dialog(&mut app, DialogOutcome::Button(layout::ORIGINAL));
    let original = dialog(&app).0.masked_text(layout::date(2));
    focus(&mut app, SUBFOLDERS);
    press(&mut app, KeyCode::Char(' '));
    assert_ne!(
        dialog(&app).0.masked_text(layout::date(2)),
        original,
        "Original: untouched again"
    );
}

#[test]
fn original_under_subfolders_gives_the_folders_times_to_what_is_inside() {
    let (temp, mut app) = app_on_folder();
    let folder = temp.path().join("folder");
    let deep = folder.join("inner").join("deep.txt");
    let folder_time =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_400_000_000);
    let times = files::FileTimes {
        modified: Some(folder_time),
        ..files::FileTimes::default()
    };
    files::set_file_times(&folder, &times).unwrap();
    press_ctrl_a(&mut app);
    focus(&mut app, SUBFOLDERS);
    press(&mut app, KeyCode::Char(' '));
    submit_dialog(&mut app, DialogOutcome::Button(layout::ORIGINAL));
    submit_dialog(&mut app, DialogOutcome::Button(SET));
    finish_transfer(&mut app);
    let modified = files::file_properties(&deep).unwrap().times.modified;
    assert_eq!(modified, Some(folder_time));
}

#[test]
fn an_attributes_task_counts_nothing_and_sent_back_brings_no_menu() {
    let (temp, mut app) = app_on_folder();
    let path = temp.path().join("folder").join("top.txt");
    let request = Request {
        items: vec![path],
        wanted: [None; 20],
        owner: String::new(),
        times: std::array::from_fn(|_| TimeEdit::default()),
        utc: false,
        format: TimeFormat::current(),
        subfolders: false,
        link_target: None,
    };
    let place = crate::app::drive_actions::MenuPlace {
        side: Side::Left,
        selected: 0,
    };
    app.start_attributes(request, Some(place));
    let id = app.shown_task.as_ref().unwrap().id;
    app.tasks.set_paused(id, true);
    app.tasks.pump(std::time::Instant::now());
    let task = app.tasks.get(id).unwrap();
    assert_eq!(crate::app::screens::task_label(task), "Attributes paused");
    let title = app.task_title().unwrap();
    assert_eq!((title.text.as_str(), title.percent), ("Attributes", None));
    press(&mut app, KeyCode::Char('b'));
    assert!(app.shown_task.is_none());
    app.tasks.set_paused(id, false);
    finish_transfer(&mut app);
    assert!(
        app.overlay.is_none(),
        "ended in the background: no drive menu"
    );
}
