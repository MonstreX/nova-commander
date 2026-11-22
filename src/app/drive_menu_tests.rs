//! The drive menu: opened at the panel's drive, keys that move, switch columns and choose, the F9
//! dialog, the folder a drive is entered at, the columns in the settings file.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::dialog::DialogOutcome;
use crate::app::fileops::tests::{app_in, submit_dialog};
use crate::app::waiting::finish_waiting;

fn menu(app: &App) -> &DriveMenu {
    match &app.overlay {
        Some(Overlay::DriveMenu(menu)) => menu,
        _ => panic!("the drive menu"),
    }
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    let Some(Overlay::DriveMenu(menu)) = app.overlay.take() else {
        panic!("the drive menu");
    };
    app.handle_drive_menu_key(menu, KeyEvent::new(code, modifiers));
    finish_waiting(app);
}

fn letter_of(path: &std::path::Path) -> char {
    crate::platform::files::paths::volume_letter(path).unwrap()
}

#[test]
fn the_menu_opens_at_the_panels_drive_and_wraps() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.open_drive_menu(Side::Right);
    finish_waiting(&mut app);
    let opened = menu(&app);
    assert_eq!(opened.side, Side::Right);
    let at = opened.selected;
    assert_eq!(
        opened.drives[at].letter,
        Some(letter_of(temp.path())),
        "the panel's own drive"
    );
    let count = opened.drives.len();
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(menu(&app).selected, 0, "wraps to the top");
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(menu(&app).selected, count - 1);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
}

#[test]
fn ctrl_keys_switch_columns_as_far_does_and_keep_the_line() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    let at = menu(&app).selected;
    let ctrl = KeyModifiers::CONTROL;

    press(&mut app, KeyCode::Char('1'), ctrl);
    assert!(!menu(&app).columns.kind);
    assert_eq!(menu(&app).selected, at);

    // Ctrl+3 from the default label: shell name, nothing, label.
    let label = |app: &App| {
        (
            app.config.drive_columns.label,
            app.config.drive_columns.shell_name,
        )
    };
    assert_eq!(label(&app), (true, false), "the label is shown by default");
    press(&mut app, KeyCode::Char('3'), ctrl);
    assert_eq!(label(&app), (true, true));
    press(&mut app, KeyCode::Char('3'), ctrl);
    assert_eq!(label(&app), (false, false));
    press(&mut app, KeyCode::Char('3'), ctrl);
    assert_eq!(label(&app), (true, false));

    // Ctrl+5 from the default "float": nothing, megabytes, float.
    let size = |app: &App| {
        (
            app.config.drive_columns.size,
            app.config.drive_columns.size_float,
        )
    };
    press(&mut app, KeyCode::Char('5'), ctrl);
    assert_eq!(size(&app), (false, false));
    press(&mut app, KeyCode::Char('5'), ctrl);
    assert_eq!(size(&app), (true, false));
    press(&mut app, KeyCode::Char('5'), ctrl);
    assert_eq!(size(&app), (false, true));

    press(&mut app, KeyCode::Char('h'), ctrl);
    assert!(menu(&app).columns.unmounted);
    press(&mut app, KeyCode::Char('q'), ctrl);
    assert!(app.overlay.is_some(), "other keys leave the menu open");
}

