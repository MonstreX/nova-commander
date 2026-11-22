//! A plugin's panel in the program's panels, on the probe: opened by a drive menu item or an F11
//! command where it was chosen, its folders entered and left, ".." at its root back to where the panel
//! came from, its plugin told when no panel shows it, the folder saved being the one it came from.
//! Each test names its probe alone: the providers' list is the program's, shared by the tests.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::super::fileops::tests::app_in;
use super::super::super::waiting::finish_waiting;
use super::super::super::{App, Dialog, Overlay, Side};
use crate::plugin_host::tests::{install_probe, load as load_plugins};
use crate::vfs::VfsPath;

/// The probe installed as plugin `name`, its marker files `markers` (name, text), loaded into an app.
fn app_with(name: &str, markers: &[(&str, &str)]) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    let place = install_probe(temp.path(), name, &manifest);
    for (marker, text) in markers {
        std::fs::write(place.join(marker), text).unwrap();
    }
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    (temp, app)
}

/// The event loop's periodic work until `done` holds, at most 10 s.
fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The panel of `side` showing `path`, read.
fn shows(app: &App, side: Side, path: &VfsPath) -> bool {
    let panel = &app.panels[side.index()];
    panel.path == *path && panel.reading().is_none()
}

fn sorted_names(app: &App, side: Side) -> Vec<String> {
    let mut names: Vec<String> = (app.panels[side.index()].entries.iter())
        .map(|entry| entry.name.clone())
        .collect();
    names.sort();
    names
}

/// The panel of `side` on `name`, entered (Enter's own call).
fn enter(app: &mut App, side: Side, name: &str) {
    let panel = &mut app.panels[side.index()];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == name)
        .unwrap();
    let _ = panel.enter();
}

fn log(temp: &tempfile::TempDir, name: &str) -> String {
    let path = temp.path().join("Logs").join(format!("{name}.log"));
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Every job sent to the plugin so far handled: its command "Fail" (the probe's second) run after
/// them and its failure waited for and put away. A close sent is in its log after this.
fn settle(app: &mut App) {
    assert!(app.overlay.is_none(), "nothing up before");
    // The plugins' pump first: a panel read in the last tick has its close sent by the next one,
    // which would come after the probe's command and its failure.
    app.tick();
    assert!(app.overlay.is_none(), "nothing up after the tick");
    app.run_command_for_test(0, 1, 0);
    tick_until(app, |app| app.overlay.is_some());
    assert!(message(app).1, "the failure");
    app.overlay = None;
}

/// The message up: its lines and whether it is an error.
fn message(app: &App) -> (Vec<String>, bool) {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Message { lines, error, .. })) => (lines.clone(), *error),
        _ => panic!("a message"),
    }
}

#[test]
fn a_drive_menu_item_opens_the_panel_there_and_dot_dot_at_its_root_leads_back() {
    let (temp, mut app) = app_with("panelsone", &[("disks.txt", "Panel one")]);
    let came_from = app.panels[1].path.clone();
    app.open_disk_for_test(0, 0, Side::Right.index());
    let root = VfsPath::in_session("panelsone", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Right, &root));
    let panel = &app.panels[1];
    assert_eq!(panel.look.title.as_deref(), Some("Probe one:/"));
    assert_eq!(panel.return_to(), Some(&came_from));
    assert!(
        app.panels[0].path.as_local().is_some(),
        "the other panel stays"
    );
    assert_eq!(
        sorted_names(&app, Side::Right),
        ["..", ".hidden", "docs", "locked", "readme.txt"]
    );
    enter(&mut app, Side::Right, "docs");
    let docs = VfsPath::in_session("panelsone", "one", "/docs");
    tick_until(&mut app, |app| shows(app, Side::Right, &docs));
    assert_eq!(app.panels[1].look.title.as_deref(), Some("Probe one:/docs"));
    assert_eq!(sorted_names(&app, Side::Right), ["..", "a.txt", "deep"]);
    enter(&mut app, Side::Right, "..");
    tick_until(&mut app, |app| shows(app, Side::Right, &root));
    let panel = &app.panels[1];
    assert_eq!(
        panel.entries[panel.cursor].name, "docs",
        "on the folder left"
    );
    assert_eq!(panel.return_to(), Some(&came_from), "kept inside");
    settle(&mut app);
    assert!(!log(&temp, "panelsone").contains("closed"), "still shown");
    enter(&mut app, Side::Right, "..");
    tick_until(&mut app, |app| shows(app, Side::Right, &came_from));
    assert_eq!(app.panels[1].return_to(), None);
    assert_eq!(app.panels[1].look.title, None);
    settle(&mut app);
    assert_eq!(log(&temp, "panelsone").matches("closed one").count(), 1);
}

