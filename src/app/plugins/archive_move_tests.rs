//! F6 out of an archive's panel (arclite's `move`) through the program and the shipped plugin: what was
//! extracted is deleted from the archive only when nothing failed and "Delete files from archive" stayed
//! on; an entry it cannot tell from another is refused when the archive could be changed; and a folder
//! made that is no longer waited for still has the plugin's panels read again.

use super::*;
use crate::app::form::Control;
use crate::plugin_host::Event;

/// The plugin's dialog up: its check box (or three-state one) whose label starts with `label` set to
/// `state` (`None`: `[?]`), as the user would leave it.
fn set_box(app: &mut App, label: &str, state: Option<bool>) {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("a dialog up");
    };
    let item = (form.items.iter_mut())
        .find(|item| match &item.control {
            Control::Check { label: own, .. } | Control::Tristate { label: own, .. } => {
                own.starts_with(label)
            }
            _ => false,
        })
        .unwrap_or_else(|| panic!("no box {label}"));
    match &mut item.control {
        Control::Check { checked, .. } => *checked = state == Some(true),
        Control::Tristate { state: own, .. } => *own = state,
        _ => unreachable!(),
    }
}

/// Whether the plugin's dialog has the box whose label starts with `label` disabled.
fn box_disabled(app: &App, label: &str) -> bool {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("a dialog up");
    };
    (form.items.iter()).any(|item| {
        item.disabled
            && matches!(&item.control, Control::Check { label: own, .. }
                | Control::Tristate { label: own, .. } if own.starts_with(label))
    })
}

/// The state of the plugin's three-state box whose label starts with `label` (`None`: `[?]`).
fn box_state(app: &App, label: &str) -> Option<bool> {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("a dialog up");
    };
    (form.items.iter())
        .find_map(|item| match &item.control {
            Control::Tristate {
                label: own, state, ..
            } if own.starts_with(label) => Some(*state),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no box {label}"))
}

const MOVE_BOX: &str = "&Delete files from archive";

/// F6 on `name` of the open archive: arclite's Extract dialog up.
fn f6_dialog(app: &mut App, name: &str) {
    point(app, name);
    press(app, KeyCode::F(6));
    tick_until(app, |app| plugin_window(app).is_some());
    assert_eq!(
        plugin_window(app),
        Some(("dialog", "Extract files".to_string()))
    );
}

/// A file whose data is not what its CRC says: extracted with "Ignore errors" its failure is logged, so
/// F6 deletes nothing (arclite deletes only after an extraction without errors).
#[test]
fn f6_with_a_failure_logged_deletes_nothing() {
    let (_plugins, files, out, mut app) = app_with_archives("archivemovefault");
    let mut zip = zip_of(&[("bad.txt", b"good data"), ("ok.txt", b"ok")]);
    let at = (zip.windows(9).position(|bytes| bytes == b"good data")).unwrap();
    zip[at..at + 9].copy_from_slice(b"evil data");
    let archive = files.path().join("bad.zip");
    std::fs::write(&archive, &zip).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| {
        app.panels[0].reading().is_none() && names(app).contains(&"bad.zip".to_string())
    });
    open_archive(&mut app, "bad.zip");
    f6_dialog(&mut app, "bad.txt");
    set_box(&mut app, "&Ignore errors", Some(true));
    press(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| {
        plugin_window(app) == Some(("message", "Operation log".to_string()))
    });
    press(&mut app, KeyCode::Esc);
    settled(&mut app);
    assert!(
        out.path().join("bad.txt").exists(),
        "extracted, its data failed"
    );
    assert_eq!(std::fs::read(&archive).unwrap(), zip, "nothing deleted");
    assert_eq!(zip_names(&archive), ["bad.txt", "ok.txt"]);
}

/// F6 with "Delete files from archive" turned off in the dialog: extracted as F5, the archive as it was.
#[test]
fn f6_with_deleting_turned_off_extracts_only() {
    let (_plugins, files, out, mut app) = app_with_archives("archivemoveoff");
    let archive = files.path().join("a.zip");
    let before = std::fs::read(&archive).unwrap();
    open_archive(&mut app, "a.zip");
    f6_dialog(&mut app, "top.txt");
    assert!(!box_disabled(&app, MOVE_BOX), "an archive that can change");
    assert_eq!(
        box_state(&app, MOVE_BOX),
        Some(true),
        "F6: arclite's triTrue"
    );
    set_box(&mut app, MOVE_BOX, Some(false));
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    tick_until(&mut app, |_| {
        std::fs::read(out.path().join("top.txt")).is_ok_and(|data| data == b"top")
    });
    assert_eq!(std::fs::read(&archive).unwrap(), before, "nothing deleted");
}

