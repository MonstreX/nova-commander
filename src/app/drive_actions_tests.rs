//! The drive menu's actions up to the point where a device would be touched: which question each drive
//! gets, and the menu coming back after No, Esc, Cancel and a failure. Nothing here disconnects, ejects
//! or relabels a real drive: the only actions run are on a letter nothing is mounted at.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::dialog::DialogOutcome;
use crate::app::fileops::tests::{app_in, submit_dialog};
use crate::app::waiting::finish_waiting;
use crate::app::{DriveMenu, Overlay};

fn drive(letter: char, kind: DriveKind) -> Drive {
    Drive {
        root: format!("{letter}:\\"),
        letter: Some(letter),
        kind,
        disconnected: false,
        subst: None,
        image: None,
        drive_type: None,
        label: None,
        file_system: None,
        size: None,
        associated_path: None,
        volume_read: false,
    }
}

/// A letter nothing is mounted at.
fn free_letter() -> char {
    let mounted: Vec<char> = platform::volumes::volume_roots()
        .iter()
        .filter_map(|root| root.letter)
        .collect();
    ('D'..='Z')
        .rev()
        .find(|letter| !mounted.contains(letter))
        .unwrap()
}

fn system_letter() -> char {
    std::env::var("SystemDrive")
        .ok()
        .and_then(|drive| drive.chars().next())
        .unwrap_or('C')
}

/// The menu on the right with `drives`, the second line selected, and `key` pressed in it.
fn press_on(app: &mut App, drives: Vec<Drive>, code: KeyCode, modifiers: KeyModifiers) {
    let menu = DriveMenu {
        side: Side::Right,
        drives,
        plugins: Vec::new(),
        selected: 1,
        columns: crate::vfs::local::DriveColumns::default(),
        top: std::cell::Cell::new(0),
    };
    app.handle_drive_menu_key(menu, KeyEvent::new(code, modifiers));
    finish_waiting(app);
}

/// A button of the dialog, and the work it starts ended.
fn answer(app: &mut App, outcome: DialogOutcome) {
    submit_dialog(app, outcome);
    finish_waiting(app);
}

fn menu_is_back(app: &mut App) -> bool {
    finish_waiting(app);
    matches!(&app.overlay, Some(Overlay::DriveMenu(menu)) if menu.side == Side::Right)
}

fn two(second: Drive) -> Vec<Drive> {
    vec![drive(system_letter(), DriveKind::Fixed), second]
}

#[test]
fn del_asks_before_a_subst_drive_and_no_brings_the_menu_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let mut subst = drive(free_letter(), DriveKind::Fixed);
    subst.subst = Some(r"C:\work".to_string());
    press_on(&mut app, two(subst), KeyCode::Delete, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm { title, lines, .. })) = &app.overlay else {
        panic!("a question");
    };
    assert_eq!(title, "Substituted disk deletion");
    assert_eq!(lines[2], r"C:\work");
    submit_dialog(&mut app, DialogOutcome::Button(1));
    assert!(menu_is_back(&mut app));
}

#[test]
fn del_asks_before_a_virtual_disk_is_detached_and_no_brings_the_menu_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let letter = free_letter();
    let mut image = drive(letter, DriveKind::Fixed);
    image.image = Some(r"C:\disks\work.vhdx".to_string());
    press_on(&mut app, two(image), KeyCode::Delete, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm {
        title,
        lines,
        buttons,
        action,
        ..
    })) = &app.overlay
    else {
        panic!("a question");
    };
    assert_eq!(title, "Virtual disk detaching");
    assert_eq!(
        lines,
        &[
            format!("Detach virtual disk {letter}:?"),
            "The drive is mapped to:".to_string(),
            r"C:\disks\work.vhdx".to_string(),
        ]
    );
    assert_eq!(buttons, &["Yes", "No"]);
    assert!(matches!(
        action,
        ConfirmAction::Drive(DriveAction::DetachVirtual(_), _)
    ));
    submit_dialog(&mut app, DialogOutcome::Button(1));
    assert!(menu_is_back(&mut app));
}

