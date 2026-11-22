//! EMenu (FAR's emenu, P6.3) through the program, the plugin's library as it is shipped: Apps on a panel
//! shows the file's text menu, `rclk_cmd:rename` is Shift+F6, an unknown command and files of two folders
//! are told, F11 shows its main menu. No command of Explorer's is run.
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::plugin_host::tests::install_shipped;

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The plugin installed as `name`, the program in a folder holding `files`, read, the cursor on the first.
fn app_with(name: &str, files: &[&str]) -> (tempfile::TempDir, tempfile::TempDir, App) {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "emenu", name);
    let folder = tempfile::tempdir().unwrap();
    for file in files {
        std::fs::write(folder.path().join(file), file).unwrap();
    }
    let mut app = App::new(
        folder.path(),
        100,
        30,
        &crate::settings::Settings::default(),
    );
    let host = crate::plugin_host::PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &serde_json::Map::new(),
        &|_| true,
    );
    app.set_plugins(host);
    app.active = Side::Left;
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    if let Some(first) = files.first() {
        let panel = &mut app.panels[0];
        panel.cursor = (panel.entries.iter())
            .position(|entry| entry.name == *first)
            .unwrap();
    }
    (plugins, folder, app)
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn run_line(app: &mut App, line: &str) {
    app.command_line.set(line);
    app.run_command_line();
}

/// The plugin's menu shown: its title and lines.
fn plugin_menu(app: &App) -> Option<(String, Vec<String>)> {
    match &app.overlay {
        Some(Overlay::PluginMenu(menu)) if menu.commands.is_none() => Some((
            menu.title.clone(),
            menu.lines.iter().map(|line| line.text.clone()).collect(),
        )),
        _ => None,
    }
}

/// The message or dialog shown, as text.
fn shown_form(app: &App) -> Option<String> {
    match &app.overlay {
        Some(Overlay::Dialog(crate::app::dialog::Dialog::Form { form, .. })) => {
            Some(format!("{:?}", form.items))
        }
        _ => None,
    }
}

#[test]
fn apps_on_a_file_shows_its_explorer_menu_as_text_and_esc_closes_it() {
    let (_plugins, _folder, mut app) = app_with("emone", &["a.txt"]);
    key(&mut app, KeyCode::Menu);
    tick_until(&mut app, |app| plugin_menu(app).is_some());
    let (title, lines) = plugin_menu(&app).unwrap();
    assert_eq!(title, "a.txt");
    assert!(lines.len() > 3, "Explorer's commands: {lines:?}");
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, |app| !app.plugins.host.in_call("emone"));
    assert!(app.overlay.is_none(), "a cancel says nothing");
}

#[test]
fn rename_by_its_verb_is_the_programs_shift_f6() {
    let (_plugins, _folder, mut app) = app_with("emtwo", &["a.txt"]);
    run_line(&mut app, "rclk_cmd:rename a.txt");
    tick_until(&mut app, |app| shown_form(app).is_some());
    let shown = shown_form(&app).unwrap();
    assert!(shown.contains("a.txt"), "the rename dialog: {shown}");
}

#[test]
fn an_unknown_command_and_files_of_two_folders_are_told() {
    let (_plugins, folder, mut app) = app_with("emthree", &["a.txt"]);
    run_line(&mut app, "rclk_cmd:no-such-verb-vc a.txt");
    tick_until(&mut app, |app| shown_form(app).is_some());
    assert!(
        shown_form(&app)
            .unwrap()
            .contains("Can't invoke menu command.")
    );
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, |app| app.overlay.is_none());
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("b.txt"), "b").unwrap();
    let line = format!(
        "rclk: \"{}\" \"{}\"",
        folder.path().join("a.txt").display(),
        other.path().join("b.txt").display()
    );
    run_line(&mut app, &line);
    tick_until(&mut app, |app| shown_form(app).is_some());
    assert!(shown_form(&app).unwrap().contains("different folders"));
}

