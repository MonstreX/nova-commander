//! F11 and what the probe plugin asks, through the program as the user meets it.

use std::time::{Duration, Instant};

#[cfg(windows)]
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::*;
use crate::app::fileops::tests::app_in;
#[cfg(windows)]
use crate::app::form::Control;
use crate::plugin_host::tests::{install_probe, load as load_plugins, plugins_with_probe};

fn app_with_plugins(plugins: &tempfile::TempDir) -> App {
    let mut app = app_in(plugins.path());
    app.set_plugins(load_plugins(plugins.path(), &|_| true));
    app
}

/// Ticks until a dialog is up.
fn dialog(app: &mut App) -> Dialog {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.pump_plugins();
        if let Some(Overlay::Dialog(dialog)) = &app.overlay {
            return dialog.clone();
        }
        assert!(Instant::now() < deadline, "no dialog");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A plugin's message window: its title and lines.
#[cfg(windows)]
fn plugin_message(dialog: &Dialog) -> (String, Vec<String>) {
    let Dialog::Form {
        form,
        action: FormAction::Plugin(PluginForm::Message { .. }),
    } = dialog
    else {
        panic!("a plugin's message: {dialog:?}");
    };
    let lines = (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Label(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    (form.title.clone(), lines)
}

/// F11's menu up: its title and its lines without their hotkeys' marks.
fn commands_menu(app: &App) -> (String, Vec<String>) {
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("Plugin commands");
    };
    let lines = (menu.lines.iter())
        .map(|line| line.text.replace('&', ""))
        .collect();
    (menu.title.clone(), lines)
}

/// F11, the item `label` chosen (other systems load no plugins yet).
#[cfg(windows)]
fn run(app: &mut App, label: &str) {
    app.handle_key(KeyEvent::from(KeyCode::F(11)));
    let (title, items) = commands_menu(app);
    assert_eq!(title, "Plugin commands");
    assert_eq!(items, ["Fail", "Panic", "Say hello"], "sorted");
    let at = items.iter().position(|item| item == label).unwrap();
    for _ in 0..at {
        app.handle_key(KeyEvent::from(KeyCode::Down));
    }
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.overlay.is_none());
}

#[cfg(windows)]
#[test]
fn f11_runs_a_command_and_its_message_waits_for_ok() {
    let temp = plugins_with_probe(|_| {});
    let mut app = app_with_plugins(&temp);
    run(&mut app, "Say hello");
    let (title, lines) = plugin_message(&dialog(&mut app));
    assert_eq!(title, "Probe");
    assert_eq!(lines, ["Hello from the probe", "second line"]);
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.overlay.is_none());
    // OK: the command succeeded, so the next window is the next command's.
    run(&mut app, "Fail");
    let Dialog::Message { lines, .. } = dialog(&mut app) else {
        panic!("the failure");
    };
    assert_eq!(lines, ["The probe fails on purpose"]);
}

#[cfg(windows)]
#[test]
fn esc_on_a_message_and_a_failing_command_are_reported() {
    let temp = plugins_with_probe(|_| {});
    let mut app = app_with_plugins(&temp);
    run(&mut app, "Say hello");
    dialog(&mut app);
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    let Dialog::Message { title, lines, .. } = dialog(&mut app) else {
        panic!("the failure");
    };
    assert_eq!(title, "Probe");
    assert_eq!(lines, ["closed with Esc"]);
    app.overlay = None;
    run(&mut app, "Panic");
    let Dialog::Message { lines, .. } = dialog(&mut app) else {
        panic!("the panic");
    };
    assert_eq!(lines, ["The plugin failed: the probe panics on purpose"]);
}

#[cfg(windows)]
#[test]
fn a_message_window_gone_another_way_answers_esc() {
    let temp = plugins_with_probe(|_| {});
    let mut app = app_with_plugins(&temp);
    run(&mut app, "Say hello");
    dialog(&mut app);
    app.overlay = None;
    let Dialog::Message { lines, .. } = dialog(&mut app) else {
        panic!("the plugin heard Esc");
    };
    assert_eq!(lines, ["closed with Esc"]);
}

