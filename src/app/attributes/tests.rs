//! FAR's "Attributes" dialog: its items at FAR's places, what it starts with for one item and for a
//! selection, its keys, and what "Set" changes on disk (only in folders the tests make).

use std::path::Path;
use std::time::{Duration, SystemTime};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::layout::{
    self, FIRST_FLAG, HARD_LINKS, LINK_EDIT, LINK_TEXT, NAME, OWNER, SET, SYSTEM, UTC,
};
use super::*;
use crate::app::dialog::DialogOutcome;
use crate::app::fileops::tests::{app_in, finish_transfer, submit_dialog, wait_for_question};
use crate::app::form::{Control, Place};
use crate::app::{Overlay, Side};
use crate::platform::files::NewLink;
use crate::platform::locale::DateOrder;

const DMY: TimeFormat = TimeFormat {
    order: DateOrder::DayMonthYear,
    date_separator: '.',
    time_separator: ':',
    decimal_separator: '.',
};

const HIDDEN: usize = FIRST_FLAG + 2;

pub(super) fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

pub(super) fn session_for(paths: &[&Path]) -> Session {
    let paths = paths.iter().map(|path| path.to_path_buf()).collect();
    Session::read(paths, "a.txt".to_string(), None, DMY).unwrap()
}

/// An app in a new folder with `a.txt` and `b.txt`, the cursor on `a.txt`.
fn app_with_two() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::write(temp.path().join("b.txt"), "b").unwrap();
    let mut app = app_in(temp.path());
    let panel = app.active_panel_mut();
    panel.cursor = panel
        .entries
        .iter()
        .position(|e| e.name == "a.txt")
        .unwrap();
    (temp, app)
}

pub(super) fn dialog(app: &App) -> (&Form, &Session) {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Attributes(session),
        })) => (form, session),
        _ => panic!("no Attributes dialog"),
    }
}

pub(super) fn form_mut(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no form"),
    }
}

pub(super) fn press_button(app: &mut App, index: usize) {
    submit_dialog(app, DialogOutcome::Button(index));
}

fn attributes_of(path: &Path) -> crate::platform::files::Attributes {
    files::file_properties(path).unwrap().attributes
}

#[test]
fn the_dialog_has_fars_items_at_fars_places() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    std::fs::write(&path, "a").unwrap();
    let form = layout::main_form(&session_for(&[&path]));
    assert_eq!(
        (form.title.as_str(), form.width, form.height),
        ("Attributes", 80, 21)
    );
    let at = |index: usize| (form.items[index].x, form.items[index].y);
    assert_eq!(
        form.focus, FIRST_FLAG,
        "Read only has the focus (DIF_FOCUS)"
    );
    assert_eq!(form.control(NAME), Some(&Control::Label("a.txt".into())));
    assert!(form.items[HARD_LINKS].hidden && form.items[LINK_TEXT].hidden);
    let labels: Vec<&str> = (FIRST_FLAG..FIRST_FLAG + 10)
        .map(|index| match form.control(index) {
            Some(Control::Tristate { label, three, .. }) => {
                assert!(!three, "one item: two states");
                label.as_str()
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        labels[..4],
        ["&Read only", "&Archive", "&Hidden", "&System"]
    );
    assert_eq!((at(FIRST_FLAG), at(FIRST_FLAG + 9)), ((5, 5), (5, 14)));
    assert_eq!((at(MORE), form.items[MORE].place), ((5, 16), Place::At));
    assert_eq!(
        (at(layout::date(0)), at(layout::time(3))),
        ((47, 7), (59, 10))
    );
    assert_eq!(at(UTC), (68, 6));
    assert_eq!((at(OWNER), form.items[OWNER].width), ((32, 16), 43));
    assert_eq!(form.items[ORIGINAL].place, Place::Between(32, 6));
}

#[test]
fn the_dialogs_buttons_and_lines_are_fars() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    std::fs::write(&path, "a").unwrap();
    let form = layout::main_form(&session_for(&[&path]));
    assert!(matches!(
        form.control(SET),
        Some(Control::Button { default: true, .. })
    ));
    assert!(
        !form.items[SYSTEM].disabled,
        "one object: FAR's `SelCount == 1`"
    );
    let rules: Vec<(u16, u16, &Control)> = (form.items.iter().rev().take(2))
        .map(|item| (item.x, item.y, &item.control))
        .collect();
    assert_eq!(
        rules,
        [(30, 14, &Control::HRule(47)), (30, 4, &Control::VRule(14))]
    );
}