#[test]
fn an_f11_command_opens_the_panel_in_the_active_panel() {
    let (_temp, mut app) = app_with("panelstwo", &[("commands.txt", "Panel two")]);
    let side = app.command_side();
    app.run_plugin_command(&[(0, 0)], 0);
    let root = VfsPath::in_session("panelstwo", "two", "/");
    tick_until(&mut app, |app| shows(app, side, &root));
    assert!(app.panels[side.other().index()].path.as_local().is_some());
}

#[test]
fn a_folder_the_plugin_refuses_keeps_the_panel_where_it_is() {
    let (_temp, mut app) = app_with("panelsthree", &[("disks.txt", "Panel three")]);
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("panelsthree", "three", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    // Enter as the user presses it: the refusal shown as a folder that cannot be read is.
    app.active = Side::Left;
    let panel = &mut app.panels[0];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "locked")
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| app.overlay.is_some());
    let (lines, error) = message(&app);
    assert!(error);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("The probe keeps it locked")),
        "{lines:?}"
    );
    assert!(
        shows(&app, Side::Left, &root),
        "the panel stays at the root"
    );
}

#[test]
fn both_panels_on_one_panel_of_a_plugin_close_it_when_the_last_leaves() {
    let (temp, mut app) = app_with("panelsfour", &[("disks.txt", "Panel four")]);
    let starts = [app.panels[0].path.clone(), app.panels[1].path.clone()];
    let root = VfsPath::in_session("panelsfour", "four", "/");
    // One item at a time, as the keys after one wait for its end (ADR-0019).
    app.open_disk_for_test(0, 0, Side::Left.index());
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    app.open_disk_for_test(0, 0, Side::Right.index());
    tick_until(&mut app, |app| {
        shows(app, Side::Left, &root) && shows(app, Side::Right, &root)
    });
    let saved = app.to_settings();
    assert_eq!(
        [saved.left_path, saved.right_path],
        starts.clone().map(|path| path.to_string()),
        "a plugin's panel is saved as the folder it came from"
    );
    enter(&mut app, Side::Left, "..");
    tick_until(&mut app, |app| shows(app, Side::Left, &starts[0]));
    settle(&mut app);
    let told = log(&temp, "panelsfour");
    assert!(!told.contains("closed"), "the right one shows it: {told}");
    enter(&mut app, Side::Right, "..");
    tick_until(&mut app, |app| shows(app, Side::Right, &starts[1]));
    settle(&mut app);
    assert_eq!(
        log(&temp, "panelsfour").matches("closed").count(),
        1,
        "once"
    );
}

#[test]
fn a_panels_name_that_cannot_be_a_path_is_the_items_failure() {
    let (_temp, mut app) = app_with("panelsfive", &[("disks.txt", "Panel a/b")]);
    let before = app.panels[0].path.clone();
    app.open_disk_for_test(0, 0, Side::Left.index());
    tick_until(&mut app, |app| app.overlay.is_some());
    let (lines, error) = message(&app);
    assert!(error);
    assert!(
        lines.iter().any(|line| line.contains("\"a/b\"")),
        "{lines:?}"
    );
    assert_eq!(app.panels[0].path, before);
}

