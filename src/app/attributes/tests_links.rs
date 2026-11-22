//! The "Attributes" dialog's links and device (FAR's `ModifyReparsePoint`, `symlink_type`, "Device:"),
//! the mouse on its time labels, "System properties", and "Access denied" on "Set" (ADR-0007): only in
//! folders the tests make, rights denied only on their own files.

use std::path::Path;
use std::process::Command;

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::layout::{
    self, DEVICE_EDIT, DEVICE_TEXT, FIRST_FLAG, LINK_EDIT, LINK_TEXT, SET, SYSTEM,
};
use super::tests::{dialog, form_mut, press, press_button, session_for};
use super::*;
use crate::app::dialog::DialogOutcome;
use crate::app::elevation_question;
use crate::app::fileops::tests::{app_in, finish_transfer, submit_dialog, wait_for_question};
use crate::app::form::Control;
use crate::app::{Overlay, Side};
use crate::platform::files::{LinkKind, NewLink};

fn session_with_link(kind: LinkKind, target: &str, known: bool) -> Session {
    let temp = std::env::temp_dir();
    Session {
        link: Some(LinkFacts {
            kind,
            target: target.to_string(),
            volume: false,
            known,
            editable: false,
        }),
        ..session_for(&[&temp])
    }
}

fn text_of(form: &Form, index: usize) -> String {
    match form.control(index) {
        Some(Control::Text(text) | Control::Label(text)) => text.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_app_alias_and_a_wsl_link_show_their_targets_under_fars_labels() {
    let alias = layout::main_form(&session_with_link(
        LinkKind::Reparse(0x8000_001B),
        r"C:\Program Files\WindowsApps\wt.exe",
        true,
    ));
    assert_eq!(text_of(&alias, LINK_TEXT), "App Alias:");
    assert_eq!(
        alias.edit_text(LINK_EDIT),
        r"C:\Program Files\WindowsApps\wt.exe"
    );
    assert!(alias.items[LINK_EDIT].read_only && !alias.items[LINK_EDIT].hidden);
    let wsl = layout::main_form(&session_with_link(
        LinkKind::Reparse(0xA000_001D),
        "../x",
        true,
    ));
    assert_eq!(text_of(&wsl, LINK_TEXT), "S&ymlink LX:");
    let nfs = layout::main_form(&session_with_link(
        LinkKind::Reparse(0x8000_0014),
        "/x",
        true,
    ));
    assert_eq!(text_of(&nfs, LINK_TEXT), "S&ymlink NFS:");
    // One the system says nothing of: its tag's name across the dialog, no target.
    let cloud = layout::main_form(&session_with_link(
        LinkKind::Reparse(0x9000_001A),
        "",
        false,
    ));
    assert_eq!(text_of(&cloud, LINK_TEXT), "Reparse point CLOUD");
    assert!(cloud.items[LINK_EDIT].hidden);
}

#[test]
fn a_junctions_new_target_is_set_and_one_differing_only_by_case_changes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let (first, second) = (temp.path().join("first"), temp.path().join("second"));
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let link = temp.path().join("j");
    files::make_link(NewLink::Junction, &first, &link).unwrap();
    let session = session_for(&[&link]);
    let mut form = layout::main_form(&session);
    let expand = |text: &str| text.to_string();
    form.set_edit_text(LINK_EDIT, &first.display().to_string().to_uppercase());
    assert_eq!(apply::request(&session, &form, &expand).link_target, None);
    form.set_edit_text(LINK_EDIT, "\"second\"");
    assert_eq!(
        apply::request(&session, &form, &expand)
            .link_target
            .as_deref(),
        Some("second"),
        "unquoted, as FAR's `unquote`"
    );

    let mut app = app_in(temp.path());
    let panel = app.active_panel_mut();
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "j")
        .unwrap();
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    form_mut(&mut app).set_edit_text(LINK_EDIT, "%NC_P43_TARGET%");
    app.environment.set("NC_P43_TARGET", "second");
    press_button(&mut app, SET);
    finish_transfer(&mut app);

    assert_eq!(files::link_target(&link).unwrap(), second);
}