#[cfg(windows)]
#[test]
fn the_program_ends_at_once_with_a_plugin_waiting_for_an_answer() {
    let temp = plugins_with_probe(|_| {});
    let mut app = app_with_plugins(&temp);
    // A message waiting behind another window.
    app.show_dialog(Dialog::message("Busy", vec![]));
    run_without_check(&mut app);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.plugins.waiting.is_empty() {
        app.pump_plugins();
        assert!(Instant::now() < deadline, "the message did not come");
        std::thread::sleep(Duration::from_millis(2));
    }
    let started = Instant::now();
    drop(app);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}: the plugin heard Esc before its thread was ended",
        started.elapsed()
    );
}

/// "Say hello" run straight, a window being up.
#[cfg(windows)]
fn run_without_check(app: &mut App) {
    app.run_command_for_test(0, 0, 0);
}

#[test]
fn f11_without_plugins_is_an_empty_menu() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_with_plugins(&temp);
    app.open_plugin_commands();
    let (title, items) = commands_menu(&app);
    assert_eq!(title, "Plugin commands");
    assert!(items.is_empty());
}

#[test]
fn a_plugin_that_cannot_load_is_reported_as_far_does() {
    // A later version than the program's (the program's own would load).
    let temp =
        plugins_with_probe(|manifest| manifest["api"] = (nc_plugin_api::API_VERSION + 1).into());
    let mut app = app_with_plugins(&temp);
    let Dialog::Message { title, lines, .. } = dialog(&mut app) else {
        panic!("FAR's error");
    };
    assert_eq!(title, "Error");
    assert_eq!(lines[0], "Error loading plugin");
    assert_eq!(lines[1], temp.path().join("probe").display().to_string());
    assert_eq!(
        lines[2],
        format!(
            "The plugin needs API version {}; this program has {}",
            nc_plugin_api::API_VERSION + 1,
            nc_plugin_api::API_VERSION
        )
    );
}

#[test]
fn a_plugin_turned_off_stays_off_when_the_settings_are_written() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = crate::settings::Settings::default();
    settings
        .other
        .insert("plugins.probe.enabled".to_string(), false.into());
    let app = App::new(temp.path(), 100, 40, &settings);
    let written = app.to_settings();
    assert!(!crate::settings::plugin_enabled(&written, "probe"));
}

/// A probe's manifest naming it `name`, built for API version `api`.
fn manifest(name: &str, api: u32) -> String {
    serde_json::json!({"name": name, "version": "0.1.0", "api": api, "library": "probe"})
        .to_string()
}

/// The dialog up now.
fn shown(app: &App) -> Dialog {
    let Some(Overlay::Dialog(dialog)) = &app.overlay else {
        panic!("a dialog");
    };
    dialog.clone()
}

#[test]
fn load_failures_wait_for_the_window_up_and_come_one_at_a_time() {
    let temp = tempfile::tempdir().unwrap();
    // Two later versions than the program's.
    let (one, two) = (
        nc_plugin_api::API_VERSION + 1,
        nc_plugin_api::API_VERSION + 2,
    );
    install_probe(temp.path(), "a", &manifest("one", one));
    install_probe(temp.path(), "b", &manifest("two", two));
    let mut app = app_in(temp.path());
    let settings = Dialog::message("Settings", vec!["broken".to_string()]);
    app.show_dialog(settings.clone());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    assert!(!app.pump_plugins(), "nothing new on the screen");
    assert_eq!(shown(&app), settings, "the window up stays");
    for (folder, api) in [("a", one), ("b", two)] {
        app.overlay = None;
        assert!(app.pump_plugins(), "{folder}: shown");
        let Dialog::Message {
            title,
            lines,
            error,
        } = shown(&app)
        else {
            panic!("FAR's error");
        };
        assert_eq!((title.as_str(), error), ("Error", true));
        assert_eq!(lines[1], temp.path().join(folder).display().to_string());
        assert_eq!(
            lines[2],
            format!(
                "The plugin needs API version {api}; this program has {}",
                nc_plugin_api::API_VERSION
            )
        );
        assert!(!app.pump_plugins(), "{folder}: one at a time");
        assert_eq!(shown(&app), Dialog::error(&title, lines));
    }
    app.overlay = None;
    assert!(!app.pump_plugins(), "nothing left");
    assert!(app.overlay.is_none());
}

