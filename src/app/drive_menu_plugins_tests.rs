//! The plugins' items in the drive menu: their lines, the keys that move over them and choose them, their
//! hotkeys and order, Ctrl+7, and an item chosen running in its plugin.

use super::*;
use crate::app::Side;
use crate::vfs::local::{Drive, DriveColumns};

fn drive(letter: char) -> Drive {
    Drive {
        root: format!("{letter}:\\"),
        letter: Some(letter),
        kind: crate::platform::volumes::DriveKind::Fixed,
        disconnected: false,
        subst: None,
        image: None,
        drive_type: None,
        label: None,
        file_system: None,
        size: None,
        associated_path: None,
        volume_read: true,
    }
}

fn item(index: usize, hotkey: Option<char>) -> DiskItem {
    DiskItem {
        plugin: 0,
        index,
        label: format!("Item {index}"),
        hotkey,
    }
}

/// Owner 2026-11-22 (over FAR's blank): an item without the user's key gets FAR's first-start one when
/// free, else the first free digit, then letter no drive has; the user's keys stay and are skipped.
#[test]
fn items_without_the_users_key_get_free_ones_digits_first() {
    let mut items: Vec<DiskItem> = (0..14).map(|index| item(index, None)).collect();
    items[5].hotkey = Some('2');
    items[6].hotkey = Some('a');
    let far_key = |item: &DiskItem| (item.index == 3).then_some('7');
    give_free_hotkeys(&mut items, &[drive('C'), drive('D')], far_key);
    let keys: Vec<char> = items.iter().map(|item| item.hotkey.unwrap()).collect();
    assert_eq!(
        keys,
        [
            '0', '1', '3', '7', '4', '2', 'a', '5', '6', '8', '9', 'B', 'E', 'F'
        ],
        "FAR's key to its item, the user's kept, the digits, then letters past A (the user's), C and D"
    );
    let mut taken = vec![item(0, None), item(1, Some('1'))];
    give_free_hotkeys(&mut taken, &[], |_| Some('1'));
    assert_eq!(taken[0].hotkey, Some('0'), "FAR's key taken by the user's");
    let mut many: Vec<DiskItem> = (0..40).map(|index| item(index, None)).collect();
    give_free_hotkeys(&mut many, &[], |_| None);
    assert_eq!(many[35].hotkey, Some('Z'));
    assert_eq!(many[36].hotkey, None, "none left");
}

#[test]
fn far_s_first_start_keys_are_its_four() {
    let keys: Vec<(&str, &str)> = (crate::app::plugins::DRIVE_MENU_HOTKEYS.iter())
        .map(|(plugin, _, key)| (*plugin, *key))
        .collect();
    assert_eq!(
        keys,
        [
            ("proclist", "0"),
            ("tmppanel", "1"),
            ("netway", "2"),
            ("network", "3")
        ]
    );
}

/// Drives C and D, then the items given.
fn menu(plugins: Vec<DiskItem>) -> DriveMenu {
    DriveMenu {
        side: Side::Left,
        drives: vec![drive('C'), drive('D')],
        plugins,
        selected: 0,
        columns: DriveColumns::default(),
        top: std::cell::Cell::new(0),
    }
}

#[test]
fn the_items_come_after_the_drives_and_a_separator_never_selected() {
    let with = menu(vec![item(0, Some('1')), item(1, None)]);
    assert_eq!(with.line_count(), 5);
    assert_eq!(with.plugin_at(1), None, "a drive");
    assert_eq!(with.plugin_at(2), None, "the separator");
    assert_eq!(with.plugin_at(3), Some(&with.plugins[0]));
    assert_eq!(with.plugin_at(4), Some(&with.plugins[1]));
    assert_eq!(with.plugin_at(5), None);
    let choosable: Vec<bool> = (0..6).map(|line| with.choosable(line)).collect();
    assert_eq!(choosable, [true, true, false, true, true, false]);
    let without = menu(Vec::new());
    assert_eq!(without.line_count(), 2, "no separator without items");
    assert!(!without.choosable(2));
}

#[test]
fn up_and_down_pass_the_separator_and_go_round() {
    let with = menu(vec![item(0, Some('1')), item(1, None)]);
    assert_eq!(with.step(1, true), 3, "past the separator");
    assert_eq!(with.step(3, false), 1);
    assert_eq!(with.step(4, true), 0, "round from the end");
    assert_eq!(with.step(0, false), 4, "round from the start");
    assert_eq!(with.step(99, false), 3, "from beyond: from the last line");
    let without = menu(Vec::new());
    assert_eq!(without.step(1, true), 0);
    assert_eq!(without.step(0, false), 1);
    let empty = DriveMenu {
        drives: Vec::new(),
        ..menu(Vec::new())
    };
    assert_eq!(empty.step(0, true), 0, "nothing to move to");
}