#[test]
fn system_properties_is_for_one_object_only() {
    let temp = tempfile::tempdir().unwrap();
    let (a, b) = (temp.path().join("a.txt"), temp.path().join("b.txt"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    assert!(!layout::main_form(&session_for(&[&a])).items[SYSTEM].disabled);
    assert!(layout::main_form(&session_for(&[&a, &b])).items[SYSTEM].disabled);
}

#[test]
fn the_drive_menus_f4_shows_the_drives_device_under_its_volume() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
    let place = MenuPlace {
        side: Side::Left,
        selected: 0,
    };
    app.ask_root_attributes(&root, place);
    let (form, session) = dialog(&app);
    assert!(session.device.is_some());
    assert_eq!(text_of(form, DEVICE_TEXT), "Device:");
    assert!(form.edit_text(DEVICE_EDIT).starts_with(r"\Device\"));
    assert!(form.items[DEVICE_EDIT].read_only);
    let at = |index: usize| (form.items[index].x, form.items[index].y);
    assert_eq!(
        (at(LINK_TEXT), at(DEVICE_TEXT), at(DEVICE_EDIT)),
        ((5, 4), (5, 5), (18, 5))
    );
    assert_eq!(
        at(FIRST_FLAG),
        (5, 7),
        "two rows lower: the volume and the device"
    );

    // From a panel no device is asked for (FAR's `!SrcPanel`).
    let session = session_for(&[Path::new(&root)]);
    assert!(session.device.is_none());
    assert!(layout::main_form(&session).items[DEVICE_TEXT].hidden);
}

#[test]
fn a_click_on_a_times_label_goes_to_its_date_and_a_double_click_puts_now_there() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    std::fs::write(&path, "a").unwrap();
    let mut session = session_for(&[&path]);
    let form = layout::main_form(&session);
    // As the dialog is shown: the fields as they start.
    remember(&form, &mut session);
    let mut dialog = Dialog::Form {
        form,
        action: FormAction::Attributes(Box::new(session.clone())),
    };
    let label = layout::date(2) - 1;
    assert!(text_click(&mut dialog, label, false));
    let Dialog::Form { form, .. } = &mut dialog else {
        unreachable!()
    };
    assert_eq!(form.focus, layout::date(2));
    form.set_masked_text(layout::date(2), "");
    form.set_masked_text(layout::time(2), "");

    assert!(text_click(&mut dialog, label, true));
    follow(&mut dialog);
    let Dialog::Form {
        form,
        action: FormAction::Attributes(after),
    } = &dialog
    else {
        unreachable!()
    };
    let (today, _) = times::show(Some(std::time::SystemTime::now()), false, session.format);
    assert_eq!(form.masked_text(layout::date(2)).trim(), today.trim());
    assert!(after.touched.times[2], "a change by hand, as typed");
    assert!(!after.touched.times[0]);
    // Another text of the dialog is no time's label.
    assert!(!text_click(&mut dialog, layout::OWNER - 1, false));
}

fn icacls(path: &Path, arguments: &[&str]) {
    let done = Command::new("icacls")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(done.status.success(), "{done:?}");
}

/// The test's own file whose attributes cannot be written while the guard lives (Everyone, by its
/// SID, is denied "write attributes").
struct Protected<'a>(&'a Path);

impl<'a> Protected<'a> {
    fn new(path: &'a Path) -> Self {
        icacls(path, &["/deny", "*S-1-1-0:(WA)"]);
        Self(path)
    }
}

impl Drop for Protected<'_> {
    fn drop(&mut self) {
        icacls(self.0, &["/remove:d", "*S-1-1-0"]);
    }
}

#[test]
fn set_where_the_rights_are_missing_asks_access_denied_and_skip_leaves_the_error() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    std::fs::write(&path, "a").unwrap();
    let mut app = app_in(temp.path());
    let panel = app.active_panel_mut();
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "a.txt")
        .unwrap();
    let protected = Protected::new(&path);
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    form_mut(&mut app).focus = FIRST_FLAG + 2;
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    press_button(&mut app, SET);
    wait_for_question(&mut app);

    if crate::platform::elevation::can_elevate() {
        let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
            panic!("no question");
        };
        assert!(elevation_question::is_it(form), "{form:?}");
        assert!(format!("{form:?}").contains("to set attributes of this object:"));
        submit_dialog(&mut app, DialogOutcome::Button(elevation_question::SKIP));
        wait_for_question(&mut app);
    }
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no error");
    };
    assert_eq!(form.title, "Error");
    assert!(format!("{form:?}").contains("Cannot set attributes for"));
    submit_dialog(&mut app, DialogOutcome::Cancel);
    finish_transfer(&mut app);
    drop(protected);
    assert!(
        !files::file_properties(&path)
            .unwrap()
            .attributes
            .has(Flag::Hidden)
    );
}