#[test]
fn the_settings_the_program_does_not_name_are_written_back_as_read() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = crate::settings::Settings::default();
    let other = [
        ("plugins.a.enabled", serde_json::json!(false)),
        ("plugins.b.enabled", serde_json::json!(true)),
        ("x.y", serde_json::json!({"k": [1, "two", null]})),
    ];
    for (key, value) in other {
        settings.other.insert(key.to_string(), value);
    }
    let app = App::new(temp.path(), 100, 40, &settings);
    let written = app.to_settings();
    assert_eq!(written.other, settings.other);
    let text = serde_json::to_string(&written).unwrap();
    let read: crate::settings::Settings = serde_json::from_str(&text).unwrap();
    assert_eq!(read.other, settings.other, "and read again");
}

#[test]
fn a_command_past_the_menu_or_its_plugins_runs_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_with_plugins(&temp);
    app.run_plugin_command(&[(0, 1)], 1);
    app.run_plugin_command(&[], 0);
    app.run_plugin_command(&[(5, 0)], 0);
    assert!(!app.pump_plugins());
    assert!(app.overlay.is_none());
}

/// Probes `one` (folder `a`) and `two` (folder `b`), `two` given the marker files `markers` (name and
/// contents).
#[cfg(windows)]
fn two_probes(markers: &[(&str, &str)]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    install_probe(
        temp.path(),
        "a",
        &manifest("one", nc_plugin_api::API_VERSION),
    );
    let two = install_probe(
        temp.path(),
        "b",
        &manifest("two", nc_plugin_api::API_VERSION),
    );
    for (marker, text) in markers {
        std::fs::write(two.join(marker), text).unwrap();
    }
    temp
}

#[cfg(windows)]
#[test]
fn f11_lists_every_plugins_commands_sorted_by_their_text() {
    let temp = two_probes(&[
        ("commands.txt", "zeta\nalpha\nBeta"),
        ("message.txt", "From two\nhi"),
    ]);
    let mut app = app_with_plugins(&temp);
    app.handle_key(KeyEvent::from(KeyCode::F(11)));
    let (_, items) = commands_menu(&app);
    assert_eq!(
        items,
        ["alpha", "Beta", "Fail", "Panic", "Say hello", "zeta"],
        "sorted without regard to case"
    );
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("Plugin commands");
    };
    let commands: Vec<(usize, usize)> = (menu.commands.iter().flatten())
        .map(|(at, _)| *at)
        .collect();
    assert_eq!(commands, [(1, 1), (1, 2), (0, 1), (0, 2), (0, 0), (1, 0)]);
    assert_eq!(menu.bottom, "Shift+F1, F4, F3", "FAR's keys under it");
    for _ in 0..5 {
        app.handle_key(KeyEvent::from(KeyCode::Down));
    }
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    // Zeta is two's first command.
    let (title, lines) = plugin_message(&dialog(&mut app));
    assert_eq!(title, "From two");
    assert_eq!(lines, ["hi"]);
    app.handle_key(KeyEvent::from(KeyCode::Enter));
}