#[test]
fn shift_del_detaches_a_virtual_disk_without_asking_and_a_failure_names_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let letter = free_letter();
    let mut image = drive(letter, DriveKind::Fixed);
    image.image = Some(r"C:\disks\work.vhdx".to_string());
    // Nothing is mounted at the letter: the detaching fails and says so.
    press_on(&mut app, two(image), KeyCode::Delete, KeyModifiers::SHIFT);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, buttons, .. })) = &app.overlay else {
        panic!("the error");
    };
    assert_eq!(lines[0], "Cannot detach a virtual disk");
    assert_eq!(lines[1], format!("{letter}:"));
    assert_eq!(buttons, &["&Retry", "&Cancel"]);
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert!(menu_is_back(&mut app));
}

#[test]
fn del_on_a_network_drive_asks_with_reconnect_only_when_it_is_restored_at_logon() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let mut network = drive(free_letter(), DriveKind::Remote);
    network.disconnected = true;
    network.associated_path = Some(r"\server\share".to_string());
    press_on(&mut app, two(network), KeyCode::Delete, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Form { form, action })) = &app.overlay else {
        panic!("the disconnect dialog");
    };
    assert_eq!(form.title, "Disconnect network drive");
    assert!(form.warning);
    let FormAction::Disconnect { reconnect, .. } = action else {
        panic!("a disconnect");
    };
    assert_eq!(*reconnect, None, "a drive not connected now is forgotten");
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert!(menu_is_back(&mut app));
}

#[test]
fn del_ins_and_shift_del_leave_a_fixed_disk_alone() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let fixed = drive(system_letter(), DriveKind::Fixed);
    for (code, modifiers) in [
        (KeyCode::Delete, KeyModifiers::NONE),
        (KeyCode::Delete, KeyModifiers::SHIFT),
        (KeyCode::Insert, KeyModifiers::NONE),
    ] {
        app.overlay = None;
        press_on(&mut app, two(fixed.clone()), code, modifiers);
        assert!(menu_is_back(&mut app), "{code:?} {modifiers:?}");
    }
}

#[test]
fn a_failure_offers_retry_and_cancel_brings_the_menu_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let letter = free_letter();
    let place = MenuPlace {
        side: Side::Right,
        selected: 0,
    };
    app.run_drive_action(DriveAction::Load(format!("{letter}:\\")), place, false);
    finish_waiting(&mut app);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, buttons, .. })) = &app.overlay else {
        panic!("the error");
    };
    assert_eq!(
        lines[0],
        format!("{letter}:"),
        "no operation text for loading"
    );
    assert_eq!(buttons, &["&Retry", "&Cancel"]);
    // Retry fails again on the same letter and asks again.
    answer(&mut app, DialogOutcome::Button(0));
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::Drive(DriveAction::Load(_), _),
            ..
        }))
    ));
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert!(menu_is_back(&mut app));
}

#[test]
fn shift_f6_asks_for_the_label_and_esc_brings_the_menu_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let system = drive(system_letter(), DriveKind::Fixed);
    press_on(&mut app, two(system), KeyCode::F(6), KeyModifiers::SHIFT);
    let Some(Overlay::Dialog(Dialog::Input { title, .. })) = &app.overlay else {
        panic!("the label dialog");
    };
    assert_eq!(title, "Volume label");
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert!(menu_is_back(&mut app));

    // A drive that cannot be read: an error, then the menu.
    app.overlay = None;
    press_on(
        &mut app,
        two(drive(free_letter(), DriveKind::Removable)),
        KeyCode::F(6),
        KeyModifiers::SHIFT,
    );
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::BackToDriveMenu(_),
            ..
        }))
    ));
    submit_dialog(&mut app, DialogOutcome::Button(0));
    assert!(menu_is_back(&mut app));
}