#[test]
fn a_line_that_cannot_be_selected_gives_the_nearest_that_can() {
    let with = menu(vec![item(0, Some('1'))]);
    assert_eq!(with.fit(1), 1);
    assert_eq!(
        with.fit(2),
        1,
        "the separator (the last drive gone): the drive above, never an item by chance"
    );
    assert_eq!(with.fit(usize::MAX), 3, "past the end: the last");
    let only_items = DriveMenu {
        drives: Vec::new(),
        ..menu(vec![item(0, None)])
    };
    assert_eq!(only_items.fit(0), 1, "the separator first: the item");
    let without = menu(Vec::new());
    assert_eq!(without.fit(7), 1, "a plugin's line gone: the last drive");
    let empty = DriveMenu {
        drives: Vec::new(),
        ..menu(Vec::new())
    };
    assert_eq!(empty.fit(3), 0);
}

#[test]
fn a_hotkey_finds_its_items_line() {
    let with = menu(vec![item(0, Some('2')), item(1, Some('1')), item(2, None)]);
    assert_eq!(with.hotkey_line('1'), Some(4));
    assert_eq!(with.hotkey_line('2'), Some(3));
    assert_eq!(with.hotkey_line('3'), None);
    let letters = menu(vec![item(0, Some('T')), item(1, Some('q'))]);
    assert_eq!(letters.hotkey_line('t'), Some(3), "either case, as FAR's");
    assert_eq!(letters.hotkey_line('Q'), Some(4));
    assert_eq!(
        with.hotkey_line('C'),
        None,
        "a drive's letter is not an item's"
    );
}

#[cfg(windows)]
mod with_the_probe {
    use std::time::{Duration, Instant};

    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::app::fileops::tests::app_in;
    use crate::app::waiting::finish_waiting;
    use crate::app::{Dialog, Overlay};
    use crate::plugin_host::tests::{install_probe, load as load_plugins};

    fn manifest(name: &str) -> String {
        format!(r#"{{"name": "{name}", "version": "0.1.0", "api": 11, "library": "probe"}}"#)
    }

    /// Plugins `one` and `two` (folders `a` and `b`) with their drive menu items.
    fn two_probes(one: &str, two: &str) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        for (folder, name, disks) in [("a", "one", one), ("b", "two", two)] {
            let place = install_probe(temp.path(), folder, &manifest(name));
            std::fs::write(place.join("disks.txt"), disks).unwrap();
        }
        temp
    }

    fn app_with(plugins: &tempfile::TempDir) -> App {
        let mut app = app_in(plugins.path());
        app.set_plugins(load_plugins(plugins.path(), &|_| true));
        app
    }

    fn labels_and_keys(items: &[DiskItem]) -> Vec<(String, Option<char>)> {
        (items.iter())
            .map(|item| (item.label.clone(), item.hotkey))
            .collect()
    }

    /// The hotkey the user gave plugin `name`'s drive menu item `label` (F4's dialog keeps it so).
    fn give_hotkey(app: &mut App, name: &str, label: &str, key: char) {
        let setting = format!("plugins.{name}.hotkeys.disks.{label}");
        app.plugins
            .kept_settings
            .insert(setting, key.to_string().into());
    }

    #[test]
    fn every_item_has_a_hotkey_and_the_lines_go_by_name_or_by_hotkey() {
        // Owner 2026-11-22 over FAR's `AddPluginItems` (a hotkey only where the user gave one, F4): the
        // others get free digits in the lines' order.
        let plugins = two_probes("Zeta\nalpha", "Mid\nZ01\nZ02");
        let mut app = app_with(&plugins);
        let items = app.disk_items(&[]);
        let names: Vec<(String, Option<char>)> = labels_and_keys(&items);
        let expected = [
            ("alpha", '0'),
            ("Mid", '1'),
            ("Z01", '2'),
            ("Z02", '3'),
            ("Zeta", '4'),
        ];
        let expected: Vec<(String, Option<char>)> = (expected.iter())
            .map(|(label, key)| (label.to_string(), Some(*key)))
            .collect();
        assert_eq!(names, expected, "by name, case aside, the digits in turn");
        let alpha = &items[0];
        assert_eq!((alpha.plugin, alpha.index), (0, 1), "plugin one's second");
        give_hotkey(&mut app, "two", "Z02", '1');
        give_hotkey(&mut app, "one", "Zeta", '5');
        let by_name = labels_and_keys(&app.disk_items(&[]));
        assert_eq!(
            by_name[1],
            ("Mid".to_string(), Some('2')),
            "the user's 1 skipped"
        );
        assert_eq!(by_name[3], ("Z02".to_string(), Some('1')));
        assert_eq!(by_name[4], ("Zeta".to_string(), Some('5')));
        app.config.drive_columns.sort_plugins_by_hotkey = true;
        let by_key = labels_and_keys(&app.disk_items(&[]));
        let order: Vec<&str> = by_key.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(order, ["alpha", "Z02", "Mid", "Z01", "Zeta"], "by hotkey");
        app.config.drive_columns.plugins = false;
        assert!(app.disk_items(&[]).is_empty(), "Ctrl+7 hides them");
    }