#[cfg(windows)]
#[test]
fn two_plugins_failing_show_one_window_at_a_time() {
    // Two tells no title: its failure is titled by its name.
    let temp = two_probes(&[("panic-in-info", "")]);
    let mut app = app_with_plugins(&temp);
    // Both sent as by the host alone: what is tried here is their failures' windows one at a time, not
    // the wait on a command's end (the keys would hold the second command until the first ended).
    app.plugins.host.run_command(0, 1, 0, 0);
    app.plugins.host.run_command(1, 1, 0, 0);
    let first = dialog(&mut app);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.plugins.waiting.is_empty() {
        assert!(!app.pump_plugins(), "the first window stays");
        assert!(Instant::now() < deadline, "the second failure did not come");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.pump_plugins());
    assert_eq!(shown(&app), first, "one at a time");
    app.overlay = None;
    let second = dialog(&mut app);
    let mut titles = Vec::new();
    for window in [first, second] {
        let Dialog::Message {
            title,
            lines,
            error,
        } = window
        else {
            panic!("a failure");
        };
        assert!(error);
        assert_eq!(lines, ["The probe fails on purpose"]);
        titles.push(title);
    }
    titles.sort();
    assert_eq!(titles, ["Probe", "two"]);
}

#[cfg(windows)]
#[test]
fn a_message_without_a_title_is_titled_by_its_plugin() {
    let temp = two_probes(&[("panic-in-info", ""), ("message.txt", "\nfrom two")]);
    std::fs::write(temp.path().join("a").join("message.txt"), "\nfrom one").unwrap();
    let mut app = app_with_plugins(&temp);
    for (plugin, title, line) in [(0, "Probe", "from one"), (1, "two", "from two")] {
        app.run_command_for_test(plugin, 0, 0);
        let (shown_title, lines) = plugin_message(&dialog(&mut app));
        assert_eq!(shown_title, title, "its own title, else its name");
        assert_eq!(lines, [line]);
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.overlay.is_none());
    }
}

#[cfg(windows)]
#[test]
fn a_plugins_message_waits_behind_another_window() {
    let temp = plugins_with_probe(|_| {});
    let mut app = app_with_plugins(&temp);
    let busy = Dialog::message("Busy", vec![]);
    app.show_dialog(busy.clone());
    // The command sent as by the host alone: what is tried here is its message behind a window, not the
    // wait on its end (which a window up lets go at once).
    app.plugins.host.run_command(0, 0, 0, 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.plugins.waiting.is_empty() {
        assert!(!app.pump_plugins());
        assert!(Instant::now() < deadline, "the message did not come");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(shown(&app), busy, "not covered");
    assert!(app.plugins.reply.is_none(), "not shown, not answered yet");
    app.overlay = None;
    assert!(app.pump_plugins());
    plugin_message(&shown(&app));
    assert!(
        app.plugins.reply.is_some(),
        "the plugin waits for this window"
    );
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.plugins.reply.is_none(), "answered");
}

/// FAR's `SetDriveMenuHotkeys`: the temporary panel's drive menu hotkey "1", once; one the user took away
/// stays away.
#[test]
fn the_temporary_panels_drive_menu_hotkey_is_set_once() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    app.set_plugins(crate::plugin_host::PluginHost::default());
    let key = "plugins.tmppanel.hotkeys.disks.Temporary panel";
    assert_eq!(
        app.plugins.kept_settings.get(key),
        Some(&serde_json::json!("1"))
    );
    app.plugins.kept_settings.remove(key);
    app.set_plugins(crate::plugin_host::PluginHost::default());
    assert_eq!(app.plugins.kept_settings.get(key), None, "not again");
}

/// Ctrl+W waits while a plugin's window waits for its answer or tells it is busy: the list's question
/// in front would let the pump answer Esc for it.
#[test]
fn the_task_list_waits_for_a_plugins_window() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let key = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
    app.plugins.busy = Some("probe".to_string());
    assert!(!app.task_list_key(key));
    app.plugins.busy = None;
    let (sender, _receiver) = std::sync::mpsc::channel();
    app.plugins.reply = Some(sender);
    assert!(!app.task_list_key(key));
}
