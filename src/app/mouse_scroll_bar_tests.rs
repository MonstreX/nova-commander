//! The panels' scroll bars with the mouse: their arrows held and their thumb dragged with the left
//! button (the drawing is `ui`'s tests).

use ratatui::crossterm::event::KeyModifiers;

use super::*;
use crate::app::console_input::MouseAction;
use crate::app::mouse::ListArea;
use crate::platform::input::MouseButtons;

const LEFT: MouseButtons = MouseButtons {
    left: true,
    right: false,
    middle: false,
};
const NONE: MouseButtons = MouseButtons {
    left: false,
    right: false,
    middle: false,
};

/// 60 files in panels on an 80 x 20 screen, the scroll bar on.
fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for index in 0..60 {
        std::fs::write(temp.path().join(format!("f{index:02}.txt")), "x").unwrap();
    }
    let mut app = App::new(temp.path(), 80, 20, &crate::settings::Settings::default());
    app.mouse.enabled = true;
    app.active = Side::Left;
    app.prefs.panel_scrollbar = true;
    (temp, app)
}

/// The left panel laid out as the renderer puts it on an 80 x 20 screen: the list from row 2, 15 rows
/// in two stripes; the column of its right frame.
fn draw(app: &mut App) -> u16 {
    app.mouse.areas[0] = Some(PanelArea {
        x: 0,
        y: 0,
        width: 40,
        height: 20,
        list: Some(ListArea {
            top: 2,
            rows: 15,
            stripe_lefts: vec![1, 20],
        }),
    });
    app.panels[0].set_view_metrics(15, 2);
    39
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

#[test]
fn an_arrow_steps_and_the_thumb_takes_the_cursor_along() {
    let (_temp, mut app) = app();
    let x = draw(&mut app);
    let list = app.mouse.areas[0].clone().unwrap().list.unwrap();
    let before = app.panels[0].cursor;
    let bottom = list.top + list.rows - 1;
    app.handle_mouse(mouse(x, bottom, LEFT, MouseAction::Button));
    assert_eq!(app.panels[0].cursor, before + 1, "Down");
    assert!(app.mouse.hold.is_some(), "held: on as a key held down");
    app.handle_mouse(mouse(x, bottom, NONE, MouseAction::Button));
    assert!(app.mouse.hold.is_none());
    // The thumb: the middle of the bar is the middle of the list (FAR's formula).
    let middle = list.top + list.rows / 2;
    app.handle_mouse(mouse(x, middle, LEFT, MouseAction::Button));
    let count = app.panels[0].entries.len();
    let field = usize::from(list.rows - 2);
    let expected = (count - 1) * usize::from(middle - list.top) / field;
    assert_eq!(app.panels[0].cursor, expected);
    // Dragged past the bottom: the last entry; let go: over.
    app.handle_mouse(mouse(x + 5, 19, LEFT, MouseAction::Moved));
    assert_eq!(app.panels[0].cursor, count - 1);
    app.handle_mouse(mouse(x + 5, 19, NONE, MouseAction::Button));
    assert!(app.mouse.thumb.is_none());
    app.handle_mouse(mouse(x, list.top, LEFT, MouseAction::Button));
    assert_eq!(app.panels[0].cursor, count - 2, "Up");
}

#[test]
fn without_the_option_the_frame_is_no_bar() {
    let (_temp, mut app) = app();
    app.prefs.panel_scrollbar = false;
    let x = draw(&mut app);
    let list = app.mouse.areas[0].clone().unwrap().list.unwrap();
    let before = app.panels[0].cursor;
    app.handle_mouse(mouse(
        x,
        list.top + list.rows / 2,
        LEFT,
        MouseAction::Button,
    ));
    assert!(app.mouse.thumb.is_none());
    // The click is the list's (its last stripe at that row), as on a frame without a bar.
    let panel = &app.panels[0];
    let row = usize::from(list.rows / 2);
    let expected = panel.offset + (panel.stripes() - 1) * panel.column_height() + row;
    assert_eq!(panel.cursor, expected.min(panel.entries.len() - 1));
    assert_ne!(panel.cursor, before);
}

#[test]
fn the_trees_bar_is_drawn_and_its_thumb_moves_its_cursor() {
    let temp = tempfile::tempdir().unwrap();
    for index in 0..30 {
        std::fs::create_dir(temp.path().join(format!("d{index:02}"))).unwrap();
    }
    let mut app = App::new(temp.path(), 80, 20, &crate::settings::Settings::default());
    app.mouse.enabled = true;
    app.prefs.panel_scrollbar = true;
    app.active = Side::Right;
    app.show_tree_panel_on(Side::Left);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app
        .tree_panel
        .as_ref()
        .is_none_or(|tree| tree.items.len() < 31)
    {
        app.pump_tree_panel();
        assert!(std::time::Instant::now() < deadline, "the tree's reading");
        std::thread::yield_now();
    }
    let area = PanelArea {
        x: 0,
        y: 0,
        width: 40,
        height: 20,
        list: None,
    };
    app.mouse.areas[0] = Some(area.clone());
    let x = 39;
    let length = usize::from(area.height - 4);
    let bottom_of_bar = area.y + area.height - 4;
    app.handle_mouse(mouse(x, bottom_of_bar - 1, LEFT, MouseAction::Button));
    let tree = app.tree_panel.as_ref().unwrap();
    let field = length - 2;
    let expected = (tree.items.len() - 1) * (length - 2) / field;
    assert_eq!(
        tree.cursor, expected,
        "the thumb's last row: the last folder"
    );
}