    fn open_menu(app: &mut App) -> DriveMenu {
        app.open_drive_menu(Side::Left);
        finish_waiting(app);
        shown_menu(app)
    }

    fn shown_menu(app: &App) -> DriveMenu {
        let Some(Overlay::DriveMenu(menu)) = &app.overlay else {
            panic!("the drive menu");
        };
        menu.clone()
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        let menu = shown_menu(app);
        app.overlay = None;
        app.handle_drive_menu_key(menu, KeyEvent::new(code, modifiers));
        finish_waiting(app);
    }

    /// Ticks until the plugin's message is up: its title and form lines.
    fn plugin_message(app: &mut App) -> (String, Vec<String>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.pump_plugins();
            if let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay {
                let lines = (form.items.iter())
                    .filter_map(|item| match &item.control {
                        crate::app::form::Control::Label(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect();
                return (form.title.clone(), lines);
            }
            assert!(Instant::now() < deadline, "no message from the plugin");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn enter_on_an_item_runs_it_in_its_plugin_and_the_menu_goes() {
        let plugins = two_probes("Temp", "Other");
        let mut app = app_with(&plugins);
        let menu = open_menu(&mut app);
        let drives = menu.drives.len();
        assert_eq!(menu.line_count(), drives + 3);
        press(&mut app, KeyCode::End, KeyModifiers::NONE);
        assert_eq!(shown_menu(&app).selected, drives + 2, "the last item");
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(shown_menu(&app).selected, drives - 1, "past the separator");
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.overlay.is_none(), "the menu goes");
        let (title, lines) = plugin_message(&mut app);
        assert_eq!(
            (title.as_str(), lines),
            ("Disk", vec!["0 Temp".to_string()])
        );
    }

    #[test]
    fn an_items_hotkey_runs_it_and_ctrl_7_hides_the_items() {
        let plugins = two_probes("Temp", "Other");
        let mut app = app_with(&plugins);
        give_hotkey(&mut app, "two", "Other", '2');
        let drives = open_menu(&mut app).drives.len();
        press(&mut app, KeyCode::End, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('7'), KeyModifiers::CONTROL);
        let hidden = shown_menu(&app);
        assert!(hidden.plugins.is_empty());
        assert_eq!(
            hidden.selected,
            drives - 1,
            "the item's line gone: the last drive"
        );
        press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
        assert!(
            shown_menu(&app).plugins.is_empty(),
            "no item: the key does nothing"
        );
        press(&mut app, KeyCode::Char('7'), KeyModifiers::CONTROL);
        assert_eq!(shown_menu(&app).plugins.len(), 2, "shown again");
        press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
        assert!(app.overlay.is_none());
        let (title, lines) = plugin_message(&mut app);
        assert_eq!(
            (title.as_str(), lines),
            ("Disk", vec!["0 Other".to_string()])
        );
    }

    #[test]
    fn each_opening_asks_the_plugins_their_items_again_as_f11_does() {
        let plugins = two_probes("Temp", "Other");
        let mut app = app_with(&plugins);
        std::fs::write(
            plugins.path().join("b").join("disks.txt"),
            "Other
New",
        )
        .unwrap();
        let menu = open_menu(&mut app);
        let labels: Vec<&str> = menu
            .plugins
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(
            labels,
            ["New", "Other", "Temp"],
            "FAR's GetPluginInfo for each menu"
        );
        std::fs::write(plugins.path().join("a").join("commands.txt"), "Fresh").unwrap();
        app.overlay = None;
        app.open_plugin_commands();
        let Some(Overlay::PluginMenu(commands)) = &app.overlay else {
            panic!("F11's menu");
        };
        let texts: Vec<String> = (commands.lines.iter())
            .map(|line| line.text.replace('&', ""))
            .collect();
        assert!(texts.contains(&"Fresh".to_string()), "F11's too: {texts:?}");
    }

    #[test]
    fn f3_on_an_item_tells_its_plugin_and_ok_brings_the_menu_back_at_it() {
        let plugins = two_probes("Temp", "Other");
        let mut app = app_with(&plugins);
        let drives = open_menu(&mut app).drives.len();
        press(&mut app, KeyCode::Home, KeyModifiers::NONE);
        press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
        assert!(
            matches!(app.overlay, Some(Overlay::DriveMenu(_))),
            "on a drive nothing"
        );
        press(&mut app, KeyCode::End, KeyModifiers::NONE);
        let menu = shown_menu(&app);
        app.overlay = None;
        app.handle_drive_menu_key(menu, KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
        let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
            panic!("Plugin information");
        };
        assert_eq!(form.title, "Plugin information");
        let texts: Vec<String> = (form.items.iter())
            .filter_map(|item| match &item.control {
                crate::app::form::Control::Edit(edit) => Some(edit.text().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(texts[5], "one", "the last item is plugin one's Temp");
        assert_eq!(texts[6], "Temp");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        finish_waiting(&mut app);
        assert_eq!(shown_menu(&app).selected, drives + 2, "back at its line");
    }
}