#[test]
fn a_link_moves_everything_below_one_row_down_and_shows_its_target() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let link = temp.path().join("j");
    files::make_link(NewLink::Junction, &target, &link).unwrap();
    let session = session_for(&[&link]);
    assert_eq!(session.mode, Mode::Folder);
    let form = layout::main_form(&session);
    // A junction is a folder: "Process subfolders" adds its two rows too.
    assert_eq!(form.height, 24);
    assert_eq!(
        form.control(LINK_TEXT),
        Some(&Control::Text("&Junction:".into()))
    );
    assert_eq!(form.edit_text(LINK_EDIT), target.display().to_string());
    assert!(
        !form.items[LINK_EDIT].read_only,
        "a junction's target is edited there"
    );
    assert_eq!(form.items[FIRST_FLAG].y, 6);
    assert_eq!(form.items[layout::SUBFOLDERS].y, 19);
    assert_eq!(form.items[SET].y, 21);
}

#[test]
fn a_file_with_hard_links_shows_them_instead_of_its_name() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    std::fs::write(&path, "a").unwrap();
    std::fs::hard_link(&path, temp.path().join("b.txt")).unwrap();
    let form = layout::main_form(&session_for(&[&path]));
    assert!(form.items[NAME].hidden && !form.items[HARD_LINKS].hidden);
    let Some(Control::DropDown { lines, caption, .. }) = form.control(HARD_LINKS) else {
        panic!("no list");
    };
    assert_eq!(
        (lines.len(), caption.as_deref()),
        (2, Some("hard links (2)"))
    );
}

#[test]
fn a_selection_shows_what_some_have_as_unknown_and_shares_only_common_values() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a.txt");
    let b = temp.path().join("b.txt");
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    files::set_attributes(&a, attributes_of(&a).with(Flag::Hidden, true)).unwrap();
    let old = SystemTime::now() - Duration::from_secs(86_400);
    let times = files::FileTimes {
        modified: Some(old),
        ..files::FileTimes::default()
    };
    files::set_file_times(&a, &times).unwrap();
    let session = session_for(&[&a, &b]);
    assert_eq!(session.mode, Mode::Multiple);
    assert_eq!(
        (session.flags[2], session.three[2]),
        (None, true),
        "hidden: some"
    );
    assert_eq!(
        (session.flags[1], session.three[1]),
        (Some(true), false),
        "archive: all"
    );
    assert_eq!(session.flags[0], Some(false), "read only: none");
    assert_eq!(session.times[0], None, "write times differ");
    assert!(!session.owner.is_empty(), "both have the same owner");
    let form = layout::main_form(&session);
    assert_eq!(
        form.control(NAME),
        Some(&Control::Label("selected objects".into()))
    );
    assert!(form.items[ORIGINAL].disabled);
    assert_eq!(form.masked_text(layout::date(0)), "  .  .     ");
}

