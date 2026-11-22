//! Options > Plugins on the probe: a plugin unloaded with its panel shown (the panel back on the folder it
//! came from, the choice kept) and loaded again, Ctrl+R in a hanging call's window starting it anew.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::plugin_host::tests::{install_probe, load};
use crate::vfs::VfsPath;

fn app_with(name: &str, markers: &[(&str, &str)]) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    let place = install_probe(temp.path(), name, &manifest);
    for (marker, text) in markers {
        std::fs::write(place.join(marker), text).unwrap();
    }
    let mut app = app_in(temp.path());
    app.set_plugins(load(temp.path(), &|_| true));
    (temp, app)
}

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn list(app: &App) -> Vec<String> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::List { title, items, .. })) if title == "Plugins" => {
            items.clone()
        }
        _ => panic!("no plugins' list up"),
    }
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

#[test]
fn a_plugin_unloaded_takes_its_panel_back_and_is_loaded_again_the_choice_kept() {
    let (_temp, mut app) = app_with("managerone", &[("disks.txt", "Panel one")]);
    let came_from = app.panels[0].path.clone();
    app.active = Side::Left;
    app.open_disk_for_test(0, 0, Side::Left.index());
    let panel = VfsPath::in_session("managerone", "one", "/");
    tick_until(&mut app, |app| app.panels[0].path == panel);
    app.open_plugin_manager(0);
    assert_eq!(list(&app), ["managerone   [  loaded  ]"]);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(
        list(&app),
        ["managerone   [ disabled ]"],
        "turned off: kept so"
    );
    assert!(app.plugins.host.plugins.is_empty());
    assert_eq!(app.panels[0].path, came_from, "back where it came from");
    let key = "plugins.managerone.enabled";
    assert_eq!(app.plugins.kept_settings.get(key), Some(&false.into()));
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    tick_until(&mut app, |app| !app.plugins.host.plugins.is_empty());
    assert_eq!(list(&app), ["managerone   [  loaded  ]"], "the list told");
    assert_eq!(
        app.plugins.kept_settings.get(key),
        None,
        "loaded at the next start"
    );
}

#[test]
fn ctrl_r_in_a_hanging_calls_window_starts_the_plugin_anew() {
    let (temp, mut app) = app_with("managertwo", &[("commands.txt", "Stubborn")]);
    app.run_plugin_command(&[(0, 0)], 0);
    assert!(app.plugins.key_wait.is_some());
    std::thread::sleep(crate::app::held::QUICK);
    let running = app.plugin_wait_lines().unwrap();
    assert!(
        !running.iter().any(|line| line.starts_with("Ctrl+R")),
        "{running:?}"
    );
    // Esc asks it to stop; it does not answer: then it may be started anew.
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    std::thread::sleep(crate::app::held::STOP_WAIT);
    let lines = app.plugin_wait_lines().unwrap();
    assert!(
        lines.iter().any(|line| line.starts_with("Ctrl+R")),
        "{lines:?}"
    );
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(app.plugins.key_wait.is_none(), "no longer waited for");
    tick_until(&mut app, |app| !app.plugins.host.plugins.is_empty());
    assert!(
        !app.plugins.host.in_call("managertwo"),
        "the new one is free"
    );
    std::fs::write(temp.path().join("managertwo").join("release"), "").unwrap();
}

#[test]
fn a_plugin_finds_unloads_and_loads_a_plugin_by_its_name_or_its_folder() {
    use crate::plugin_host::panel_requests::PluginsAction;
    let (temp, mut app) = app_with("managerthree", &[]);
    let place = temp.path().join("managerthree");
    let folder = place.display().to_string();
    assert!(app.plugins_control("", PluginsAction::Find, "ManagerThree"));
    assert!(
        app.plugins_control("", PluginsAction::Find, &folder),
        "by its folder"
    );
    assert!(!app.plugins_control("", PluginsAction::Find, "none"));
    assert!(
        !app.plugins_control("", PluginsAction::Load, "managerthree"),
        "loaded already"
    );
    assert!(app.plugins_control("", PluginsAction::Unload, &folder));
    assert!(!app.plugins_control("", PluginsAction::Find, "managerthree"));
    let manifest = place.join("plugin.json").display().to_string();
    assert!(
        app.plugins_control("", PluginsAction::Load, &manifest),
        "by its manifest"
    );
    tick_until(&mut app, |app| !app.plugins.host.plugins.is_empty());
    assert!(app.plugins_control("", PluginsAction::Unload, "managerthree"));
    assert!(
        app.plugins_control("", PluginsAction::Load, "managerthree"),
        "by its name"
    );
    tick_until(&mut app, |app| !app.plugins.host.plugins.is_empty());
}