/// The error of the action run at once, on a letter nothing is mounted at.
fn failed_action(app: &App) -> &DriveAction {
    let Some(Overlay::Dialog(Dialog::Confirm {
        title,
        action: ConfirmAction::Drive(action, _),
        ..
    })) = &app.overlay
    else {
        panic!("the action's error");
    };
    assert_eq!(title, "Error", "no question");
    action
}

#[test]
fn with_its_question_off_a_subst_drive_is_removed_at_once() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.prefs.confirm_subst = false;
    let mut subst = drive(free_letter(), DriveKind::Fixed);
    subst.subst = Some(r"C:\work".to_string());
    press_on(&mut app, two(subst), KeyCode::Delete, KeyModifiers::NONE);
    assert!(matches!(failed_action(&app), DriveAction::RemoveSubst(_)));
}

#[test]
fn with_its_question_off_a_virtual_disk_is_detached_at_once() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.prefs.confirm_virtual = false;
    let mut image = drive(free_letter(), DriveKind::Fixed);
    image.image = Some(r"C:\disks\work.vhdx".to_string());
    press_on(&mut app, two(image), KeyCode::Delete, KeyModifiers::NONE);
    assert!(matches!(failed_action(&app), DriveAction::DetachVirtual(_)));
}

#[test]
fn with_its_question_off_a_connection_not_restored_at_logon_is_forgotten() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.prefs.confirm_connection = false;
    let remote = drive(free_letter(), DriveKind::Remote);
    press_on(&mut app, two(remote), KeyCode::Delete, KeyModifiers::NONE);
    assert!(matches!(
        failed_action(&app),
        DriveAction::Disconnect { forget: true, .. }
    ));
}

#[test]
fn esc_while_a_drive_action_runs_brings_the_menu_back_after_the_notice() {
    // ADR-0019 (review of 19-5): as F4's wait keeps the menu.
    use crate::app::waiting::hold_next_work;
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let place = MenuPlace {
        side: Side::Right,
        selected: 0,
    };
    let release = hold_next_work();
    // Loading media on a letter no drive has: nothing happens when it runs late.
    let action = DriveAction::Load(format!("{}:\\", free_letter()));
    app.run_drive_action(action, place, false);
    assert!(app.is_waiting());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Confirm { title, .. })) if title == "Stopped"),
        "the notice"
    );
    drop(release);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(menu_is_back(&mut app), "the menu read again");
}

#[test]
fn a_refused_name_or_password_asks_for_them_and_cancel_brings_the_menu_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let place = MenuPlace {
        side: Side::Left,
        selected: 0,
    };
    let connect = |login| DriveAction::Connect {
        root: "Q:\\".to_string(),
        share: r"\192.0.2.1\share".to_string(),
        login,
    };
    // ERROR_LOGON_FAILURE: FAR's own dialog, titled with the share.
    let refused = || io::Error::from_raw_os_error(1326);
    app.drive_action_ended(connect(None), place, Err(refused()));
    let Some(Overlay::Dialog(Dialog::Form {
        form,
        action: FormAction::NetworkLogin { share, .. },
    })) = &app.overlay
    else {
        panic!("the name and password asked");
    };
    assert_eq!(form.title, r"\192.0.2.1\share");
    assert_eq!(share, r"\192.0.2.1\share");
    // Refused again with what was typed: asked again.
    app.overlay = None;
    let login = login::Login {
        user: "me".to_string(),
        password: "wrong".to_string(),
    };
    app.drive_action_ended(connect(Some(login)), place, Err(refused()));
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::NetworkLogin { .. },
            ..
        }))
    ));
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert!(matches!(app.overlay, Some(Overlay::DriveMenu(_))));
    // Another failure is the error with Retry.
    app.overlay = None;
    let gone = io::Error::from_raw_os_error(67);
    app.drive_action_ended(connect(None), place, Err(gone));
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::Drive(DriveAction::Connect { .. }, _),
            ..
        }))
    ));
}