#[test]
fn a_date_typed_over_the_field_is_the_files_new_write_date() {
    let (temp, mut app) = app_with_two();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let format = dialog(&app).1.format;
    let wanted = SystemTime::UNIX_EPOCH + Duration::from_secs(1_741_996_800); // 2025-03-15 00:00 UTC
    let (date, _) = times::show(Some(wanted), false, format);
    form_mut(&mut app).focus = layout::time(0);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    for ch in date.chars().filter(char::is_ascii_digit) {
        press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    assert_eq!(
        dialog(&app).0.masked_text(layout::date(0)).trim(),
        date.trim()
    );
    let before = files::file_properties(&temp.path().join("a.txt"))
        .unwrap()
        .times;
    press_button(&mut app, SET);
    finish_transfer(&mut app);

    let after = files::file_properties(&temp.path().join("a.txt"))
        .unwrap()
        .times;
    let written = times::show(after.modified, false, format).0;
    assert_eq!(written, date, "the day typed, the time of day kept");
    assert_eq!(
        after.created, before.created,
        "lines left as shown are not written"
    );
}

#[test]
fn a_long_name_is_cut_inside_the_frame() {
    let session = Session {
        shown_name: "n".repeat(120),
        ..session_for(&[&std::env::temp_dir()])
    };
    let form = layout::main_form(&session);
    let Some(Control::Label(name)) = form.control(NAME) else {
        panic!("no name");
    };
    assert_eq!((form.items[NAME].x, name.chars().count()), (5, 70));
    assert!(name.contains('…'));
}

#[test]
fn ctrl_a_and_set_make_a_file_hidden_and_take_the_selection_off() {
    let (temp, mut app) = app_with_two();
    app.active_panel_mut().selected.insert("a.txt".to_string());
    let before = files::file_properties(&temp.path().join("a.txt"))
        .unwrap()
        .times;

    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(dialog(&app).1.mode, Mode::File);
    form_mut(&mut app).focus = HIDDEN;
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(app.overlay.is_none());
    assert!(app.shown_task.is_some(), "FAR's progress is in front");
    finish_transfer(&mut app);
    assert!(attributes_of(&temp.path().join("a.txt")).has(Flag::Hidden));
    assert!(!attributes_of(&temp.path().join("b.txt")).has(Flag::Hidden));
    let after = files::file_properties(&temp.path().join("a.txt"))
        .unwrap()
        .times;
    assert_eq!(
        (after.modified, after.created),
        (before.modified, before.created)
    );
    assert!(app.active_panel().selected.is_empty());
}

#[test]
fn set_on_a_selection_leaves_unknown_boxes_and_changes_the_set_ones_on_all() {
    let (temp, mut app) = app_with_two();
    let a = temp.path().join("a.txt");
    files::set_attributes(&a, attributes_of(&a).with(Flag::Hidden, true)).unwrap();
    let panel = app.active_panel_mut();
    panel.selected.insert("a.txt".to_string());
    panel.selected.insert("b.txt".to_string());

    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(dialog(&app).0.tristate(HIDDEN), None);
    form_mut(&mut app).focus = FIRST_FLAG + 8;
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press_button(&mut app, SET);
    finish_transfer(&mut app);

    for (name, hidden) in [("a.txt", true), ("b.txt", false)] {
        let attributes = attributes_of(&temp.path().join(name));
        assert!(attributes.has(Flag::Temporary), "{name}");
        assert_eq!(
            attributes.has(Flag::Hidden),
            hidden,
            "{name}: [?] leaves it"
        );
    }
}

#[test]
fn a_new_write_date_is_set_and_a_star_gives_it_to_the_other_lines() {
    let (temp, mut app) = app_with_two();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let format = dialog(&app).1.format;
    let wanted = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    let (date, time) = times::show(Some(wanted), false, format);
    let form = form_mut(&mut app);
    form.set_masked_text(layout::date(0), &date);
    form.set_masked_text(layout::time(0), &time);
    form.focus = layout::time(0);
    press(&mut app, KeyCode::Char('*'), KeyModifiers::NONE);
    let form = dialog(&app).0;
    assert_eq!(
        form.masked_text(layout::date(1)),
        form.masked_text(layout::date(0))
    );
    press_button(&mut app, SET);
    finish_transfer(&mut app);

    let read = files::file_properties(&temp.path().join("a.txt")).unwrap();
    assert_eq!(read.times.modified, Some(wanted));
    assert_eq!(read.times.created, Some(wanted));
}

#[test]
fn blank_changes_no_time_and_current_shows_now() {
    let (temp, mut app) = app_with_two();
    let before = files::file_properties(&temp.path().join("a.txt"))
        .unwrap()
        .times;
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    press_button(&mut app, CURRENT);
    let (form, session) = dialog(&app);
    let (today, _) = times::show(Some(SystemTime::now()), false, session.format);
    assert_eq!(form.masked_text(layout::date(2)).trim(), today.trim());
    assert_eq!(form.focus, layout::date(0));
    press_button(&mut app, BLANK);
    let blank = format!(":  :  {}", dialog(&app).1.format.decimal_separator);
    assert_eq!(dialog(&app).0.masked_text(layout::time(0)).trim(), blank);
    press_button(&mut app, SET);
    finish_transfer(&mut app);
    let after = files::file_properties(&temp.path().join("a.txt"))
        .unwrap()
        .times;
    assert_eq!(
        (after.modified, after.created),
        (before.modified, before.created)
    );
}

#[test]
fn utc_shows_the_same_times_in_utc_and_back() {
    let (_temp, mut app) = app_with_two();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let local = dialog(&app).0.masked_text(layout::time(0));
    form_mut(&mut app).focus = UTC;
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    let (form, session) = dialog(&app);
    assert!(session.utc);
    let utc = times::show(session.times[0], true, session.format).1;
    assert_eq!(form.masked_text(layout::time(0)), utc);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(dialog(&app).0.masked_text(layout::time(0)), local);
}

#[test]
fn compressed_and_encrypted_are_never_both_on() {
    let (_temp, mut app) = app_with_two();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    form_mut(&mut app).focus = FIRST_FLAG + ENCRYPTED;
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    form_mut(&mut app).focus = FIRST_FLAG + COMPRESSED;
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    let form = dialog(&app).0;
    assert_eq!(form.tristate(FIRST_FLAG + COMPRESSED), Some(true));
    assert_eq!(form.tristate(FIRST_FLAG + ENCRYPTED), Some(false));
}

#[test]
fn more_keeps_its_boxes_on_ok_and_a_reparse_point_can_only_go() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    files::make_link(NewLink::Junction, &target, &temp.path().join("j")).unwrap();
    let mut app = app_in(temp.path());
    let panel = app.active_panel_mut();
    panel.cursor = panel.entries.iter().position(|e| e.name == "j").unwrap();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    press_button(&mut app, MORE);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no More");
    };
    assert_eq!(
        form.tristate(0),
        Some(true),
        "the junction is a reparse point"
    );
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(
        form_mut(&mut app).tristate(0),
        Some(false),
        "it cannot be set again"
    );
    press_button(&mut app, layout::MORE_OK);
    assert_eq!(dialog(&app).1.advanced[0], Some(false));
    press_button(&mut app, SET);
    finish_transfer(&mut app);
    let read = files::file_properties(&temp.path().join("j")).unwrap();
    assert!(read.link.is_none() && read.attributes.is_directory());
}