#[test]
fn a_letter_or_enter_goes_to_the_drive() {
    let temp = tempfile::tempdir().unwrap();
    let inner = temp.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let mut app = app_in(temp.path());
    app.panels[1]
        .change_dir(&crate::vfs::VfsPath::local(&inner), None)
        .unwrap();
    let letter = letter_of(temp.path());

    // The left panel is elsewhere, the other one on that drive: the left panel joins it, as FAR's
    // per-drive folder.
    app.panels[0].path = crate::vfs::VfsPath::local(r"\\server\share");
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    let typed = letter.to_ascii_lowercase();
    press(&mut app, KeyCode::Char(typed), KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert_eq!(app.panels[0].path, inner);
}

#[test]
fn a_drive_is_entered_where_the_panels_last_were_on_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let letter = letter_of(temp.path());
    let drive = crate::vfs::local::list_drives(&app.config.drive_columns)
        .into_iter()
        .find(|drive| drive.letter == Some(letter))
        .unwrap();
    let root = PathBuf::from(&drive.root);
    app.remember_drive_folder();
    let key = root_key(&root);
    assert_eq!(app.remembered.drive_folders[&key], temp.path());
    // Neither panel is on the drive now.
    app.panels[0].path = crate::vfs::VfsPath::local(r"\\server\share");
    app.panels[1].path = crate::vfs::VfsPath::local(r"\\server\share");
    assert_eq!(
        app.drive_folder(Side::Left, &drive.root, true).resolve(),
        temp.path()
    );
    app.remembered
        .drive_folders
        .insert(key, temp.path().join("gone"));
    assert_eq!(
        app.drive_folder(Side::Left, &drive.root, true).resolve(),
        root,
        "gone: the root"
    );
}

#[test]
fn f9_sets_the_columns_and_brings_the_menu_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.open_drive_menu(Side::Right);
    finish_waiting(&mut app);
    let at = menu(&app).selected;
    let Some(Overlay::DriveMenu(opened)) = app.overlay.take() else {
        panic!("the drive menu");
    };
    app.handle_drive_menu_key(opened, KeyEvent::from(KeyCode::F(9)));
    {
        let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
            panic!("the options dialog");
        };
        assert_eq!(form.title, "Change Drive Menu Options");
        assert!(form.is_checked(4), "Show size: on for the float style");
        assert!(!form.items[12].disabled, "Detect virtual disks");
        // Show file system on, decimal fraction off: sizes in megabytes.
        for (index, checked) in [(3, true), (5, false)] {
            if let Control::Check { checked: value, .. } = &mut form.items[index].control {
                *value = checked;
            }
        }
    }
    submit_dialog(&mut app, DialogOutcome::Button(OK_BUTTON));
    let columns = app.config.drive_columns;
    assert!(columns.file_system && columns.size && !columns.size_float);
    assert_eq!(menu(&app).selected, at);
    assert_eq!(menu(&app).side, Side::Right);

    // Cancel changes nothing and the menu comes back too.
    app.ask_drive_columns(Side::Right, at);
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert_eq!(app.config.drive_columns, columns);
    assert!(matches!(app.overlay, Some(Overlay::DriveMenu(_))));
}

#[test]
fn the_settings_keep_the_columns() {
    let defaults = crate::settings::Settings::default();
    assert_eq!(
        columns_from_ids(&defaults.drive_menu),
        DriveColumns::default()
    );
    let columns = DriveColumns {
        label: true,
        shell_name: true,
        unmounted: true,
        ..DriveColumns::default()
    };
    assert_eq!(columns_from_ids(&column_ids(&columns)), columns);
    assert_eq!(latin_letter('d'), Some('D'));
}

#[test]
fn choosing_the_panels_own_drive_keeps_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    let inner = temp.path().join("inner");
    let other = temp.path().join("other");
    std::fs::create_dir(&inner).unwrap();
    std::fs::create_dir(&other).unwrap();
    let mut app = app_in(&inner);
    app.panels[1]
        .change_dir(&crate::vfs::VfsPath::local(&other), None)
        .unwrap();
    app.remembered
        .drive_folders
        .insert(root_key(temp.path()), temp.path().to_path_buf());
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.panels[0].path, inner);
}

#[test]
fn a_key_that_is_no_letter_chooses_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.config.drive_columns.unmounted = true;
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    for ch in ['1', ' ', '.'] {
        press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(app.overlay.is_some(), "{ch:?}");
    }
    assert_eq!(app.panels[0].path, temp.path());
}

