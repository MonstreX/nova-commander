//! The mouse on the panels (`mouse`) and its records turned into input (`console_input`).

use ratatui::crossterm::event::KeyModifiers;

use super::console_input::{Input, Mouse, MouseAction, Translator};
use super::{App, ListArea, Overlay, PanelArea, Side};
use crate::platform::input::{ConsoleRecord, ControlKeys, MouseButtons, MouseKind, MouseRecord};

const LEFT: MouseButtons = MouseButtons {
    left: true,
    right: false,
    middle: false,
};
const RIGHT: MouseButtons = MouseButtons {
    left: false,
    right: true,
    middle: false,
};
const MIDDLE: MouseButtons = MouseButtons {
    left: false,
    right: false,
    middle: true,
};
const NONE: MouseButtons = MouseButtons {
    left: false,
    right: false,
    middle: false,
};
/// Rows of entries per column in the panels the tests lay out.
const ROWS: u16 = 25;

/// Two panels as the renderer puts them on a 100 x 30 screen: the entries from row 2, two stripes.
fn app_with(files: usize) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    for index in 0..files {
        std::fs::write(temp.path().join(format!("f{index:02}.txt")), "x").unwrap();
    }
    let mut app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    for (index, x) in [0u16, 50].into_iter().enumerate() {
        app.mouse.areas[index] = Some(PanelArea {
            x,
            y: 0,
            width: 50,
            height: 30,
            list: Some(ListArea {
                top: 2,
                rows: ROWS,
                stripe_lefts: vec![x + 1, x + 25],
            }),
        });
        app.panels[index].set_view_metrics(usize::from(ROWS), 2);
    }
    (temp, app)
}

fn mouse(column: u16, row: u16, buttons: MouseButtons, action: MouseAction) -> Mouse {
    Mouse {
        column,
        row,
        buttons,
        pressed: buttons,
        action,
        modifiers: KeyModifiers::NONE,
    }
}

fn click(app: &mut App, column: u16, row: u16, buttons: MouseButtons) -> bool {
    let changed = app.handle_mouse(mouse(column, row, buttons, MouseAction::Button));
    app.handle_mouse(mouse(column, row, NONE, MouseAction::Button));
    changed
}

fn current_name(app: &App) -> String {
    app.active_panel().current().unwrap().name.clone()
}

#[test]
fn a_click_moves_the_cursor_there_and_makes_the_panel_active() {
    let (_temp, mut app) = app_with(30);
    assert!(click(&mut app, 60, 2 + 3, LEFT));
    assert_eq!(app.active, Side::Right);
    assert_eq!(app.active_panel().cursor, 3);
    // The second stripe begins its column with the entry after the first stripe's last.
    click(&mut app, 80, 2, LEFT);
    assert_eq!(app.active_panel().cursor, usize::from(ROWS));
}

#[test]
fn a_click_below_the_last_entry_takes_the_last() {
    let (_temp, mut app) = app_with(2);
    click(&mut app, 10, 2 + 20, LEFT);
    assert_eq!(current_name(&app), "f01.txt");
}

#[test]
fn the_right_button_selects_what_it_goes_over_as_the_first_entry_decided() {
    let (_temp, mut app) = app_with(30);
    let row = 2 + 2;
    app.handle_mouse(mouse(10, row, RIGHT, MouseAction::Button));
    app.handle_mouse(mouse(10, row + 1, RIGHT, MouseAction::Moved));
    app.handle_mouse(mouse(10, row + 2, RIGHT, MouseAction::Moved));
    app.handle_mouse(mouse(10, row + 2, NONE, MouseAction::Button));
    let selected: Vec<&str> = app.panels[0].selected.iter().map(String::as_str).collect();
    assert_eq!(selected, ["f00.txt", "f01.txt", "f02.txt"]);
    // On a selected entry the right button deselects along the way.
    app.handle_mouse(mouse(10, row + 1, RIGHT, MouseAction::Button));
    app.handle_mouse(mouse(10, row + 2, RIGHT, MouseAction::Moved));
    app.handle_mouse(mouse(10, row + 2, NONE, MouseAction::Button));
    let selected: Vec<&str> = app.panels[0].selected.iter().map(String::as_str).collect();
    assert_eq!(selected, ["f00.txt"]);
}

#[test]
fn the_right_button_on_a_column_with_no_entries_does_nothing() {
    let (_temp, mut app) = app_with(3);
    click(&mut app, 10, 3, LEFT);
    click(&mut app, 30, 2, RIGHT);
    assert_eq!(app.active_panel().cursor, 1, "FAR's RightClickRule 2");
    assert!(app.panels[0].selected.is_empty());
    // The left button takes the last entry there.
    click(&mut app, 30, 2, LEFT);
    assert_eq!(current_name(&app), "f02.txt");
}

