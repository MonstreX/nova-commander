//! F3 in F11's menu: FAR's "Plugin information" with the probe's lines read-only, OK back to the menu at
//! its line.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::fileops::tests::app_in;
use crate::plugin_host::tests::{load, plugins_with_probe};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The read-only lines' texts of the form up.
fn values(app: &App) -> (String, Vec<String>) {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no dialog");
    };
    let texts = (form.items.iter())
        .filter(|item| item.read_only)
        .filter_map(|item| match &item.control {
            Control::Edit(edit) => Some(edit.text().to_string()),
            _ => None,
        })
        .collect();
    (form.title.clone(), texts)
}

#[test]
fn f3_shows_the_plugins_information_and_ok_brings_the_menu_back() {
    let plugins = plugins_with_probe(|_| {});
    let mut app = app_in(plugins.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    app.open_plugin_commands();
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("F11's menu");
    };
    let label = menu.lines[0].text.clone();
    app.handle_key(key(KeyCode::F(3)));
    let (title, texts) = values(&app);
    assert_eq!(title, "Plugin information");
    let loaded = &app.plugins.host.plugins[0];
    assert_eq!(texts[0], loaded.info.title, "Title");
    assert_eq!(texts[3], loaded.info.version, "Version");
    assert!(texts[4].ends_with("probe.dll"), "Module path: {}", texts[4]);
    assert_eq!(texts[5], "probe", "the plugin's name for FAR's UUID");
    assert!(
        label.contains(&texts[6]),
        "the item: {label} / {}",
        texts[6]
    );
    assert_eq!(texts.len(), 8, "with the prefixes' line");
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(&app.overlay, Some(Overlay::PluginMenu(menu)) if menu.selected == 0),
        "the menu back at its line"
    );
}