#[test]
fn a_plugin_unloading_itself_is_let_go_as_its_call_ends() {
    let (temp, mut app) = app_with("managerfour", &[("commands.txt", "Unload managerfour")]);
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| app.plugins.host.plugins.is_empty());
    let log = temp.path().join("Logs").join("managerfour.log");
    let log = std::fs::read_to_string(log).unwrap();
    assert!(
        log.contains("unload managerfour true cancelled false"),
        "its call goes on to its end: {log}"
    );
    tick_until(&mut app, |_| {
        !(std::fs::read_dir(temp.path().join("managerfour")).unwrap())
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains(".vc-"))
    });
}

#[test]
fn a_plugin_unloads_another_whose_requests_are_dropped() {
    let (temp, mut app) = app_with("managerfive", &[("commands.txt", "Unload managersix")]);
    let manifest = r#"{"name": "managersix", "version": "1", "api": 11, "library": "probe"}"#;
    install_probe(temp.path(), "managersix", manifest);
    app.set_plugins(load(temp.path(), &|_| true));
    assert_eq!(app.plugins.host.plugins.len(), 2);
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| app.plugins.host.plugins.len() == 1);
    assert!(app.plugins.host.is_loaded("managerfive"));
    let log = temp.path().join("Logs").join("managerfive.log");
    // The plugin writes its log as its call ends, which may come after the other one is let go.
    tick_until(&mut app, |_| {
        std::fs::read_to_string(&log).is_ok_and(|log| log.contains("unload managersix"))
    });
    let log = std::fs::read_to_string(log).unwrap();
    assert!(
        log.contains("unload managersix true cancelled false"),
        "{log}"
    );
    // One's request taken out with another's that lets it go is not answered: its call hears nothing.
    use super::super::panel_control::PanelAsk;
    use crate::plugin_host::panel_requests::{PanelRequest, PluginsAction, Which};
    let (_nine, mut app) = app_with("managernine", &[]);
    let (reply, unloaded) = std::sync::mpsc::channel();
    let unload = PanelRequest::Plugins(PluginsAction::Unload, "managernine".to_string());
    let (state, waits) = std::sync::mpsc::channel();
    app.plugins.panel_asks = vec![
        PanelAsk::for_test("managerx", unload, reply),
        PanelAsk::for_test("managernine", PanelRequest::State(Which::Active), state),
    ];
    app.answer_panel_asks();
    assert!(unloaded.try_recv().is_ok(), "answered");
    assert!(app.plugins.host.plugins.is_empty());
    assert!(waits.recv().is_err(), "dropped");
    assert!(app.plugins.panel_asks.is_empty());
}

#[test]
fn a_bare_name_is_a_plugins_name_before_a_folder_of_the_working_folder() {
    let (_temp, mut app) = app_with("managerseven", &[]);
    use crate::plugin_host::panel_requests::PluginsAction;
    assert!(
        !app.plugins_control("", PluginsAction::Find, "src"),
        "a folder of the working folder is not taken for a path"
    );
    assert!(app.plugins_control("", PluginsAction::Find, "managerseven"));
}

#[test]
fn the_plugins_commands_menu_closes_as_a_plugin_is_let_go() {
    let (_temp, mut app) = app_with("managereight", &[("commands.txt", "One")]);
    app.open_plugin_commands();
    assert!(matches!(app.overlay, Some(Overlay::PluginMenu(_))));
    assert!(app.let_plugin_go("managereight").is_some());
    assert!(
        app.overlay.is_none(),
        "its lines named the plugins by their places"
    );
}

#[test]
fn the_names_stand_left_and_the_states_right_in_one_column() {
    let row = |name: &str, state| PluginRow {
        name: name.to_string(),
        state,
    };
    let rows = [
        row("commands", PluginState::Loaded),
        row("emenu", PluginState::Starting),
        row("filecase", PluginState::Disabled),
        row("x", PluginState::NotLoaded),
    ];
    assert_eq!(
        plugin_lines(&rows),
        [
            "commands   [  loaded  ]",
            "emenu      [ starting ]",
            "filecase   [ disabled ]",
            "x          [not loaded]",
        ]
    );
}

#[test]
fn a_plugin_that_did_not_start_is_not_loaded_not_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = r#"{"name": "managerold", "version": "1", "api": 1, "library": "probe"}"#;
    install_probe(temp.path(), "managerold", manifest);
    let mut app = app_in(temp.path());
    app.set_plugins(load(temp.path(), &|_| true));
    app.overlay = None;
    app.open_plugin_manager(0);
    assert_eq!(list(&app), ["managerold   [not loaded]"]);
}
