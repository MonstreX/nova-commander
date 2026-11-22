//! Options > Plugins configuration and F11's Shift+F9 on the probe (FAR's `PluginManager::Configure`,
//! `ConfigureCurrent`, `CommandsMenu`): the menu as F11's (sorted, wrapping, automatic hotkeys, "Shift+F1,
//! F4, F3", F4 its own hotkeys), Enter running a configuration on the plugin's thread with the menu back
//! when it ends, the values the plugin sets kept in the settings, its items read again when it changed
//! them; F11's Shift+F9 configuring the item's plugin and F11 back; a failure leaving no menu.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::super::fileops::tests::app_in;
use super::super::super::menu::{MENUS, MenuItem};
use super::super::super::{App, Command, Dialog, Overlay};
use super::HotkeyMenu;
use crate::plugin_host::tests::{install_probe, load as load_plugins};

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

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The program's plugin menu up: whether it is the configuration's, its lines and the cursor.
fn program_menu(app: &App) -> Option<(bool, Vec<String>, usize)> {
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        return None;
    };
    menu.commands.as_ref()?;
    let lines = menu.lines.iter().map(|line| line.text.clone()).collect();
    Some((menu.config, lines, menu.selected))
}

#[test]
fn the_options_menu_has_it_after_the_interface_settings_as_far() {
    let options = MENUS.iter().find(|menu| menu.title == "&Options").unwrap();
    let labels: Vec<&str> = (options.items.iter())
        .map(|item| match item {
            MenuItem::Item { label, .. } => *label,
            _ => "",
        })
        .collect();
    let at = labels
        .iter()
        .position(|label| *label == "Pl&ugins configuration");
    let interface = labels
        .iter()
        .position(|label| *label == "&Interface settings");
    assert_eq!(at, interface.map(|at| at + 1), "{labels:?}");
    assert_eq!(
        options.items[at.unwrap()].command(),
        Some(Command::PluginsConfig)
    );
}

#[test]
fn it_lists_every_configuration_sorted_with_fars_keys() {
    let (_temp, mut app) = app_with("cfgone", &[("configs.txt", "Zeta\nAlpha\n")]);
    app.execute(Command::PluginsConfig);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("Plugins configuration");
    };
    assert_eq!(menu.title, "Plugins configuration");
    assert_eq!(menu.bottom, "Shift+F1, F4, F3");
    let (config, lines, selected) = program_menu(&app).unwrap();
    assert!(config);
    assert_eq!(lines, ["&Alpha", "&Zeta"], "sorted, automatic hotkeys");
    assert_eq!(selected, 0);
    // FAR's `VMENU_WRAPMODE`.
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(program_menu(&app).unwrap().2, 1);
    // F4: its own hotkey, under the configuration menu's key.
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('q'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let (config, lines, selected) = program_menu(&app).expect("the menu back");
    assert!(config);
    assert_eq!(lines[1], "&q  Zeta", "FAR's AddHotkey");
    assert_eq!(selected, 1);
    assert_eq!(
        app.plugins
            .kept_settings
            .get("plugins.cfgone.hotkeys.config.Zeta"),
        Some(&serde_json::json!("q"))
    );
    assert_eq!(
        app.plugin_hotkey(0, HotkeyMenu::Commands, "Zeta"),
        None,
        "F11's own hotkeys apart"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
}

#[test]
fn enter_configures_keeps_the_values_and_the_menu_comes_back() {
    let (_temp, mut app) = app_with("cfgtwo", &[("configs.txt", "Options\nUnchanged\n")]);
    app.execute(Command::PluginsConfig);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    tick_until(&mut app, |app| {
        app.plugins.host.plugins[0]
            .commands
            .last()
            .map(String::as_str)
            == Some("Configured 1")
    });
    assert_eq!(
        app.plugins
            .kept_settings
            .get("plugins.cfgtwo.settings.count"),
        Some(&serde_json::json!("1"))
    );
    assert_eq!(
        app.to_settings().other.get("plugins.cfgtwo.settings.label"),
        Some(&serde_json::json!("Options")),
        "saved with the settings"
    );
    let (config, _, selected) = program_menu(&app).expect("FAR's loop: the menu back");
    assert!(config);
    assert_eq!(selected, 0);
    // A configuration that changed nothing: the menu back at its line, the items as they were.
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none(), "while it runs");
    tick_until(&mut app, |app| program_menu(app).is_some());
    assert_eq!(program_menu(&app).unwrap().2, 1);
    assert_eq!(
        app.plugins.host.plugins[0]
            .commands
            .last()
            .map(String::as_str),
        Some("Configured 1")
    );
}

#[test]
fn f11s_shift_f9_configures_the_items_plugin_and_f11_comes_back() {
    let (_temp, mut app) = app_with("cfgthree", &[("configs.txt", "Options\n")]);
    press(&mut app, KeyCode::F(11), KeyModifiers::NONE);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(9), KeyModifiers::SHIFT);
    assert!(app.overlay.is_none(), "while it runs");
    tick_until(&mut app, |app| program_menu(app).is_some());
    let (config, lines, selected) = program_menu(&app).unwrap();
    assert!(!config, "F11 back");
    assert_eq!(selected, 2, "at its line");
    assert!(
        lines.iter().any(|line| line.ends_with("Configured 1")),
        "its items read again: {lines:?}"
    );
}

#[test]
fn f11s_shift_f9_on_a_plugin_without_configuration_keeps_f11() {
    let (_temp, mut app) = app_with("cfgfour", &[]);
    press(&mut app, KeyCode::F(11), KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(9), KeyModifiers::SHIFT);
    let (config, _, selected) = program_menu(&app).expect("F11 at once");
    assert!(!config);
    assert_eq!(selected, 1);
    assert!(app.plugins.configuring.is_none(), "nothing sent");
}