/// A file and a folder of one name in an archive that can change: F5 extracts it (nothing deleted
/// after), F6 is refused before its dialog - deleting either might take the other -, nothing written.
#[test]
fn f6_on_a_name_of_a_file_and_a_folder_is_refused() {
    let (_plugins, files, out, mut app) = app_with_archives("archivemovetwins");
    let twins: &[(&str, &[u8])] = &[("x/y.txt", b"1"), ("x", b"2"), ("z", b"3")];
    let archive = add_zip(&mut app, files.path(), "twins.zip", twins);
    let before = std::fs::read(&archive).unwrap();
    open_archive(&mut app, "twins.zip");
    point(&mut app, "x");
    press(&mut app, KeyCode::F(5));
    tick_until(&mut app, |app| plugin_window(app).is_some());
    assert_eq!(
        plugin_window(&app),
        Some(("dialog", "Extract files".to_string())),
        "F5 is not refused"
    );
    assert_eq!(box_state(&app, MOVE_BOX), Some(false), "F5: triFalse");
    assert!(!box_disabled(&app, MOVE_BOX), "it can be turned on");
    press(&mut app, KeyCode::Esc);
    settled(&mut app);
    point(&mut app, "x");
    press(&mut app, KeyCode::F(6));
    let told = error_told(&mut app);
    assert!(told.contains("both named \"x\""), "{told}");
    settled(&mut app);
    assert!(plugin_window(&app).is_none(), "no dialog");
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    assert_eq!(std::fs::read(&archive).unwrap(), before, "unchanged");
}

/// The same names in an archive that cannot change (a zip split into volumes: 7-Zip's Split around it,
/// arclite's `updatable` false): F6 is not refused - it deletes nothing anyway, "Delete files from
/// archive" disabled - and extracts.
#[test]
fn f6_on_twins_of_an_archive_that_cannot_change_extracts_only() {
    let (_plugins, files, out, mut app) = app_with_archives("archivemovetwinsdup");
    let twins: &[(&str, &[u8])] = &[
        ("x/y.txt", b"1"),
        ("x", b"2"),
        ("d.txt", b"3"),
        ("d.txt", b"4"),
    ];
    let bytes = zip_of(twins);
    let half = bytes.len() / 2;
    let first = files.path().join("both.zip.001");
    std::fs::write(&first, &bytes[..half]).unwrap();
    std::fs::write(files.path().join("both.zip.002"), &bytes[half..]).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| {
        app.panels[0].reading().is_none() && names(app).contains(&"both.zip.001".to_string())
    });
    let before = std::fs::read(&first).unwrap();
    point(&mut app, "both.zip.001");
    press_with(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
    tick_until(&mut app, |app| in_archive(app, "/"));
    f6_dialog(&mut app, "x");
    assert!(box_disabled(&app, MOVE_BOX), "arclite's triUndef");
    assert_eq!(box_state(&app, MOVE_BOX), None, "[?]");
    press(&mut app, KeyCode::Enter);
    settled(&mut app);
    tick_until(&mut app, |_| out.path().join("x").exists());
    assert!(error_lines(&app).is_none(), "nothing refused");
    assert_eq!(std::fs::read(&first).unwrap(), before, "unchanged");
}

/// A folder made that is not waited for any more (Esc let it go, the plugin started anew): its plugin's
/// panels read again all the same, the others not; its failure said.
#[test]
fn a_folder_made_not_waited_for_reads_the_plugins_panels_again() {
    let name = "archivemadegone";
    let (_plugins, _files, out, mut app) = app_with_archives(name);
    open_archive(&mut app, "a.zip");
    let stale = |app: &mut App| {
        let panel = &mut app.panels[0];
        panel.cursor = 0;
        panel.entries.retain(|entry| entry.name != "top.txt");
    };
    let read_again =
        |app: &App| app.panels[0].is_reading() || names(app).contains(&"top.txt".to_string());
    let right_before: Vec<String> = (app.panels[1].entries.iter())
        .map(|entry| entry.name.clone())
        .collect();
    // On the disk now, seen only by a reading of the right panel.
    std::fs::write(out.path().join("late.txt"), "late").unwrap();
    stale(&mut app);
    assert!(!read_again(&app));
    let made = |result: Result<Option<String>, String>| Event::Made {
        plugin: name.to_string(),
        serial: u64::MAX,
        result,
    };
    app.plugin_made(made(Ok(Some("new".to_string()))));
    assert!(read_again(&app), "the plugin's panel read again");
    assert!(app.overlay.is_none(), "nothing to tell");
    let right_now: Vec<String> = (app.panels[1].entries.iter())
        .map(|entry| entry.name.clone())
        .collect();
    assert!(!app.panels[1].is_reading(), "a disk's panel not read again");
    assert_eq!(right_now, right_before);
    tick_until(&mut app, |app| {
        app.panels[0].reading().is_none() && names(app).contains(&"top.txt".to_string())
    });
    stale(&mut app);
    app.plugin_made(made(Err("no room".to_string())));
    assert!(read_again(&app), "read again on a failure too");
    assert_eq!(error_lines(&app), Some(vec!["no room".to_string()]));
}
