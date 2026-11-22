//! Dragging files between the panels (`mouse_drag`).

use ratatui::crossterm::event::KeyModifiers;

use super::console_input::{Mouse, MouseAction};
use super::dialog::FormAction;
use super::transfer_dialog::Operation;
use super::{App, Dialog, ListArea, Overlay, PanelArea, Side};
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
const BOTH: MouseButtons = MouseButtons {
    left: true,
    right: true,
    middle: false,
};
/// The rows on the 100 x 30 screen of the tests: "..", "sub", then the files.
const SUB_ROW: u16 = 3;
const FIRST_FILE_ROW: u16 = 4;

fn app_with_panels() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    for name in ["a.txt", "b c.txt", "d.txt"] {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    let mut app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    for (index, x) in [0u16, 50].into_iter().enumerate() {
        app.mouse.areas[index] = Some(PanelArea {
            x,
            y: 0,
            width: 50,
            height: 28,
            list: Some(ListArea {
                top: 2,
                rows: 23,
                stripe_lefts: vec![x + 1, x + 25],
            }),
        });
        app.panels[index].set_view_metrics(23, 2);
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

fn press(app: &mut App, column: u16, row: u16) {
    app.handle_mouse(mouse(column, row, LEFT, MouseAction::Button));
}

fn drag_to(app: &mut App, column: u16, row: u16) {
    let mut moved = mouse(column, row, LEFT, MouseAction::Moved);
    moved.pressed = NONE;
    app.handle_mouse(moved);
}

fn release(app: &mut App, column: u16, row: u16) {
    app.handle_mouse(mouse(column, row, NONE, MouseAction::Button));
}

/// The operation of the dialog that came up, what it takes and the target it offers.
pub(super) fn asked(app: &App) -> (Operation, Vec<String>, String) {
    let Some(Overlay::Dialog(Dialog::Form {
        form,
        action: FormAction::Transfer {
            sources, operation, ..
        },
    })) = &app.overlay
    else {
        panic!("no F5 or F6 dialog");
    };
    let target = form
        .items
        .iter()
        .find_map(|item| match &item.control {
            super::form::Control::Edit(input) => Some(input.text().to_string()),
            _ => None,
        })
        .unwrap();
    let names = sources.iter().map(ToString::to_string).collect();
    (*operation, names, target)
}

#[test]
fn a_file_dropped_on_the_other_panel_asks_to_copy_it_there() {
    let (temp, mut app) = app_with_panels();
    let other = temp.path().join("sub");
    let _ = app.panels[1].change_dir(&crate::vfs::VfsPath::local(&other), None);
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    release(&mut app, 60, 10);
    let (operation, sources, target) = asked(&app);
    assert_eq!(operation, Operation::Copy);
    assert_eq!(sources.len(), 1);
    assert!(sources[0].ends_with("a.txt"), "{sources:?}");
    assert!(target.contains("sub"), "{target}");
    assert_eq!(app.active, Side::Left, "the source's F5");
}

#[test]
fn dropped_on_a_folder_it_goes_into_that_folder() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, SUB_ROW);
    release(&mut app, 60, SUB_ROW);
    let (_, _, target) = asked(&app);
    assert!(target.contains("sub"), "FAR's ToSubdir: {target}");
}

#[test]
fn shift_starts_a_move_and_the_right_button_swaps_it() {
    let (_temp, mut app) = app_with_panels();
    let mut down = mouse(10, FIRST_FILE_ROW, LEFT, MouseAction::Button);
    down.modifiers = KeyModifiers::SHIFT;
    app.handle_mouse(down);
    drag_to(&mut app, 60, 10);
    assert!(
        app.mouse
            .drag_text
            .as_ref()
            .unwrap()
            .text
            .starts_with("Move")
    );
    let mut right = mouse(60, 10, BOTH, MouseAction::Button);
    right.pressed = MouseButtons {
        left: false,
        right: true,
        middle: false,
    };
    app.handle_mouse(right);
    drag_to(&mut app, 61, 10);
    assert!(
        app.mouse
            .drag_text
            .as_ref()
            .unwrap()
            .text
            .starts_with("Copy")
    );
    release(&mut app, 61, 10);
    assert_eq!(asked(&app).0, Operation::Copy);
}

#[test]
fn the_note_names_the_file_quoted_or_counts_the_selected_ones() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW + 1);
    drag_to(&mut app, 60, 10);
    let note = app.mouse.drag_text.clone().unwrap();
    assert_eq!(note.text, "Copy \"b c.txt\"");
    assert_eq!((note.column, note.row), (60, 10));
    release(&mut app, 60, 2);
    app.overlay = None;
    for name in ["a.txt", "d.txt"] {
        app.panels[0].selected.insert(name.to_string());
    }
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 95, 10);
    let note = app.mouse.drag_text.clone().unwrap();
    assert_eq!(note.text, "Copy 2 files");
    assert_eq!(
        usize::from(note.column) + note.text.len(),
        99,
        "kept on screen"
    );
}