#[test]
fn a_double_click_enters_the_folder_whatever_the_command_line_holds() {
    let (temp, mut app) = app_with(3);
    app.command_line.set("dir");
    app.handle_mouse(mouse(10, 3, LEFT, MouseAction::Button));
    app.handle_mouse(mouse(10, 3, NONE, MouseAction::Button));
    app.handle_mouse(mouse(10, 3, LEFT, MouseAction::DoubleClick));
    assert_eq!(
        app.panels[0].path.as_local(),
        Some(temp.path().join("sub").as_path())
    );
    assert_eq!(app.command_line.text(), "dir", "the line stays");
}

#[test]
fn the_middle_button_is_enter() {
    let (temp, mut app) = app_with(3);
    click(&mut app, 10, 3, LEFT);
    click(&mut app, 10, 3, MIDDLE);
    assert_eq!(
        app.panels[0].path.as_local(),
        Some(temp.path().join("sub").as_path())
    );
}

#[test]
fn the_wheel_scrolls_the_active_panel_by_the_systems_lines_and_by_one_with_alt() {
    let (_temp, mut app) = app_with(80);
    let lines = crate::platform::input::wheel_scroll_lines() as usize;
    assert!(app.handle_mouse(mouse(70, 5, NONE, MouseAction::Wheel(-2))));
    assert_eq!(
        app.active,
        Side::Left,
        "the active panel, wherever the mouse is"
    );
    assert_eq!(app.panels[0].cursor, 2 * lines);
    let mut alt = mouse(10, 5, NONE, MouseAction::Wheel(1));
    alt.modifiers = KeyModifiers::ALT;
    app.handle_mouse(alt);
    assert_eq!(app.panels[0].cursor, 2 * lines - 1);
}

#[test]
fn a_button_held_over_the_top_moves_up_until_it_is_let_go() {
    let (_temp, mut app) = app_with(30);
    click(&mut app, 10, 2 + 5, LEFT);
    assert!(app.handle_mouse(mouse(10, 1, LEFT, MouseAction::Button)));
    assert_eq!(app.panels[0].cursor, 4, "one step at once");
    assert!(app.mouse.hold.is_some());
    // Moving within the top keeps holding; leaving it ends.
    app.handle_mouse(mouse(12, 0, LEFT, MouseAction::Moved));
    assert!(app.mouse.hold.is_some());
    app.handle_mouse(mouse(12, 0, NONE, MouseAction::Button));
    assert!(app.mouse.hold.is_none());
}

#[test]
fn a_button_held_over_the_bottom_moves_down() {
    let (_temp, mut app) = app_with(30);
    click(&mut app, 10, 2, LEFT);
    app.handle_mouse(mouse(10, 28, LEFT, MouseAction::Button));
    assert_eq!(app.panels[0].cursor, 1);
    app.handle_mouse(mouse(10, 10, LEFT, MouseAction::Moved));
    assert!(app.mouse.hold.is_none(), "back over the entries");
}

#[test]
fn the_sort_letter_opens_the_sort_menu_with_the_right_button() {
    let (_temp, mut app) = app_with(3);
    click(&mut app, 51, 1, RIGHT);
    assert!(matches!(app.overlay, Some(Overlay::Popup(_))));
}

#[test]
fn nothing_happens_under_a_dialog_or_with_the_mouse_off() {
    let (_temp, mut app) = app_with(30);
    app.show_dialog(super::Dialog::error("x", vec!["y".to_string()]));
    assert!(!click(&mut app, 10, 8, LEFT));
    assert_eq!(app.panels[0].cursor, 0);
    app.overlay = None;
    app.mouse.enabled = false;
    assert!(!click(&mut app, 10, 8, LEFT));
}

#[test]
fn a_move_with_no_button_asks_for_no_redraw() {
    let (_temp, mut app) = app_with(3);
    assert!(!app.handle_mouse(mouse(10, 3, NONE, MouseAction::Moved)));
}

fn record(buttons: MouseButtons, kind: MouseKind) -> ConsoleRecord {
    ConsoleRecord::Mouse(MouseRecord {
        column: 4,
        row: 7,
        buttons,
        control: ControlKeys::default(),
        kind,
    })
}

fn mouse_of(inputs: &[Input]) -> Option<Mouse> {
    inputs.iter().find_map(|input| match input {
        Input::Mouse(mouse) => Some(*mouse),
        _ => None,
    })
}

