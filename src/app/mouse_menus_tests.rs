//! The mouse in menus and lists (`mouse_menus`).

use ratatui::crossterm::event::KeyModifiers;

use super::console_input::{Mouse, MouseAction};
use super::fileops::tests::app_in;
use super::menu::{MenuState, title_positions};
use super::{App, Cells, Command, MenuArea, Overlay, Side};
use crate::platform::input::MouseButtons;

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
/// The box the tests put the menu in: items from row 6, columns 11 to 38.
const FRAME: Cells = Cells {
    x: 10,
    y: 5,
    width: 30,
    height: 14,
};

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

/// The sort menu of the left panel, as if drawn in `FRAME`.
fn sort_menu() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.execute(Command::SortMenu(Side::Left));
    assert!(matches!(app.overlay, Some(Overlay::Popup(_))));
    app.mouse.menu = Some(MenuArea {
        frame: FRAME,
        outer: FRAME,
        first: 0,
    });
    (temp, app)
}

fn selected(app: &App) -> usize {
    match &app.overlay {
        Some(Overlay::Popup(popup)) => popup.selected,
        _ => panic!("no menu"),
    }
}

fn item_row(index: usize) -> u16 {
    FRAME.y + 1 + u16::try_from(index).unwrap()
}

#[test]
fn the_item_under_the_mouse_is_selected_as_it_moves() {
    let (_temp, mut app) = sort_menu();
    assert!(app.handle_mouse(mouse(20, item_row(2), NONE, MouseAction::Moved)));
    assert_eq!(selected(&app), 2);
    assert!(
        !app.handle_mouse(mouse(21, item_row(2), NONE, MouseAction::Moved)),
        "the same item: no redraw"
    );
}

#[test]
fn a_button_pressed_and_let_go_on_an_item_chooses_it() {
    let (_temp, mut app) = sort_menu();
    let before = app.panels[0].sort;
    app.handle_mouse(mouse(20, item_row(1), LEFT, MouseAction::Button));
    assert!(app.overlay.is_some(), "not on the way down");
    app.handle_mouse(mouse(20, item_row(1), NONE, MouseAction::Button));
    assert!(app.overlay.is_none());
    assert_ne!(app.panels[0].sort, before, "the second sort mode");
}

#[test]
fn a_release_with_no_press_in_the_menu_chooses_nothing() {
    let (_temp, mut app) = sort_menu();
    app.handle_mouse(mouse(20, item_row(1), NONE, MouseAction::Button));
    assert!(app.overlay.is_some());
}

#[test]
fn a_click_outside_closes_and_the_middle_button_outside_chooses() {
    let (_temp, mut app) = sort_menu();
    app.handle_mouse(mouse(70, 2, LEFT, MouseAction::Button));
    assert!(app.overlay.is_none());
    let (_temp, mut app) = sort_menu();
    let before = app.panels[0].sort;
    app.handle_mouse(mouse(20, item_row(1), NONE, MouseAction::Moved));
    app.handle_mouse(mouse(70, 2, MIDDLE, MouseAction::Button));
    assert!(app.overlay.is_none());
    assert_ne!(app.panels[0].sort, before);
}

#[test]
fn the_wheel_moves_by_one_and_stops_at_the_ends() {
    let (_temp, mut app) = sort_menu();
    app.handle_mouse(mouse(20, item_row(0), NONE, MouseAction::Moved));
    assert!(!app.handle_mouse(mouse(20, item_row(0), NONE, MouseAction::Wheel(1))));
    assert_eq!(selected(&app), 0, "no wrap at the top");
    app.handle_mouse(mouse(20, item_row(0), NONE, MouseAction::Wheel(-2)));
    assert_eq!(selected(&app), 2);
}

#[test]
fn a_button_on_the_bottom_edge_moves_down_until_let_go() {
    let (_temp, mut app) = sort_menu();
    let bottom = FRAME.y + FRAME.height - 1;
    app.handle_mouse(mouse(20, bottom, LEFT, MouseAction::Button));
    assert_eq!(selected(&app), 1);
    assert!(app.mouse.menu_hold.is_some());
    app.handle_mouse(mouse(20, bottom, NONE, MouseAction::Button));
    assert!(app.mouse.menu_hold.is_none());
    assert!(app.overlay.is_some(), "an edge is no item");
}

#[test]
fn the_menu_bar_opens_the_menu_under_the_mouse() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.overlay = Some(Overlay::Menu(MenuState {
        menu: 0,
        item: 0,
        no_tree: false,
        provider_modes: [0; 2],
    }));
    app.mouse.menu = Some(MenuArea {
        frame: FRAME,
        outer: FRAME,
        first: 0,
    });
    let third = title_positions()[2];
    assert!(app.handle_mouse(mouse(third + 1, 0, NONE, MouseAction::Moved)));
    assert!(matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == 2));
    assert!(!app.handle_mouse(mouse(third + 2, 0, NONE, MouseAction::Moved)));
}