#[test]
fn within_the_same_panel_the_cursor_follows_and_past_15_columns_comes_back() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 12, FIRST_FILE_ROW + 2);
    assert_eq!(app.panels[0].current().unwrap().name, "d.txt");
    assert!(app.mouse.drag_text.is_none());
    drag_to(&mut app, 30, FIRST_FILE_ROW + 2);
    assert_eq!(
        app.panels[0].current().unwrap().name,
        "a.txt",
        "FAR's GoToFile: what is dragged"
    );
    assert!(app.mouse.drag_text.is_some());
    release(&mut app, 30, FIRST_FILE_ROW + 2);
    assert!(app.overlay.is_none(), "not on its own panel");
    assert!(app.mouse.drag_text.is_none());
}

#[test]
fn going_over_a_panels_frame_ends_the_drag() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    drag_to(&mut app, 60, 27);
    assert!(app.mouse.drag.is_none());
    release(&mut app, 60, 27);
    assert!(app.overlay.is_none());
}

#[test]
fn the_parent_folder_is_not_dragged_nor_a_panel_as_wide_as_the_screen() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, 2);
    assert!(app.mouse.drag.is_none(), "..");
    release(&mut app, 10, 2);
    app.mouse.areas[0].as_mut().unwrap().width = 100;
    press(&mut app, 10, FIRST_FILE_ROW);
    assert!(app.mouse.drag.is_none());
}

#[test]
fn a_key_ends_the_drag() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    app.handle_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Down,
        KeyModifiers::NONE,
    ));
    assert!(app.mouse.drag.is_none());
    assert!(app.mouse.drag_text.is_none());
}

#[test]
fn a_button_held_over_an_edge_ends_when_the_drag_takes_over() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    // Within 15 columns onto the status line: the edge hold starts.
    drag_to(&mut app, 12, 26);
    assert!(app.mouse.hold.is_some());
    drag_to(&mut app, 60, 10);
    assert!(app.mouse.hold.is_none());
    assert_eq!(
        app.mouse.drag_text.as_ref().unwrap().text,
        "Copy a.txt",
        "what is dragged, the cursor brought back"
    );
    assert_eq!(app.panels[0].current().unwrap().name, "a.txt");
}

#[test]
fn letting_go_during_a_drag_ends_the_edge_hold() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 12, 26);
    release(&mut app, 12, 26);
    assert!(app.mouse.hold.is_none());
    assert!(app.mouse.drag.is_none());
}

#[test]
fn the_wheel_does_not_change_what_is_dragged() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    assert!(!app.handle_mouse(mouse(60, 10, LEFT, MouseAction::Wheel(-1))));
    assert_eq!(app.panels[0].current().unwrap().name, "a.txt");
}

#[test]
fn a_panel_with_nothing_listed_can_be_pressed_and_dropped_on() {
    let (_temp, mut app) = app_with_panels();
    app.panels[1].entries.clear();
    press(&mut app, 60, FIRST_FILE_ROW);
    assert!(app.mouse.drag.is_none());
    release(&mut app, 60, FIRST_FILE_ROW);
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    release(&mut app, 60, 10);
    assert!(asked(&app).1[0].ends_with("a.txt"));
}

#[test]
fn one_selected_file_is_what_goes_with_the_cursor_elsewhere() {
    let (_temp, mut app) = app_with_panels();
    app.panels[0].selected.insert("d.txt".to_string());
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    assert_eq!(app.mouse.drag_text.as_ref().unwrap().text, "Copy d.txt");
    release(&mut app, 60, 10);
    assert!(asked(&app).1[0].ends_with("d.txt"));
}

#[test]
fn over_an_info_panel_there_is_no_note_and_no_drop() {
    let (_temp, mut app) = app_with_panels();
    app.info_panel = Some(super::InfoPanel::showing(
        Side::Right,
        super::InfoSections::default(),
        None,
    ));
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    assert!(app.mouse.drag_text.is_none());
    release(&mut app, 60, 10);
    assert!(app.overlay.is_none());
    assert!(app.mouse.drag.is_none());
}

#[test]
fn a_menu_or_dialog_in_front_ends_the_drag() {
    let (_temp, mut app) = app_with_panels();
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    app.show_dialog(Dialog::error("x", vec!["y".to_string()]));
    drag_to(&mut app, 61, 10);
    assert!(app.mouse.drag.is_none());
    assert!(app.mouse.drag_text.is_none());
}

#[test]
fn without_the_confirmation_a_drop_copies_at_once() {
    let (temp, mut app) = app_with_panels();
    app.prefs.confirm_drag = false;
    let other = temp.path().join("sub");
    let _ = app.panels[1].change_dir(&crate::vfs::VfsPath::local(&other), None);
    press(&mut app, 10, FIRST_FILE_ROW);
    drag_to(&mut app, 60, 10);
    release(&mut app, 60, 10);
    assert!(
        app.overlay.is_none(),
        "no dialog: {:?}",
        app.overlay.is_some()
    );
    let copied = other.join("a.txt");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !copied.exists() {
        assert!(std::time::Instant::now() < deadline, "the copy");
        app.tick();
        std::thread::yield_now();
    }
    assert!(temp.path().join("a.txt").exists(), "copied, not moved");
}