#[test]
fn the_wheel_counts_whole_notches_and_forgets_on_a_turn_back() {
    let mut translator = Translator::default();
    assert_eq!(
        mouse_of(&translator.translate(&record(NONE, MouseKind::Wheel(64)))),
        None
    );
    let notch = mouse_of(&translator.translate(&record(NONE, MouseKind::Wheel(64)))).unwrap();
    assert_eq!(notch.action, MouseAction::Wheel(1));
    assert_eq!((notch.column, notch.row), (4, 7));
    // 8 left over; a turn the other way starts from nothing.
    let back = translator.translate(&record(NONE, MouseKind::Wheel(-120)));
    assert_eq!(mouse_of(&back).unwrap().action, MouseAction::Wheel(-1));
    let tilt = translator.translate(&record(NONE, MouseKind::HorizontalWheel(240)));
    assert_eq!(
        mouse_of(&tilt).unwrap().action,
        MouseAction::HorizontalWheel(2)
    );
}

#[test]
fn a_record_tells_which_buttons_went_down_with_it() {
    let mut translator = Translator::default();
    let down = mouse_of(&translator.translate(&record(LEFT, MouseKind::Button))).unwrap();
    assert_eq!(down.pressed, LEFT);
    let both = MouseButtons {
        right: true,
        ..LEFT
    };
    let more = mouse_of(&translator.translate(&record(both, MouseKind::Button))).unwrap();
    assert_eq!(more.pressed, RIGHT);
    let double = mouse_of(&translator.translate(&record(LEFT, MouseKind::DoubleClick))).unwrap();
    assert_eq!(double.action, MouseAction::DoubleClick);
}

#[test]
fn the_modifiers_of_a_mouse_record_are_held_too() {
    let mut translator = Translator::default();
    let shift = ControlKeys {
        shift: true,
        ..ControlKeys::default()
    };
    let inputs = translator.translate(&ConsoleRecord::Mouse(MouseRecord {
        column: 0,
        row: 0,
        buttons: LEFT,
        control: shift,
        kind: MouseKind::Button,
    }));
    assert!(matches!(inputs[0], Input::Held(held) if held.shift));
    assert_eq!(mouse_of(&inputs).unwrap().modifiers, KeyModifiers::SHIFT);
}

#[test]
fn the_list_under_the_command_line_keeps_the_mouse_from_the_panels() {
    let (_temp, mut app) = app_with(30);
    app.push_history("zz-one");
    app.push_history("zz-two");
    for ch in "zz".chars() {
        app.handle_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(ch),
            KeyModifiers::NONE,
        ));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app
        .completion
        .as_ref()
        .is_some_and(super::Completion::is_shown)
    {
        assert!(std::time::Instant::now() < deadline, "the list never came");
        app.tick();
    }
    assert!(!click(&mut app, 10, 8, LEFT));
    assert!(!app.handle_mouse(mouse(10, 8, NONE, MouseAction::Wheel(-1))));
    assert_eq!(app.panels[0].cursor, 0);
    assert!(app.completion.is_some(), "FAR's list is modal");
}

#[test]
fn a_hold_ends_when_a_dialog_comes_or_the_window_loses_focus() {
    let (_temp, mut app) = app_with(30);
    click(&mut app, 10, 2 + 5, LEFT);
    app.handle_mouse(mouse(10, 1, LEFT, MouseAction::Button));
    app.show_dialog(super::Dialog::error("x", vec!["y".to_string()]));
    app.tick();
    assert!(app.mouse.hold.is_none());
    app.overlay = None;
    app.handle_mouse(mouse(10, 1, LEFT, MouseAction::Button));
    assert!(app.mouse.hold.is_some());
    app.focus_changed();
    assert!(app.mouse.hold.is_none());
}

#[test]
fn the_wheel_leaves_the_files_behind_an_active_info_panel() {
    let (_temp, mut app) = app_with(30);
    app.info_panel = Some(super::InfoPanel::showing(
        Side::Right,
        super::InfoSections::default(),
        None,
    ));
    app.active = Side::Right;
    assert!(app.info_is_active());
    assert!(!app.handle_mouse(mouse(10, 5, NONE, MouseAction::Wheel(-1))));
}

/// A panel key waits for its plugin (FAR's `ProcessPanelInput` is synchronous): the mouse leaves the
/// panels alone until it answers.
#[test]
fn the_mouse_waits_with_the_keys_for_a_plugins_panel_key() {
    let (_temp, mut app) = app_with(30);
    app.plugins.hold_panel_key_for_test();
    assert!(!click(&mut app, 60, 2 + 3, LEFT));
    assert_eq!(app.active, Side::Left, "nothing moved");
    assert!(!app.handle_mouse(mouse(10, 5, NONE, MouseAction::Wheel(-2))));
    assert_eq!(app.panels[0].cursor, 0);
}