#[cfg(windows)]
#[test]
fn a_panel_on_its_drive_spelled_in_another_case_keeps_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let letter = letter_of(temp.path());
    let drive = crate::vfs::local::list_drives(&app.config.drive_columns)
        .into_iter()
        .find(|drive| drive.letter == Some(letter))
        .unwrap();
    let lower = PathBuf::from(temp.path().to_string_lossy().to_lowercase());
    app.panels[0].path = crate::vfs::VfsPath::local(&lower);
    assert_eq!(
        app.drive_folder(Side::Left, &drive.root, true).resolve(),
        lower
    );
}

#[cfg(windows)]
#[test]
fn a_drive_is_keyed_by_its_root_case_aside() {
    use std::path::Path;
    assert_eq!(root_key(Path::new("C:\\a\\b")), "c:\\");
    assert_eq!(root_key(Path::new("c:\\")), root_key(Path::new("C:\\")));
    assert_eq!(root_key(Path::new("\\\\srv\\share\\a")), "\\\\srv\\share\\");
    let volume = "\\\\?\\volume{0}\\";
    assert_eq!(root_key(Path::new("\\\\?\\Volume{0}\\a")), volume);
}

#[test]
fn enter_on_the_panels_own_drive_keeps_the_cursor_and_the_selection() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::write(temp.path().join("b.txt"), "b").unwrap();
    let mut app = app_in(temp.path());
    let panel = &mut app.panels[0];
    let b = panel
        .entries
        .iter()
        .position(|entry| entry.name == "b.txt")
        .unwrap();
    panel.cursor = b;
    panel.selected.insert("b.txt".to_string());
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    crate::app::waiting::finish_waiting(&mut app);
    let panel = &app.panels[0];
    assert_eq!(panel.path, temp.path());
    assert_eq!(
        panel.current().map(|entry| entry.name.as_str()),
        Some("b.txt")
    );
    assert!(panel.selected.contains("b.txt"));
}

#[cfg(windows)]
#[test]
fn a_drive_is_entered_at_the_folder_the_active_panel_last_had_on_it_however_it_left() {
    let temp = tempfile::tempdir().unwrap();
    let inner = temp.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let mut app = app_in(&inner);
    app.tick();
    // The panel leaves the drive without the menu (as `cd \\server\share` would).
    app.panels[0].path = crate::vfs::VfsPath::local(r"\\server\share");
    app.panels[1].path = crate::vfs::VfsPath::local(r"\\server\share");
    app.tick();
    let root = format!("{}:\\", letter_of(temp.path()));
    // `D:` on the command line, as FAR's FarChDir("D:").
    app.change_to_drive(&root);
    finish_waiting(&mut app);
    assert_eq!(app.panels[0].path, inner);
}

#[cfg(windows)]
#[test]
fn a_device_that_comes_reads_the_open_menu_again_at_the_same_line() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.open_drive_menu(Side::Right);
    finish_waiting(&mut app);
    let Some(Overlay::DriveMenu(opened)) = &mut app.overlay else {
        panic!("the drive menu");
    };
    let at = opened.drives.len() - 1;
    opened.selected = at;
    // A stale line stands for what the menu showed before the device came.
    opened.drives[at].label = Some("stale".to_string());
    app.devices_changed(&["Q:\\".to_string()]);
    finish_waiting(&mut app);
    let menu = menu(&app);
    assert_eq!((menu.side, menu.selected), (Side::Right, at));
    assert_ne!(
        menu.drives[at].label.as_deref(),
        Some("stale"),
        "read again"
    );
}

#[test]
fn a_device_change_leaves_the_screen_alone_without_the_menu() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.devices_changed(&["Q:\\".to_string()]);
    finish_waiting(&mut app);
    assert!(app.overlay.is_none());
}

