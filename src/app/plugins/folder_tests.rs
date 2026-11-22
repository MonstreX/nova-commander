//! A plugin's panel and the folder on a disk under it (FAR's `GetCurDir` and the plugin's `CurDir`):
//! commands run in the folder the panel was opened from, the drive menu opens at that folder's drive, and
//! the command line, the dialogs' lines and the clipboard take the plugin's own path and names.

use super::*;
use crate::app::clipboard::Memory;

/// The probe's panel opened on the right, active, and its folder `docs` entered with the cursor on
/// `a.txt`; the folder the panel came from.
fn in_docs() -> (tempfile::TempDir, App, VfsPath) {
    let (temp, mut app) = app_with("folderone", &[("disks.txt", "Panel one")]);
    let came_from = app.panels[1].path.clone();
    app.active = Side::Right;
    app.open_disk_for_test(0, 0, Side::Right.index());
    let root = VfsPath::in_session("folderone", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Right, &root));
    enter(&mut app, Side::Right, "docs");
    let docs = VfsPath::in_session("folderone", "one", "/docs");
    tick_until(&mut app, |app| shows(app, Side::Right, &docs));
    let panel = &mut app.panels[1];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "a.txt")
        .unwrap();
    (temp, app, came_from)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn on_parent(app: &mut App) {
    let panel = &mut app.panels[1];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.is_parent)
        .unwrap();
}

#[test]
fn commands_and_the_info_panel_take_the_folder_the_panel_was_opened_from() {
    let (_temp, app, came_from) = in_docs();
    assert_eq!(app.active_folder(), came_from.as_local().unwrap());
    assert_eq!(app.panels[1].local_folder(), came_from.as_local());
    assert_eq!(app.panels[1].folder_text(), "/docs/");
}

#[test]
fn the_drive_menu_opens_at_the_drive_of_the_folder_under_the_panel() {
    let (_temp, mut app, came_from) = in_docs();
    app.open_drive_menu(Side::Right);
    tick_until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    let Some(Overlay::DriveMenu(menu)) = &app.overlay else {
        unreachable!()
    };
    let drive = &menu.drives[menu.selected].root;
    let folder = came_from.to_string().to_uppercase();
    assert!(
        folder.starts_with(&drive.trim_end_matches('\\').to_uppercase()),
        "{drive} for {folder}"
    );
}

#[test]
fn the_command_line_takes_the_plugins_own_path_and_names() {
    let (_temp, mut app, _) = in_docs();
    let ctrl = KeyModifiers::CONTROL;
    press(&mut app, KeyCode::Enter, ctrl);
    press(&mut app, KeyCode::Char('f'), ctrl);
    press(&mut app, KeyCode::Char(']'), ctrl);
    assert_eq!(app.command_line.text(), "a.txt /docs/a.txt /docs/");
    app.command_line.clear();
    // On "..": the plugin's folder, for Ctrl+Enter and Ctrl+F alike (FAR turns the one into the other).
    on_parent(&mut app);
    press(&mut app, KeyCode::Enter, ctrl);
    press(&mut app, KeyCode::Char('f'), ctrl | KeyModifiers::ALT);
    assert_eq!(app.command_line.text(), "/docs/ /docs/ ");
}

#[test]
fn the_clipboard_takes_the_plugins_names_and_full_names() {
    let (_temp, mut app, _) = in_docs();
    let memory = Memory::default();
    app.clipboard = Box::new(memory.clone());
    press(&mut app, KeyCode::Insert, KeyModifiers::CONTROL);
    assert_eq!(memory.contents().text.as_deref(), Some("a.txt"));
    press(
        &mut app,
        KeyCode::Insert,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    );
    assert_eq!(memory.contents().text.as_deref(), Some("/docs/a.txt"));
    on_parent(&mut app);
    press(&mut app, KeyCode::Insert, KeyModifiers::CONTROL);
    assert_eq!(
        memory.contents().text.as_deref(),
        Some("docs"),
        "the folder's name"
    );
    press(
        &mut app,
        KeyCode::Insert,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    );
    assert_eq!(
        memory.contents().text.as_deref(),
        Some("/docs/.."),
        "FAR's `CurDir` and the name"
    );
    app.panels[1]
        .selected
        .extend(["a.txt".to_string(), "deep".to_string()]);
    press(
        &mut app,
        KeyCode::Insert,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    );
    let text = memory.contents().text.unwrap();
    let mut names: Vec<&str> = text.split("\r\n").collect();
    names.sort_unstable();
    assert_eq!(names, ["/docs/a.txt", "/docs/deep"]);
}