#[test]
fn a_panel_opened_over_another_leads_back_to_the_folder_the_first_came_from() {
    let markers = [("disks.txt", "Panel one\nPanel two")];
    let (temp, mut app) = app_with("panelssix", &markers);
    let came_from = app.panels[1].path.clone();
    let one = VfsPath::in_session("panelssix", "one", "/");
    let two = VfsPath::in_session("panelssix", "two", "/");
    app.open_disk_for_test(0, 0, Side::Right.index());
    tick_until(&mut app, |app| shows(app, Side::Right, &one));
    app.open_disk_for_test(0, 1, Side::Right.index());
    tick_until(&mut app, |app| shows(app, Side::Right, &two));
    assert_eq!(app.panels[1].return_to(), Some(&came_from));
    assert_eq!(app.to_settings().right_path, came_from.to_string());
    settle(&mut app);
    assert!(
        !log(&temp, "panelssix").contains("closed two"),
        "two is shown"
    );
    assert_eq!(
        log(&temp, "panelssix").matches("closed one").count(),
        1,
        "the first closed"
    );
    enter(&mut app, Side::Right, "..");
    tick_until(&mut app, |app| shows(app, Side::Right, &came_from));
    settle(&mut app);
    let told = log(&temp, "panelssix");
    assert_eq!(
        (
            told.matches("closed one").count(),
            told.matches("closed two").count()
        ),
        (1, 1),
        "{told}"
    );
}

#[test]
fn a_panel_whose_root_the_plugin_refuses_is_closed_for_it() {
    let (temp, mut app) = app_with("panelsseven", &[("disks.txt", "Panel refused")]);
    let before = app.panels[0].path.clone();
    app.open_disk_for_test(0, 0, Side::Left.index());
    tick_until(&mut app, |app| app.overlay.is_some());
    let (lines, error) = message(&app);
    assert!(error);
    assert!(
        lines.iter().any(|line| line.contains("refuses")),
        "{lines:?}"
    );
    assert_eq!(app.panels[0].path, before, "the panel stays");
    app.overlay = None;
    settle(&mut app);
    assert_eq!(
        log(&temp, "panelsseven").matches("closed refused").count(),
        1
    );
}

/// The drive menu of `side` up, its first plugin item selected and `key` pressed on it.
fn drive_menu_key(app: &mut App, side: Side, key: KeyCode) {
    app.open_drive_menu(side);
    finish_waiting(app);
    let Some(Overlay::DriveMenu(mut menu)) = app.overlay.take() else {
        panic!("the drive menu");
    };
    assert_eq!(menu.side, side);
    menu.selected = menu.drives.len() + 1;
    app.handle_drive_menu_key(menu, KeyEvent::new(key, KeyModifiers::NONE));
    finish_waiting(app);
}

#[test]
fn the_right_drive_menus_item_opens_the_panel_on_the_right_by_enter_or_its_hotkey() {
    let markers = [("disks.txt", "Panel one\nPanel two")];
    let (_temp, mut app) = app_with("panelseight", &markers);
    // The user's hotkey for it (FAR's drive menu has none by default).
    let setting = "plugins.panelseight.hotkeys.disks.Panel two".to_string();
    app.plugins.kept_settings.insert(setting, "2".into());
    app.active = Side::Left;
    let one = VfsPath::in_session("panelseight", "one", "/");
    drive_menu_key(&mut app, Side::Right, KeyCode::Enter);
    tick_until(&mut app, |app| shows(app, Side::Right, &one));
    assert!(
        app.panels[0].path.as_local().is_some(),
        "the left one stays"
    );
    let two = VfsPath::in_session("panelseight", "two", "/");
    drive_menu_key(&mut app, Side::Right, KeyCode::Char('2'));
    tick_until(&mut app, |app| shows(app, Side::Right, &two));
    assert!(
        app.panels[0].path.as_local().is_some(),
        "the left one stays"
    );
}

#[test]
fn an_f11_command_opens_the_panel_in_the_right_panel_when_it_is_active() {
    let (_temp, mut app) = app_with("panelsnine", &[("commands.txt", "Panel nine")]);
    app.active = Side::Right;
    assert_eq!(app.command_side(), Side::Right);
    app.run_plugin_command(&[(0, 0)], 0);
    let root = VfsPath::in_session("panelsnine", "nine", "/");
    tick_until(&mut app, |app| shows(app, Side::Right, &root));
    assert!(
        app.panels[0].path.as_local().is_some(),
        "the left one stays"
    );
}