#[test]
fn a_configuration_that_fails_shows_why_and_no_menu_after() {
    let (_temp, mut app) = app_with("cfgfive", &[("configs.txt", "Panic\n")]);
    app.execute(Command::PluginsConfig);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    tick_until(&mut app, |app| {
        matches!(&app.overlay, Some(Overlay::Dialog(_)))
    });
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("the failure");
    };
    assert_eq!(
        lines,
        &["The plugin failed: the probe's configuration panics on purpose"]
    );
    assert!(app.plugins.configuring.is_none());
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.tick();
    assert!(app.overlay.is_none(), "the menu does not come back");
}

#[test]
fn a_configuration_ended_under_another_window_leaves_it() {
    let (_temp, mut app) = app_with("cfgsix", &[("configs.txt", "Options\n")]);
    app.execute(Command::PluginsConfig);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    // The user opened something else meanwhile: it stays.
    app.execute(Command::OpenMenu);
    tick_until(&mut app, |app| app.plugins.configuring.is_none());
    assert!(matches!(&app.overlay, Some(Overlay::Menu(_))));
}

#[test]
fn another_plugins_configuration_ending_leaves_the_one_waited_for() {
    let (_temp, mut app) = app_with("cfgseven", &[("configs.txt", "Options\n")]);
    app.plugins.configuring = Some(("cfgseven".to_string(), HotkeyMenu::Config, 0));
    app.plugins
        .host
        .send_event(crate::plugin_host::Event::Configured {
            plugin: "other".to_string(),
            result: Ok(None),
            serial: 0,
        });
    app.tick();
    assert!(app.overlay.is_none(), "no menu for another plugin's end");
    assert!(app.plugins.configuring.is_some(), "still waited for");
    app.plugins
        .host
        .send_event(crate::plugin_host::Event::Configured {
            plugin: "cfgseven".to_string(),
            result: Ok(None),
            serial: 0,
        });
    app.tick();
    assert!(program_menu(&app).is_some_and(|(config, ..)| config));
}

#[test]
fn a_failure_of_another_call_leaves_the_configuration_waited_for() {
    let (_temp, mut app) = app_with(
        "cfgeight",
        &[(
            "configs.txt",
            "Options
",
        )],
    );
    app.plugins.configuring = Some(("cfgeight".to_string(), HotkeyMenu::Config, 0));
    // A panel key's failure, say, queued before the configuration ended.
    app.plugins
        .host
        .send_event(crate::plugin_host::Event::Failed {
            plugin: "cfgeight".to_string(),
            error: "an older call".to_string(),
            serial: None,
        });
    app.tick();
    assert!(matches!(&app.overlay, Some(Overlay::Dialog(_))));
    assert!(app.plugins.configuring.is_some(), "not the configuration's");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.plugins
        .host
        .send_event(crate::plugin_host::Event::Configured {
            plugin: "cfgeight".to_string(),
            result: Ok(None),
            serial: 0,
        });
    app.tick();
    assert!(program_menu(&app).is_some_and(|(config, ..)| config));
}

#[test]
fn the_menu_comes_back_over_the_configurations_window_without_buttons() {
    let (_temp, mut app) = app_with(
        "cfgnine",
        &[(
            "configs.txt",
            "Busy
",
        )],
    );
    app.execute(Command::PluginsConfig);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    tick_until(&mut app, |app| app.plugins.busy.is_some());
    assert!(app.plugin_busy_shown());
    tick_until(&mut app, |app| program_menu(app).is_some());
    assert!(app.plugins.busy.is_none(), "the window gave way");
    // The call's end (its busy window's `Done`) leaves the menu up.
    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(program_menu(&app).is_some_and(|(config, ..)| config));
}

#[test]
fn a_plugins_own_setting_named_like_a_hotkey_is_no_hotkey() {
    let (_temp, mut app) = app_with("cfgten", &[]);
    let own = "plugins.cfgten.settings.x.hotkeys.commands.Fail";
    app.plugins
        .kept_settings
        .insert(own.to_string(), "z".into());
    press(&mut app, KeyCode::F(11), KeyModifiers::NONE);
    let (_, lines, _) = program_menu(&app).unwrap();
    assert_eq!(
        lines,
        ["&Fail", "&Panic", "&Say hello"],
        "the automatic layout"
    );
    let given = "plugins.cfgten.hotkeys.commands.Fail";
    app.plugins
        .kept_settings
        .insert(given.to_string(), "z".into());
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(11), KeyModifiers::NONE);
    let (_, lines, _) = program_menu(&app).unwrap();
    assert_eq!(lines[0], "&z  Fail", "a hotkey given");
}

#[test]
fn f11s_alt_shift_f9_opens_the_configurations_and_f11_comes_back_after() {
    let (_temp, mut app) = app_with(
        "cfgeleven",
        &[(
            "configs.txt",
            "Options
",
        )],
    );
    press(&mut app, KeyCode::F(11), KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(
        &mut app,
        KeyCode::F(9),
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    );
    let (config, lines, _) = program_menu(&app).expect("Plugins configuration");
    assert!(config);
    assert_eq!(lines, ["&Options"]);
    // A configuration run from it: the configurations come back, then F11 after them.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    tick_until(&mut app, |app| program_menu(app).is_some());
    assert!(program_menu(&app).unwrap().0, "the configurations back");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    let (config, _, selected) = program_menu(&app).expect("F11 back");
    assert!(!config);
    assert_eq!(selected, 1, "at its line");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    // Options' own: Esc closes it, no F11 after.
    app.execute(Command::PluginsConfig);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
}
