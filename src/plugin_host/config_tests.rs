//! A plugin's settings and configurations on the probe (`configs.txt`): its values from the program's
//! settings at the start, a configuration run on its thread, the values it sets sent to the program and
//! its items read again when it says the settings changed (FAR's `ConfigureCurrent`).
#![cfg(windows)]

use super::tests::{next_event, plugins_with_probe};
use super::*;

/// The probe with the configuration items `configs`, loaded with the program's `settings`.
fn configured_probe(
    configs: &str,
    settings: &serde_json::Value,
) -> (tempfile::TempDir, PluginHost) {
    let folder = plugins_with_probe(|_| {});
    std::fs::write(folder.path().join("probe").join("configs.txt"), configs).unwrap();
    let serde_json::Value::Object(settings) = settings else {
        panic!("the settings' keys");
    };
    let logs = folder.path().join("Logs");
    let host = PluginHost::load(folder.path(), &logs, settings, &|_| true);
    (folder, host)
}

#[test]
fn a_plugin_starts_with_its_own_values_and_tells_its_configurations() {
    let settings = serde_json::json!({
        // A number in the file edited by hand: taken as its text.
        "plugins.probe.settings.count": 4,
        "plugins.probe.enabled": true,
        "plugins.other.settings.count": "9",
    });
    let (_folder, host) = configured_probe("Options\nUnchanged\n", &settings);
    let probe = &host.plugins[0];
    assert_eq!(probe.configs, ["Options", "Unchanged"]);
    assert_eq!(
        probe.commands.last().map(String::as_str),
        Some("Configured 4"),
        "its own value, not another plugin's"
    );
    let (_folder, host) = configured_probe("", &serde_json::json!({}));
    assert!(host.plugins[0].configs.is_empty());
    let serde_json::Value::Object(values) = serde_json::json!({
        "plugins.probe.settings.text": "a",
        "plugins.probe.settings.number": -5,
        "plugins.probe.settings.flag": true,
        "plugins.probe.settings.list": [1],
        "plugins.probe.settings.none": null,
        "plugins.probe.hotkeys.commands.x": "q",
    }) else {
        unreachable!()
    };
    let read = plugin_settings(&values, "probe");
    let read: Vec<(&str, &str)> = (read.iter())
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    assert_eq!(
        read,
        [("flag", "true"), ("number", "-5"), ("text", "a")],
        "texts, numbers and flags as text; nothing else"
    );
    assert_eq!(host.plugins[0].commands, ["Say hello", "Fail", "Panic"]);
}

#[test]
fn a_configuration_that_changed_the_settings_sends_them_and_its_items_again() {
    let settings = serde_json::json!({"plugins.probe.settings.count": "1"});
    let (_folder, host) = configured_probe("Options\n", &settings);
    host.configure(0, 0, 1, 0);
    let mut sent = Vec::new();
    let items = loop {
        match next_event(&host) {
            Event::Setting { plugin, key, value } => sent.push((plugin, key, value)),
            Event::Configured { plugin, result, .. } => {
                assert_eq!(plugin, "probe");
                break result.unwrap();
            }
            other => panic!("its settings and its end: {other:?}"),
        }
    };
    let probe = |key: &str, value: &str| ("probe".to_string(), key.to_string(), value.to_string());
    assert_eq!(sent, [probe("count", "2"), probe("label", "Options")]);
    let items = items.expect("changed: its items read again");
    assert_eq!(
        items.commands.last().map(String::as_str),
        Some("Configured 2")
    );
    assert_eq!(items.configs, ["Options"]);
    assert!(items.disks.is_empty());
    // The value is the plugin's at once: counted on from it.
    host.configure(0, 0, 1, 0);
    let count = loop {
        if let Event::Setting { key, value, .. } = next_event(&host)
            && key == "count"
        {
            break value;
        }
    };
    assert_eq!(count, "3");
}

#[test]
fn a_configuration_that_changed_nothing_ends_without_items_and_a_panic_is_its_failure() {
    let (_folder, host) = configured_probe("Unchanged\nPanic\n", &serde_json::json!({}));
    host.configure(0, 0, 0, 0);
    match next_event(&host) {
        Event::Configured { plugin, result, .. } => {
            assert_eq!(plugin, "probe");
            assert_eq!(result, Ok(None), "FAR's ConfigureW returning FALSE");
        }
        other => panic!("its end: {other:?}"),
    }
    host.configure(0, 1, 0, 0);
    match next_event(&host) {
        Event::Configured {
            plugin,
            result: Err(error),
            ..
        } => {
            assert_eq!(plugin, "probe");
            assert!(error.contains("panics on purpose"), "{error}");
        }
        other => panic!("its own failure: {other:?}"),
    }
    host.configure(5, 0, 0, 0);
    assert!(host.take_event().is_none(), "no such plugin: nothing runs");
}