#[test]
fn a_panel_without_a_title_shows_its_path() {
    let (_temp, mut app) = app_with("panelsten", &[("disks.txt", "Panel untitled")]);
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("panelsten", "untitled", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    assert_eq!(app.panels[0].look.title, None);
}

#[test]
fn a_panel_both_left_and_never_shown_is_closed_once() {
    let (temp, mut app) = app_with("panelseleven", &[]);
    let one = VfsPath::in_session("panelseleven", "one", "/");
    // Shown at the last tick and opened again in a panel whose reading ended elsewhere.
    app.plugins.panels = vec![one.clone()];
    app.plugins.opening = vec![(one.join("docs"), Side::Left)];
    app.close_left_plugin_panels();
    assert!(app.plugins.panels.is_empty() && app.plugins.opening.is_empty());
    settle(&mut app);
    let told = log(&temp, "panelseleven");
    assert_eq!(told.matches("closed").count(), 1, "{told}");
}

#[test]
fn a_panel_opened_on_one_side_and_shown_on_the_other_is_not_closed() {
    let markers = [("disks.txt", "Panel one")];
    let (temp, mut app) = app_with("panelstwelve", &markers);
    let one = VfsPath::in_session("panelstwelve", "one", "/");
    app.open_disk_for_test(0, 0, Side::Right.index());
    tick_until(&mut app, |app| shows(app, Side::Right, &one));
    // The plugin's folder is in the left panel's: its library's copy made there is read again first.
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    // Its opening on the left ended elsewhere: the right panel still shows it.
    app.plugins.opening = vec![(one.clone(), Side::Left)];
    app.close_left_plugin_panels();
    assert!(app.plugins.opening.is_empty());
    assert_eq!(app.plugins.panels, [one]);
    settle(&mut app);
    let told = log(&temp, "panelstwelve");
    assert!(!told.contains("closed"), "{told}");
}

#[test]
fn an_entry_with_a_real_file_is_read_copied_run_and_deleted_as_that_file() {
    let (temp, mut app) = app_with("panelsreal", &[("disks.txt", "Panel real")]);
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("panelsreal", "real", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    let file = temp.path().join("panelsreal").join("plugin.json");
    let panel = &mut app.panels[0];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "plugin.json")
        .unwrap();
    assert_eq!(
        panel.current_path(),
        Some(file.clone()),
        "viewed, edited, run"
    );
    assert_eq!(panel.targets(), [VfsPath::local(&file)], "copied and moved");
    let entry = panel.entries[panel.cursor].clone();
    assert_eq!(panel.item_path(&entry), VfsPath::local(&file));
    let deleted: Vec<VfsPath> = (panel.delete_items(true).into_iter())
        .map(|item| item.path)
        .collect();
    assert_eq!(
        deleted,
        [VfsPath::local(&file)],
        "deleted as that file (ADR-0016 §14: FAR's)"
    );
    panel.toggle_selection();
    assert_eq!(panel.targets(), [VfsPath::local(&file)], "selected too");
    let provider = crate::vfs::provider_for(&root).unwrap();
    provider
        .remove(&root.join("plugin.json"), crate::vfs::Removal::File)
        .unwrap();
    assert!(file.is_file(), "the file stays on the disk");
    let mut listed = Vec::new();
    let stop = std::sync::atomic::AtomicBool::new(false);
    provider
        .list(&root, &stop, &mut |entry| listed.push(entry.name))
        .unwrap();
    assert!(!listed.contains(&"plugin.json".to_string()), "off the list");
}

/// The probe `name`'s real panel open on the left, active; its folder.
fn real_panel(name: &str) -> (tempfile::TempDir, App, std::path::PathBuf) {
    let (temp, mut app) = app_with(name, &[("disks.txt", "Panel real")]);
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session(name, "real", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    app.active = Side::Left;
    let folder = temp.path().join(name);
    (temp, app, folder)
}

#[test]
fn real_files_selected_go_in_the_panels_order_and_a_name_typed_for_them_is_beside_them() {
    let (_temp, mut app, folder) = real_panel("panelsrealtwo");
    let panel = &mut app.panels[0];
    for name in ["plugin.json", "disks.txt", "gone.txt"] {
        panel.selected.insert(name.to_string());
    }
    let root = VfsPath::in_session("panelsrealtwo", "real", "/");
    let targets = panel.targets();
    assert_eq!(
        targets,
        [
            VfsPath::local(folder.join("disks.txt")),
            VfsPath::local(folder.join("plugin.json")),
            root.join("gone.txt"),
        ],
        "in the panel's order (review R41); one no longer listed after them, by its path here"
    );
    let real = [VfsPath::local(folder.join("disks.txt"))];
    assert_eq!(
        app.operation_base(&real),
        VfsPath::local(&folder),
        "beside the real file"
    );
    assert_eq!(
        app.operation_base(&targets),
        root,
        "not all real: the panel's folder"
    );
    // Ctrl+Shift+C: the real files for Explorer's Paste; one with none on a disk left out.
    let memory = crate::app::clipboard::Memory::default();
    app.clipboard = Box::new(memory.clone());
    app.handle_key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    let files = vec![folder.join("disks.txt"), folder.join("plugin.json")];
    assert_eq!(memory.contents().files, Some((files, false)));
}

#[test]
fn quick_view_and_f8_take_a_real_file_as_its_file() {
    let (temp, mut app, folder) = real_panel("panelsrealthree");
    app.config.delete_settings.to_recycle_bin = false;
    let panel = &mut app.panels[0];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "disks.txt")
        .unwrap();
    assert_eq!(
        app.quick_view_target(Side::Right),
        Some((VfsPath::local(folder.join("disks.txt")), false))
    );
    app.handle_key(KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        !folder.join("disks.txt").exists()
            && !(app.panels[0].entries.iter()).any(|entry| entry.name == "disks.txt")
    });
    settle(&mut app);
    assert!(
        !log(&temp, "panelsrealthree").contains("removed"),
        "deleted as the file on its disk, the plugin not asked"
    );
}