#[test]
fn f11_shows_its_main_menu() {
    let (_plugins, _folder, mut app) = app_with("emfour", &["a.txt"]);
    key(&mut app, KeyCode::F(11));
    tick_until(
        &mut app,
        |app| matches!(&app.overlay, Some(Overlay::PluginMenu(menu)) if menu.commands.is_some()),
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| plugin_menu(app).is_some());
    let (title, lines) = plugin_menu(&app).unwrap();
    assert_eq!(title, "EMenu");
    assert_eq!(lines, ["&Context menu", "&My Computer"]);
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, |app| !app.plugins.host.in_call("emfour"));
}

#[test]
fn apps_without_the_plugin_does_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let mut bare = App::new(
        folder.path(),
        100,
        30,
        &crate::settings::Settings::default(),
    );
    assert_eq!(bare.context_menu_key(None, Side::Left, None), None);
    assert!(bare.overlay.is_none());
}

#[test]
fn apps_on_a_drive_of_the_drive_menu_shows_the_drives_menu() {
    let (_plugins, _folder, mut app) = app_with("emsix", &["a.txt"]);
    app.execute(crate::app::Command::ChangeDrive(Side::Left));
    tick_until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    key(&mut app, KeyCode::Menu);
    tick_until(&mut app, |app| plugin_menu(app).is_some());
    let (title, lines) = plugin_menu(&app).unwrap();
    assert!(title.ends_with(":\\"), "the drive's root: {title}");
    assert!(!lines.is_empty());
    key(&mut app, KeyCode::Esc);
    // FAR's drive menu stays under EMenu's: back once its call ends, for the keys it posts (Shift+F6).
    tick_until(&mut app, |app| {
        !app.plugins.host.in_call("emsix") && matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    assert!(app.plugins.drive_menu_under.is_none());
    app.overlay = None;
    app.execute(crate::app::Command::ChangeDrive(Side::Left));
    tick_until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    app.handle_key(KeyEvent::new(KeyCode::Menu, KeyModifiers::CONTROL));
    assert!(
        matches!(app.overlay, Some(Overlay::DriveMenu(_))) && !app.plugins.host.in_call("emsix"),
        "Ctrl+Apps is not FAR's"
    );
}

/// The panels laid out as the renderer puts them on a 100 x 30 screen (entries from row 2).
fn lay_out(app: &mut App) {
    for (index, x) in [0u16, 50].into_iter().enumerate() {
        app.mouse.areas[index] = Some(crate::app::PanelArea {
            x,
            y: 0,
            width: 50,
            height: 30,
            list: Some(crate::app::ListArea {
                top: 2,
                rows: 25,
                stripe_lefts: vec![x + 1, x + 25],
            }),
        });
        app.panels[index].set_view_metrics(25, 2);
    }
}

fn right(app: &mut App, row: u16, modifiers: KeyModifiers, down: bool) {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::platform::input::MouseButtons;
    let buttons = MouseButtons {
        left: false,
        right: down,
        middle: false,
    };
    app.handle_mouse(Mouse {
        column: 5,
        row,
        buttons,
        pressed: buttons,
        action: MouseAction::Button,
        modifiers,
    });
}

#[test]
fn a_right_click_on_a_file_shows_its_menu_as_the_button_is_let_go() {
    let (_plugins, _folder, mut app) = app_with("emseven", &["a.txt", "b.txt"]);
    lay_out(&mut app);
    // `..`, a.txt, b.txt from row 2.
    right(&mut app, 4, KeyModifiers::NONE, true);
    assert_eq!(
        app.panels[0].current().unwrap().name,
        "b.txt",
        "the cursor there"
    );
    assert!(!app.plugins.host.in_call("emseven"), "not while it is held");
    right(&mut app, 4, KeyModifiers::NONE, false);
    tick_until(&mut app, |app| plugin_menu(app).is_some());
    assert_eq!(plugin_menu(&app).unwrap().0, "b.txt");
    assert!(!app.panels[0].current_selected(), "nothing selected");
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, |app| !app.plugins.host.in_call("emseven"));
    // Ctrl with it, or the setting on: the right button selects.
    right(&mut app, 3, KeyModifiers::CONTROL, true);
    right(&mut app, 3, KeyModifiers::NONE, false);
    assert!(app.overlay.is_none());
    assert!(app.panels[0].current_selected(), "Ctrl: selected");
    app.panels[0].options.right_click_select = true;
    right(&mut app, 4, KeyModifiers::NONE, true);
    right(&mut app, 4, KeyModifiers::NONE, false);
    assert!(app.overlay.is_none() && app.panels[0].current_selected());
}

#[test]
fn a_right_click_on_a_drive_shows_the_drives_menu_at_the_mouse() {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::platform::input::MouseButtons;
    let (_plugins, _folder, mut app) = app_with("emeight", &["a.txt"]);
    app.execute(crate::app::Command::ChangeDrive(Side::Left));
    tick_until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    let Some(Overlay::DriveMenu(menu)) = &app.overlay else {
        unreachable!()
    };
    let selected = menu.selected;
    let buttons = MouseButtons {
        left: false,
        right: true,
        middle: false,
    };
    let click = Mouse {
        column: 10,
        row: 6,
        buttons,
        pressed: buttons,
        action: MouseAction::Button,
        modifiers: KeyModifiers::NONE,
    };
    assert!(app.drive_right_click(selected, &click));
    tick_until(&mut app, |app| plugin_menu(app).is_some());
    assert!(plugin_menu(&app).unwrap().0.ends_with(":\\"));
    assert!(app.plugins.drive_menu_under.is_some());
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
}

#[test]
fn a_right_press_whose_release_is_taken_elsewhere_shows_nothing_later() {
    let (_plugins, _folder, mut app) = app_with("emnine", &["a.txt", "b.txt"]);
    lay_out(&mut app);
    right(&mut app, 4, KeyModifiers::NONE, true);
    // A key changes what is in front while the button is held: the pending menu goes.
    key(&mut app, KeyCode::Up);
    right(&mut app, 4, KeyModifiers::NONE, false);
    right(&mut app, 5, KeyModifiers::NONE, false);
    app.tick();
    assert!(!app.plugins.host.in_call("emnine") && app.overlay.is_none());
    // Let go over a window: taken by it, nothing later.
    right(&mut app, 4, KeyModifiers::NONE, true);
    app.show_error("Title", &std::io::Error::other("a window over the panels"));
    right(&mut app, 4, KeyModifiers::NONE, false);
    app.overlay = None;
    right(&mut app, 5, KeyModifiers::NONE, false);
    app.tick();
    assert!(!app.plugins.host.in_call("emnine") && app.overlay.is_none());
}

#[test]
fn a_right_click_in_the_drive_menu_goes_to_emenu_through_the_mouse() {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::app::mouse_menus::{Cells, MenuArea};
    use crate::platform::input::MouseButtons;
    let (_plugins, _folder, mut app) = app_with("emten", &["a.txt"]);
    app.execute(crate::app::Command::ChangeDrive(Side::Left));
    tick_until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    let frame = Cells {
        x: 10,
        y: 5,
        width: 30,
        height: 10,
    };
    app.mouse.menu = Some(MenuArea {
        frame,
        outer: frame,
        first: 0,
    });
    let buttons = MouseButtons {
        left: false,
        right: true,
        middle: false,
    };
    // The first drive's line, under the frame's top.
    app.handle_mouse(Mouse {
        column: 15,
        row: 6,
        buttons,
        pressed: buttons,
        action: MouseAction::Button,
        modifiers: KeyModifiers::NONE,
    });
    tick_until(&mut app, |app| plugin_menu(app).is_some());
    assert!(plugin_menu(&app).unwrap().0.ends_with(":\\"));
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
}