#[cfg(windows)]
#[test]
fn a_device_that_comes_while_work_runs_reads_the_menu_after_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.open_drive_menu(Side::Right);
    finish_waiting(&mut app);
    let Some(Overlay::DriveMenu(opened)) = &mut app.overlay else {
        panic!("the drive menu");
    };
    opened.drives[0].label = Some("stale".to_string());
    // Work that outlasts the quick wait keeps the menu on screen.
    crate::app::waiting::quick_as_the_program();
    let (go, wait) = std::sync::mpsc::channel::<()>();
    app.wait_for(Vec::new(), move || {
        let _ = wait.recv();
        Box::new(|_: &mut App| {})
    });
    app.devices_changed(&["Q:\\".to_string()]);
    go.send(()).unwrap();
    finish_waiting(&mut app);
    assert_eq!(
        menu(&app).drives[0].label.as_deref(),
        Some("stale"),
        "not yet"
    );
    app.tick();
    finish_waiting(&mut app);
    assert_ne!(
        menu(&app).drives[0].label.as_deref(),
        Some("stale"),
        "read after the work"
    );
}

#[test]
fn a_panel_on_a_drive_that_came_or_left_reads_its_folder_again() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    std::fs::write(temp.path().join("new.txt"), "n").unwrap();
    let listed = |app: &App| {
        app.panels[0]
            .entries
            .iter()
            .any(|entry| entry.name == "new.txt")
    };
    let other = if cfg!(windows) { "Q:\\" } else { "/elsewhere" };
    app.devices_changed(&[other.to_string()]);
    assert!(!listed(&app), "another drive: not read again");
    let own = temp
        .path()
        .ancestors()
        .last()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    app.devices_changed(&[own]);
    assert!(listed(&app));
}

#[test]
fn esc_while_f4_reads_a_roots_properties_brings_the_menu_back() {
    // ADR-0019: the menu under the wait (review of 19-5).
    use crate::app::waiting::{hold_next_work, shows_stopped};
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    let Some(Overlay::DriveMenu(opened)) = app.overlay.take() else {
        panic!("the drive menu");
    };
    let release = hold_next_work();
    app.handle_drive_menu_key(opened, KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
    assert!(app.is_waiting());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(shows_stopped(&app));
    drop(release);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        matches!(app.overlay, Some(Overlay::DriveMenu(_))),
        "the menu back"
    );
}

#[test]
fn a_drive_folder_that_fails_after_the_panel_went_on_is_said_under_change_drive() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    // A slow folder: the panel goes on before its reading ends.
    app.panels[0].set_read_wait(std::time::Duration::ZERO);
    let gone = VfsPath::local(temp.path().join("gone"));
    app.enter_drive_folder(Side::Left, &gone, "Change drive");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.overlay.is_none() {
        assert!(std::time::Instant::now() < deadline, "no failure came");
        app.tick();
        std::thread::yield_now();
    }
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, error: true, .. })) if title == "Change drive")
    );
}

#[test]
fn ctrl_page_up_closes_the_menu_when_it_changes_drives() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    assert!(app.overlay.is_some(), "PgUp: the first line");
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert!(app.overlay.is_none(), "FAR's `ChDisk->Close(-1)`");
    app.prefs.ctrl_pg_up_changes_drive = 0;
    app.open_drive_menu(Side::Left);
    finish_waiting(&mut app);
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert!(app.overlay.is_some(), "off: the first line, as PgUp");
}

#[test]
fn f9_disables_the_shell_name_and_the_fraction_while_their_boxes_are_off() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.config.drive_columns.label = false;
    app.ask_drive_columns(Side::Left, 0);
    let form = |app: &mut App| -> Form {
        match &app.overlay {
            Some(Overlay::Dialog(Dialog::Form { form, .. })) => form.clone(),
            _ => panic!("the options dialog"),
        }
    };
    let shown = form(&mut app);
    assert!(
        shown.items[LABEL + 1].disabled,
        "Use shell name: the label is off"
    );
    assert!(
        !shown.items[SIZE + 1].disabled,
        "decimal fraction: the size is on"
    );
    // Show &label by its hotkey, Show &size by Space on it.
    app.handle_key(KeyEvent::from(KeyCode::Char('l')));
    assert!(!form(&mut app).items[LABEL + 1].disabled);
    if let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay {
        form.focus_item(SIZE);
    }
    app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    let shown = form(&mut app);
    assert!(!shown.is_checked(SIZE) && shown.items[SIZE + 1].disabled);
}
