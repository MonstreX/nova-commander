//! FAR's first plugins (P6) in the program's panels, their libraries as they are shipped: Same Folder
//! from F11 and from a side's drive menu, Case conversion on the file under the cursor.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::plugin_host::tests::install_shipped;
use crate::vfs::VfsPath;

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The plugin of `library` installed as `name` with its `settings`, the program in `folder`.
fn app_with(
    plugins: &std::path::Path,
    library: &str,
    name: &str,
    settings: &[(&str, serde_json::Value)],
    folder: &std::path::Path,
) -> App {
    install_shipped(plugins, library, name);
    let mut app = App::new(folder, 100, 30, &crate::settings::Settings::default());
    let settings: serde_json::Map<String, serde_json::Value> = (settings.iter())
        .map(|(key, value)| (format!("plugins.{name}.settings.{key}"), value.clone()))
        .collect();
    let host =
        crate::plugin_host::PluginHost::load(plugins, &plugins.join("Logs"), &settings, &|_| true);
    app.set_plugins(host);
    app
}

/// A folder of the files `names`, kept by the caller.
fn files(names: &[&str]) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    for name in names {
        std::fs::write(folder.path().join(name), name).unwrap();
    }
    folder
}

fn cursor_on(app: &mut App, side: Side, name: &str) {
    let panel = &mut app.panels[side.index()];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == name)
        .unwrap();
}

fn under_cursor(app: &App, side: Side) -> &str {
    let panel = &app.panels[side.index()];
    &panel.entries[panel.cursor].name
}

#[test]
fn same_folder_brings_the_passive_panel_to_the_active_ones_item_at_its_top() {
    let plugins = tempfile::tempdir().unwrap();
    let names: Vec<String> = (0..40).map(|at| format!("f{at:02}.txt")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let source = files(&names);
    let other = files(&["x.txt"]);
    let mut app = app_with(plugins.path(), "samefolder", "sameone", &[], other.path());
    app.active = Side::Left;
    let folder = VfsPath::local(source.path());
    app.panels[0].change_dir(&folder, None).unwrap();
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    cursor_on(&mut app, Side::Left, "f05.txt");
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| {
        let panel = &app.panels[1];
        panel.path == folder && !panel.is_reading() && under_cursor(app, Side::Right) == "f05.txt"
    });
    assert_eq!(
        app.panels[1].offset, app.panels[1].cursor,
        "made the top line"
    );
    assert_eq!(app.active, Side::Left, "the active panel stays");
}

#[test]
fn same_folder_from_a_sides_drive_menu_brings_that_side_to_the_other_ones_folder() {
    let plugins = tempfile::tempdir().unwrap();
    let source = files(&["a.txt", "b.txt"]);
    let other = files(&["x.txt"]);
    let settings = [("Add2DisksMenu", serde_json::Value::from("1"))];
    let mut app = app_with(
        plugins.path(),
        "samefolder",
        "sametwo",
        &settings,
        other.path(),
    );
    assert_eq!(app.plugins.host.plugins[0].disks, ["Same Folder"]);
    app.active = Side::Left;
    let folder = VfsPath::local(source.path());
    app.panels[0].change_dir(&folder, None).unwrap();
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    cursor_on(&mut app, Side::Left, "b.txt");
    app.open_disk_for_test(0, 0, Side::Right.index());
    tick_until(&mut app, |app| {
        app.panels[1].path == folder
            && !app.panels[1].is_reading()
            && under_cursor(app, Side::Right) == "b.txt"
    });
    // The left side's menu with the left panel active: the left one to the right one's folder.
    let third = files(&["c.txt"]);
    let elsewhere = VfsPath::local(third.path());
    app.panels[0].change_dir(&elsewhere, None).unwrap();
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    assert_eq!(app.active, Side::Left);
    app.open_disk_for_test(0, 0, Side::Left.index());
    tick_until(&mut app, |app| {
        app.panels[0].path == folder
            && !app.panels[0].is_reading()
            && under_cursor(app, Side::Left) == "b.txt"
    });
}

#[test]
fn case_conversion_renames_the_file_under_the_cursor_and_reads_the_panel_again() {
    let plugins = tempfile::tempdir().unwrap();
    let folder = files(&["READ ME.TXT", "Keep.Txt"]);
    let mut app = app_with(plugins.path(), "filecase", "caseone", &[], folder.path());
    app.active = Side::Left;
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    cursor_on(&mut app, Side::Left, "READ ME.TXT");
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| app.plugins.dialog.is_some());
    // Enter: the default button, OK, the dialog as the settings had it (lower case both).
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        (app.panels[0].entries.iter()).any(|entry| entry.name == "read me.txt")
    });
    let mut names: Vec<String> = (std::fs::read_dir(folder.path()).unwrap().flatten())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["Keep.Txt", "read me.txt"],
        "only the one under the cursor"
    );
}

#[test]
fn case_conversion_for_this_run_only_keeps_no_settings_and_a_run_keeps_them() {
    let plugins = tempfile::tempdir().unwrap();
    let folder = files(&["ONE.TXT", "TWO.TXT"]);
    let mut app = app_with(plugins.path(), "filecase", "casetwo", &[], folder.path());
    app.active = Side::Left;
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    let kept = |app: &App| {
        (app.plugins.kept_settings.keys()).any(|key| key.starts_with("plugins.casetwo.settings."))
    };
    for (name, this_run) in [("ONE.TXT", true), ("TWO.TXT", false)] {
        cursor_on(&mut app, Side::Left, name);
        app.run_plugin_command(&[(0, 0)], 0);
        tick_until(&mut app, |app| app.plugins.dialog.is_some());
        if this_run {
            app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::ALT));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let renamed = name.to_lowercase();
        tick_until(&mut app, |app| {
            (app.panels[0].entries.iter()).any(|entry| entry.name == renamed)
        });
        assert_eq!(kept(&app), !this_run, "{name}");
    }
    assert!(
        app.panels[0].selected.is_empty(),
        "read again without its selection"
    );
}