/// The list under the command line for "zz", as if drawn in `FRAME`; the row of "zz-one".
fn completion_list() -> (tempfile::TempDir, App, usize) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
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
    app.mouse.menu = Some(MenuArea {
        frame: FRAME,
        outer: FRAME,
        first: 0,
    });
    let items = &app.completion.as_ref().unwrap().items;
    let index = items
        .iter()
        .position(
            |item| matches!(item, super::CompletionItem::Text { text, .. } if text == "zz-one"),
        )
        .unwrap();
    (temp, app, index)
}

#[test]
fn the_list_under_the_command_line_puts_the_item_under_the_mouse_on_the_line() {
    let (_temp, mut app, index) = completion_list();
    // A move outside does nothing.
    assert!(!app.handle_mouse(mouse(70, 2, NONE, MouseAction::Moved)));
    app.handle_mouse(mouse(20, item_row(index), NONE, MouseAction::Moved));
    assert_eq!(app.command_line.text(), "zz-one");
}

#[test]
fn the_list_under_the_command_line_closes_on_a_click_outside_or_the_right_button() {
    let (_temp, mut app, _) = completion_list();
    assert!(app.handle_mouse(mouse(70, 2, LEFT, MouseAction::Button)));
    assert!(app.completion.is_none(), "FAR's LBtnClick: cancel");
    let (_temp, mut app, index) = completion_list();
    app.handle_mouse(mouse(20, item_row(index), RIGHT, MouseAction::Button));
    assert!(app.completion.is_none(), "FAR's default: closes");
    assert_eq!(
        app.command_line.text(),
        "zz",
        "nothing is run or put on the line"
    );
}

#[test]
fn the_mouse_passes_over_the_panel_modes_separator() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.panel_modes
        .add(0, crate::settings::PanelMode::default());
    app.execute(Command::PanelModes);
    app.mouse.menu = Some(MenuArea {
        frame: FRAME,
        outer: FRAME,
        first: 0,
    });
    let selected = |app: &App| match &app.overlay {
        Some(Overlay::EditList(list)) => list.selected,
        _ => panic!("no list"),
    };
    assert!(app.handle_mouse(mouse(20, item_row(7), NONE, MouseAction::Moved)));
    assert_eq!(selected(&app), 7, "the user's mode");
    app.handle_mouse(mouse(20, item_row(6), NONE, MouseAction::Moved));
    assert_eq!(selected(&app), 7, "the separator is not chosen");
}

#[test]
fn the_mouse_passes_over_the_drive_menus_separator_onto_the_plugins_items() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let drive = |letter: char| crate::vfs::local::Drive {
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
    };
    let item = |index: usize| super::DiskItem {
        plugin: 0,
        index,
        label: format!("Item {index}"),
        hotkey: None,
    };
    app.overlay = Some(Overlay::DriveMenu(super::DriveMenu {
        side: Side::Left,
        drives: vec![drive('C'), drive('D')],
        plugins: vec![item(0), item(1)],
        selected: 1,
        columns: crate::vfs::local::DriveColumns::default(),
        top: std::cell::Cell::new(0),
    }));
    app.mouse.menu = Some(MenuArea {
        frame: FRAME,
        outer: FRAME,
        first: 0,
    });
    let selected = |app: &App| match &app.overlay {
        Some(Overlay::DriveMenu(menu)) => menu.selected,
        _ => panic!("no drive menu"),
    };
    app.handle_mouse(mouse(20, item_row(2), NONE, MouseAction::Moved));
    assert_eq!(selected(&app), 1, "the separator is not chosen");
    app.handle_mouse(mouse(20, item_row(2), LEFT, MouseAction::Button));
    app.handle_mouse(mouse(20, item_row(2), NONE, MouseAction::Button));
    assert_eq!(selected(&app), 1, "a click on it chooses nothing");
    app.handle_mouse(mouse(20, item_row(1), NONE, MouseAction::Wheel(-1)));
    assert_eq!(selected(&app), 3, "the wheel passes it");
    app.handle_mouse(mouse(20, item_row(3), NONE, MouseAction::Wheel(-5)));
    assert_eq!(selected(&app), 4, "down to the last item, past the drives");
    app.handle_mouse(mouse(20, item_row(3), NONE, MouseAction::Moved));
    assert_eq!(selected(&app), 3, "an item under the mouse");
}
