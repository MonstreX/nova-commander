//! The temporary panel's configuration (ADR-0016 §14, 5d-3a; FAR's TmpPanel `Config`, `TmpCfg.cpp`), its
//! library as it is shipped: Options > Plugins configuration shows its dialog, OK keeps its options in the
//! program's settings under FAR's keys, the menus' items follow them, its own mode is read again on the
//! panel showing it, SavLst's extension comes from the list files' mask; Esc changes nothing.

use super::*;
use crate::app::command::Command;

fn alt(ch: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT)
}

/// The dialog's form as it is up.
fn dialog_form(app: &App) -> Option<&crate::app::form::Form> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => Some(form),
        _ => None,
    }
}

/// Options > Plugins configuration, Enter on the temporary panel's item: its dialog up.
fn configure(app: &mut App) {
    app.execute(Command::PluginsConfig);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("Plugins configuration");
    };
    assert_eq!(menu.title, "Plugins configuration");
    let texts: Vec<&str> = menu.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(texts, ["&Temporary panel"]);
    app.handle_key(key(KeyCode::Enter));
    tick_until(app, |app| {
        dialog_form(app).is_some_and(|form| form.title == "Temporary panel")
    });
}

/// The configuration menu back after the plugin's call ended.
fn menu_back(app: &mut App) {
    tick_until(
        app,
        |app| matches!(&app.overlay, Some(Overlay::PluginMenu(menu)) if menu.config),
    );
}

fn kept(app: &App, name: &str, key: &str) -> Option<String> {
    let key = format!("plugins.{name}.settings.{key}");
    let value = app.plugins.kept_settings.get(&key)?;
    value.as_str().map(String::from)
}

#[test]
fn ok_keeps_the_options_and_the_menus_and_its_mode_follow_them() {
    let (_temp, _files, mut app) = opened("tmpcfgone");
    configure(&mut app);
    // FAR's labels and hotkeys: Alt+P "Add to &Plugins menu", Alt+U "F&ull screen mode".
    app.handle_key(alt('p'));
    app.handle_key(alt('u'));
    app.handle_key(key(KeyCode::Enter));
    menu_back(&mut app);
    assert_eq!(
        kept(&app, "tmpcfgone", "AddToPluginsMenu").as_deref(),
        Some("0")
    );
    assert_eq!(
        kept(&app, "tmpcfgone", "FullScreenPanel").as_deref(),
        Some("1")
    );
    assert_eq!(
        kept(&app, "tmpcfgone", "AddToDisksMenu").as_deref(),
        Some("1")
    );
    assert_eq!(
        kept(&app, "tmpcfgone", "Mode").as_deref(),
        Some("1"),
        "Replace"
    );
    assert_eq!(
        kept(&app, "tmpcfgone", "CopyContents").as_deref(),
        Some("0")
    );
    assert_eq!(kept(&app, "tmpcfgone", "Mask").as_deref(), Some("*.temp"));
    assert_eq!(kept(&app, "tmpcfgone", "ListUTF8").as_deref(), Some("1"));
    let saved = app.to_settings();
    assert_eq!(
        saved.other.get("plugins.tmpcfgone.settings.Prefix"),
        Some(&serde_json::json!("tmp")),
        "saved with the settings"
    );
    let plugin = &app.plugins.host.plugins[0];
    assert!(plugin.commands.is_empty(), "off F11's menu");
    assert_eq!(plugin.disks, ["Temporary panel"]);
    assert_eq!(plugin.configs, ["Temporary panel"], "always configured");
    // FAR's `ConfigureCurrent`: the panel showing it read again, its own mode full screen now.
    tick_until(&mut app, |app| {
        let look = &app.panels[0].look;
        look.mode(4).is_some_and(|mode| mode.full_screen)
    });
    app.handle_key(key(KeyCode::Esc));
    assert!(app.overlay.is_none(), "Esc closes it (FAR's loop)");
}

#[test]
fn the_list_files_mask_gives_savlst_its_extension() {
    let (_temp, files, mut app) = opened("tmpcfgtwo");
    configure(&mut app);
    // Alt+L: the mask's line (its label's hotkey), typed anew.
    app.handle_key(alt('l'));
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
    for ch in "*.lst,*.temp".chars() {
        app.handle_key(key(KeyCode::Char(ch)));
    }
    app.handle_key(key(KeyCode::Enter));
    menu_back(&mut app);
    assert_eq!(
        kept(&app, "tmpcfgtwo", "Mask").as_deref(),
        Some("*.lst,*.temp")
    );
    app.handle_key(key(KeyCode::Esc));
    // SavLst saves nothing from an empty panel: an entry first.
    point(&mut app, Side::Right, "a.txt");
    app.active = Side::Right;
    copy(&mut app, KeyCode::F(5));
    tick_until(&mut app, |app| {
        names(app, Side::Left).len() == 1 && app.overlay.is_none() && app.plugins.putting.is_none()
    });
    app.active = Side::Left;
    app.handle_key(KeyEvent::new(
        KeyCode::F(2),
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    ));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(key(KeyCode::Enter));
    let saved = files.path().join("panel0.lst");
    tick_until(&mut app, |_| saved.is_file());
}

#[test]
fn esc_in_its_dialog_keeps_the_options_and_the_menu_comes_back() {
    let (_temp, _files, mut app) = opened("tmpcfgthree");
    configure(&mut app);
    app.handle_key(alt('d'));
    app.handle_key(key(KeyCode::Esc));
    menu_back(&mut app);
    assert_eq!(
        kept(&app, "tmpcfgthree", "AddToDisksMenu"),
        None,
        "nothing written"
    );
    assert_eq!(app.plugins.host.plugins[0].disks, ["Temporary panel"]);
}
