//! Advanced compare (FAR's compare, P6.4) through the program, the plugin's library as it is shipped: the
//! two panels' files compared, the different ones and those without a pair selected; only two file panels.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::plugin_host::tests::install_shipped;
use crate::vfs::VfsPath;

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    for (name, text) in files {
        std::fs::write(folder.path().join(name), text).unwrap();
    }
    folder
}

/// The plugin installed as `name`, the left panel on `left`, the right one on `right`, both read.
fn app_with(
    name: &str,
    left: &std::path::Path,
    right: &std::path::Path,
) -> (tempfile::TempDir, App) {
    app_set(name, left, right, &[])
}

/// As [`app_with`], the plugin's `settings` given.
fn app_set(
    name: &str,
    left: &std::path::Path,
    right: &std::path::Path,
    settings: &[(&str, &str)],
) -> (tempfile::TempDir, App) {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "compare", name);
    let mut app = App::new(left, 100, 30, &crate::settings::Settings::default());
    let host = crate::plugin_host::PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &(settings.iter())
            .map(|(key, value)| {
                let key = format!("plugins.{name}.settings.{key}");
                (key, serde_json::Value::from(*value))
            })
            .collect(),
        &|_| true,
    );
    app.set_plugins(host);
    app.active = Side::Left;
    app.panels[1]
        .change_dir(&VfsPath::local(right), None)
        .unwrap();
    tick_until(&mut app, |app| {
        app.panels.iter().all(|panel| !panel.is_reading())
    });
    (plugins, app)
}

fn selected(app: &App, side: Side) -> Vec<String> {
    let panel = &app.panels[side.index()];
    let mut names: Vec<String> = (panel.entries.iter())
        .filter(|entry| panel.selected.contains(&entry.name))
        .map(|entry| entry.name.clone())
        .collect();
    names.sort();
    names
}

#[test]
fn the_files_that_differ_or_have_no_pair_are_selected_the_same_not() {
    let left = folder(&[("a.txt", "1"), ("b.txt", "22"), ("left.txt", "l")]);
    let right = folder(&[("a.txt", "1"), ("b.txt", "2"), ("right.txt", "r")]);
    let (_plugins, mut app) = app_with("cmpone", left.path(), right.path());
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| app.plugins.dialog.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| !app.plugins.host.in_call("cmpone"));
    tick_until(&mut app, |app| !selected(app, Side::Left).is_empty());
    assert_eq!(selected(&app, Side::Left), ["b.txt", "left.txt"]);
    assert_eq!(selected(&app, Side::Right), ["b.txt", "right.txt"]);
}

#[test]
fn only_two_file_panels_are_compared() {
    let left = folder(&[("a.txt", "1")]);
    let right = folder(&[("a.txt", "2")]);
    let (_plugins, mut app) = app_with("cmptwo", left.path(), right.path());
    app.toggle_info_panel();
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| app.overlay.is_some());
    let Some(Overlay::Dialog(crate::app::dialog::Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no message");
    };
    let shown = format!("{:?}", form.items);
    assert!(shown.contains("Two file panels are required"), "{shown}");
}

#[test]
fn selected_only_on_the_panel_with_a_selection_and_the_items_not_compared_keep_theirs() {
    let left = folder(&[("a.txt", "1"), ("b.txt", "22"), ("c.txt", "c")]);
    std::fs::create_dir(left.path().join("sub")).unwrap();
    let right = folder(&[("a.txt", "1"), ("b.txt", "2"), ("c.txt", "cc")]);
    let settings = [("ProcessSelected", "1")];
    let (_plugins, mut app) = app_set("cmpthree", left.path(), right.path(), &settings);
    for name in ["a.txt", "b.txt", "sub"] {
        app.panels[0].selected.insert(name.to_string());
    }
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| app.plugins.dialog.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| !app.plugins.host.in_call("cmpthree"));
    tick_until(&mut app, |app| selected(app, Side::Right).len() == 2);
    // Left: a and b compared (a the same), c not (left as it was), sub no folder compared (kept).
    assert_eq!(selected(&app, Side::Left), ["b.txt", "sub"]);
    // Right, no selection: all compared, c without a pair on the left's compared side.
    assert_eq!(selected(&app, Side::Right), ["b.txt", "c.txt"]);
}