#[test]
fn f5_into_a_plugins_panel_offers_its_format_and_copies_into_it() {
    let (temp, mut app) = app_with("panelsformat", &[("disks.txt", "Panel one")]);
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("x.txt"), "x").unwrap();
    app.open_disk_for_test(0, 0, Side::Right.index());
    let root = VfsPath::in_session("panelsformat", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Right, &root));
    let left = VfsPath::local(files.path());
    app.panels[0].change_dir(&left, None).unwrap();
    app.active = Side::Left;
    let panel = &mut app.panels[0];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "x.txt")
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the copy dialog");
    };
    assert!(
        (form.items.iter()).any(|item| matches!(
            &item.control,
            super::super::super::form::Control::Edit(edit) if edit.text() == "Probe:"
        )),
        "FAR's `<Format>:`"
    );
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |_| {
        log(&temp, "panelsformat").contains("made file one:/x.txt")
    });
}

#[test]
fn a_plugin_without_a_format_is_offered_as_two_colons() {
    let (_temp, mut app) = app_with("panelscolons", &[("disks.txt", "Panel untitled")]);
    app.open_disk_for_test(0, 0, Side::Right.index());
    let root = VfsPath::in_session("panelscolons", "untitled", "/");
    tick_until(&mut app, |app| shows(app, Side::Right, &root));
    app.active = Side::Left;
    let target = app.plugin_target().unwrap();
    assert_eq!(
        (target.text.as_str(), target.dir),
        ("::", root),
        "FAR's `::`"
    );
    assert!(!target.takes_paths);
}

#[path = "modes_tests.rs"]
mod modes;

#[path = "tmp_panel_tests.rs"]
mod tmp_panel;

#[path = "first_set_tests.rs"]
mod first_set;

#[path = "commands_tests.rs"]
mod commands;

#[path = "emenu_tests.rs"]
mod emenu;

#[path = "compare_tests.rs"]
mod compare;

#[path = "archive_tests.rs"]
mod archive_panels;

#[path = "stack_tests.rs"]
mod stack;

#[path = "folder_tests.rs"]
mod folder;

#[path = "mkdir_tests.rs"]
mod mkdir;