#[test]
fn a_change_that_fails_asks_and_skip_goes_on_with_the_next() {
    let (temp, mut app) = app_with_two();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let form = form_mut(&mut app);
    if let Some(Control::Edit(input)) = form.items.get_mut(OWNER).map(|item| &mut item.control) {
        *input = crate::app::LineInput::with_text("no-such-account-6f1c");
    }
    form.focus = HIDDEN;
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press_button(&mut app, SET);
    wait_for_question(&mut app);

    let Some(Overlay::Dialog(Dialog::Form { form, action })) = &app.overlay else {
        panic!("no question");
    };
    assert!(matches!(action, FormAction::AnswerAttributes { .. }));
    assert_eq!(form.title, "Error");
    assert!(
        form.items
            .iter()
            .any(|item| item.control == Control::Label("Cannot set owner for".into()))
    );
    let skip = (form.items.iter())
        .position(|item| matches!(&item.control, Control::Button { label, .. } if label == "&Skip"))
        .unwrap();
    press_button(&mut app, skip);
    finish_transfer(&mut app);
    assert!(attributes_of(&temp.path().join("a.txt")).has(Flag::Hidden));
    let Some(Overlay::Dialog(report)) = &app.overlay else {
        panic!("no report");
    };
    assert!(format!("{report:?}").contains("Cannot set owner for"));
}

#[test]
fn the_files_menu_and_esc_on_the_drive_menus_f4_bring_fars_dialog_and_the_menu_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let files_menu = crate::app::MENUS[1].items;
    assert!(files_menu.iter().any(|item| matches!(
        item,
        crate::app::MenuItem::Item {
            label: "File &attributes",
            key: "Ctrl+A",
            command: crate::app::Command::Attributes
        }
    )));
    let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
    let place = MenuPlace {
        side: Side::Right,
        selected: 0,
    };
    app.ask_root_attributes(&root, place);
    let (form, session) = dialog(&app);
    assert_eq!(
        form.control(LINK_TEXT),
        Some(&Control::Text("&Volume:".into()))
    );
    assert_eq!(session.shown_name, root);
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert!(matches!(&app.overlay, Some(Overlay::DriveMenu(menu)) if menu.side == Side::Right));
}

#[test]
fn ctrl_a_reads_off_the_screens_thread_and_esc_shows_no_dialog() {
    // ADR-0019, review R16: the items' properties (owners' names too) from a disk slow to answer.
    use crate::app::waiting::{finish_waiting, hold_next_work, shows_stopped};
    let (_temp, mut app) = app_with_two();
    let release = hold_next_work();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(app.is_waiting(), "read off the screen's thread");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(shows_stopped(&app));
    drop(release);
    assert!(!app.pump_waiting(), "its late end is not done");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none(), "no dialog");
    let release = hold_next_work();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let _ = release.send(());
    finish_waiting(&mut app);
    assert_eq!(dialog(&app).1.targets.len(), 1, "read: the dialog on a.txt");
}
