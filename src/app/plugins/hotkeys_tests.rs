//! The plugins' hotkeys as FAR's (`SetHotKeyDialog`, `CommandsMenu`, `AddPluginItems`): F11's menu with
//! its automatic hotkeys and "Shift+F1, F4, F3" under it, F4 giving an item its own (kept in the settings,
//! then "&k  text"), a space taking it away; the drive menu's F4 the same.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::plugin_host::tests::{install_probe, load as load_plugins};

/// The probe installed as `name` with its drive menu item "Temp", loaded.
fn probe_app(name: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    let place = install_probe(temp.path(), name, &manifest);
    std::fs::write(place.join("disks.txt"), "Temp").unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    (temp, app)
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn menu_lines(app: &App) -> Vec<String> {
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("Plugin commands");
    };
    menu.lines.iter().map(|line| line.text.clone()).collect()
}

/// The hotkey dialog's field: its text and cursor.
fn field(app: &App) -> (String, usize) {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("Assign plugin hot key");
    };
    assert_eq!(form.title, "Assign plugin hot key");
    let Some(Control::Masked(input)) = form.items.get(1).map(|item| &item.control) else {
        panic!("FAR's fixed field");
    };
    (input.text(), input.cursor())
}

/// The hotkey dialog up: `typed` typed into its field, then OK.
fn give(app: &mut App, typed: &str) {
    field(app);
    for ch in typed.chars() {
        press(app, KeyCode::Char(ch));
    }
    press(app, KeyCode::Enter);
}

#[test]
fn f11_gives_its_own_hotkeys_until_one_is_given_then_the_users() {
    let (_temp, mut app) = probe_app("hotone");
    press(&mut app, KeyCode::F(11));
    assert_eq!(
        menu_lines(&app),
        ["&Fail", "&Panic", "&Say hello"],
        "FAR's automatic hotkeys"
    );
    // FAR's `CommandsMenu`: `VMENU_WRAPMODE`, Up on the first line goes to the last.
    press(&mut app, KeyCode::Up);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("Plugin commands");
    };
    assert_eq!(menu.selected, 2);
    press(&mut app, KeyCode::Home);
    // F4 on "Say hello": the user's own hotkey.
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::F(4));
    give(&mut app, "z");
    let lines = menu_lines(&app);
    assert_eq!(lines[2], "&z  Say hello", "FAR's AddHotkey");
    assert!(
        lines[0].ends_with("Fail") && lines[0].starts_with("   "),
        "{lines:?}"
    );
    let saved = app.to_settings();
    assert_eq!(
        saved.other.get("plugins.hotone.hotkeys.commands.Say hello"),
        Some(&serde_json::json!("z"))
    );
    // FAR's fixed field of one place: the hotkey shown, the cursor on it, typed over.
    press(&mut app, KeyCode::F(4));
    assert_eq!(field(&app), ("z".to_string(), 0));
    give(&mut app, "q");
    assert_eq!(menu_lines(&app)[2], "&q  Say hello", "typed over");
    // A space takes it away.
    press(&mut app, KeyCode::F(4));
    give(&mut app, " ");
    assert_eq!(
        menu_lines(&app)[2],
        "&Say hello",
        "none given: automatic again"
    );
    assert!(
        !app.plugins
            .kept_settings
            .contains_key("plugins.hotone.hotkeys.commands.Say hello")
    );
}

#[test]
fn the_drive_menus_f4_gives_an_item_a_hotkey_that_then_opens_it() {
    let (_temp, mut app) = probe_app("hottwo");
    app.open_drive_menu(Side::Left);
    crate::app::waiting::finish_waiting(&mut app);
    let Some(Overlay::DriveMenu(menu)) = &app.overlay else {
        panic!("the drive menu");
    };
    assert_eq!(
        menu.plugins[0].hotkey,
        Some('0'),
        "the first free digit by default (owner 2026-11-22; FAR's: none)"
    );
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::F(4));
    give(&mut app, "7");
    crate::app::waiting::finish_waiting(&mut app);
    let Some(Overlay::DriveMenu(menu)) = &app.overlay else {
        panic!("the drive menu back");
    };
    assert_eq!(menu.plugins[0].hotkey, Some('7'));
    // Ctrl+A as F4 (FAR's `diskmenu.cpp`).
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(field(&app), ("7".to_string(), 0));
    press(&mut app, KeyCode::Esc);
    crate::app::waiting::finish_waiting(&mut app);
    press(&mut app, KeyCode::Char('7'));
    assert!(app.overlay.is_none(), "the item run");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(&app.overlay, Some(Overlay::Dialog(_))) {
        app.tick();
        assert!(Instant::now() < deadline, "the probe's answer");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn f4_on_a_menu_without_commands_keeps_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    press(&mut app, KeyCode::F(11));
    assert!(menu_lines(&app).is_empty());
    press(&mut app, KeyCode::F(4));
    assert!(menu_lines(&app).is_empty(), "FAR's: no item, still up");
}

#[test]
fn f11s_items_are_ordered_as_fars_without_their_hotkey_marks() {
    // FAR's VMenu::SortItems: remove_highlight, then string_sort::less (review R42).
    use std::cmp::Ordering::{Greater, Less};
    assert_eq!(menu_order("&Network", "Archive"), Greater, "a lone & goes");
    assert_eq!(
        menu_order("Plugin 2", "Plugin 10"),
        Less,
        "digits as numbers"
    );
    assert_eq!(
        menu_order("&&b", "&b"),
        Less,
        "&& stays a & (a sign, before letters), a lone & goes"
    );
    assert_eq!(
        menu_order("my-tool", "mytool"),
        Less,
        "a sign before letters"
    );
}
