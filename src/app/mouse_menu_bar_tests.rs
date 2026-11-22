use ratatui::crossterm::event::KeyModifiers;

use crate::app::console_input::{Mouse, MouseAction};
use crate::app::dialog::{Dialog, ListAction};
use crate::app::fileops::tests::app_in;
use crate::app::{App, Cells, MenuArea, Overlay};
use crate::platform::input::MouseButtons;

/// The list's box: rows 6 to 13 inside, the bar in column 39 (`▲` on row 6, `▼` on row 13).
const FRAME: Cells = Cells {
    x: 10,
    y: 5,
    width: 30,
    height: 10,
};

fn record(app: &mut App, (column, row): (u16, u16), left: bool, action: MouseAction) {
    let buttons = MouseButtons {
        left,
        right: false,
        middle: false,
    };
    app.handle_mouse(Mouse {
        column,
        row,
        buttons,
        pressed: buttons,
        action,
        modifiers: KeyModifiers::NONE,
    });
}

fn click(app: &mut App, at: (u16, u16)) {
    record(app, at, true, MouseAction::Button);
    record(app, at, false, MouseAction::Button);
}

/// A list of 30 lines, as if drawn in `FRAME`.
fn list() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.show_dialog(Dialog::List {
        title: "Themes".to_string(),
        items: (0..30).map(|n| format!("line {n}")).collect(),
        cursor: 0,
        top: Default::default(),
        action: ListAction::Theme,
    });
    drawn(&mut app);
    (temp, app)
}

fn drawn(app: &mut App) {
    app.mouse.menu = Some(MenuArea {
        frame: FRAME,
        outer: FRAME,
        first: 0,
    });
}

fn cursor(app: &App) -> usize {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::List { cursor, .. })) => *cursor,
        _ => panic!("the list is up"),
    }
}

#[test]
fn the_arrows_move_one_line_and_the_field_drags_the_selection_wherever_the_mouse_goes() {
    let (_temp, mut app) = list();
    click(&mut app, (39, 13));
    assert_eq!(cursor(&app), 1, "▼");
    click(&mut app, (39, 6));
    assert_eq!(cursor(&app), 0, "▲");
    record(&mut app, (39, 9), true, MouseAction::Button);
    assert_eq!(cursor(&app), 16, "FAR's 29 * (9 - 5) / 7");
    record(&mut app, (55, 12), true, MouseAction::Moved);
    assert_eq!(cursor(&app), 29, "off the bar it still drags");
    record(&mut app, (55, 12), false, MouseAction::Button);
    assert_eq!(cursor(&app), 29);
    assert!(
        !app.mouse.menu_thumb && app.overlay.is_some(),
        "let go: no Enter, no click outside"
    );
}

#[test]
fn without_menu_scroll_bars_the_right_side_is_the_frame() {
    let (_temp, mut app) = list();
    app.prefs.dialogs.menu_scrollbar = false;
    click(&mut app, (39, 9));
    assert_eq!(cursor(&app), 0);
}
