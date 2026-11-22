//! F3 / F4 in Find file's results with the file associations (FAR's `OpenFile`, `ProcessLocalFileTypes`):
//! the found file's association for the key runs, its symbols naming the file by its whole path and its
//! folder, and the results come back after it; Ctrl+Shift opens the program's own viewer at once. The
//! results' mouse (a click, the wheel, a double click Go to) and the console's title once it is done.

use super::*;
use crate::settings::{Association, AssociationCommand};

fn on(command: &str) -> AssociationCommand {
    AssociationCommand {
        command: command.to_string(),
        on: true,
    }
}

/// The search of `*.txt` over, the cursor on `sub\b.txt`, `.txt` files associated.
fn on_b(view: &str, alt_view: &str) -> (tempfile::TempDir, App) {
    let (temp, mut app) = app();
    app.config.associations = vec![Association {
        masks: "*.txt".to_string(),
        view: on(view),
        alt_view: on(alt_view),
        ..Association::default()
    }];
    find(&mut app, "*.txt", "");
    let Some(Overlay::FindResults(results)) = &mut app.overlay else {
        panic!("no results");
    };
    results.cursor = (results.lines.iter())
        .position(|line| matches!(line, ResultLine::Found(at) if results.found[*at].entry.name == "b.txt"))
        .unwrap();
    (temp, app)
}

fn settle(app: &mut App) {
    for _ in 0..5 {
        app.tick();
    }
}

#[test]
fn f3_runs_the_found_files_association_with_its_whole_name_and_the_results_come_back() {
    let (temp, mut app) = on_b("set NC_ASSOC=!.!", "set NC_DIR=!\\");
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    settle(&mut app);
    let b = temp.path().join("sub").join("b.txt");
    assert_eq!(
        app.environment.get("NC_ASSOC"),
        Some(b.display().to_string())
    );
    assert!(matches!(app.screen, Screen::Panels), "no viewer");
    assert_eq!(found_names(&app), ["a.txt", "b.txt"], "the results back");
    // Alt+F3's command speaks of the found file's folder, not the panel's.
    press(&mut app, KeyCode::F(3), KeyModifiers::ALT);
    settle(&mut app);
    let folder = format!("{}\\", temp.path().join("sub").display());
    assert_eq!(app.environment.get("NC_DIR"), Some(folder));
}

#[test]
fn ctrl_shift_f3_and_a_file_without_an_association_open_the_viewer() {
    let (temp, mut app) = on_b("set NC_ASSOC=!.!", "");
    press(
        &mut app,
        KeyCode::F(3),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let Screen::Viewer(viewer) = &app.screen else {
        panic!("no viewer");
    };
    assert_eq!(viewer.path, temp.path().join("sub").join("b.txt"));
    assert_eq!(app.environment.get("NC_ASSOC"), None);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.tick();
    // Alt+F3 with no command for it and no external viewer: the program's own (FAR's `FILETYPE_VIEW`).
    press(&mut app, KeyCode::F(3), KeyModifiers::ALT);
    assert!(matches!(app.screen, Screen::Viewer(_)));
}

/// The results' box and list where the drawing reports them (`ui::find_results`'s own test checks
/// that it does); the list's first row.
fn list_top(app: &mut App) -> (u16, u16) {
    use crate::app::{Cells, ControlKind, DialogArea, DialogControl};
    let rows = u16::try_from(results(app).rows).unwrap();
    app.mouse.dialog = Some(DialogArea {
        frame: Cells {
            x: 3,
            y: 2,
            width: 94,
            height: rows + 8,
        },
        controls: vec![DialogControl {
            cells: Cells {
                x: 7,
                y: 4,
                width: 86,
                height: rows,
            },
            index: usize::MAX,
            kind: ControlKind::List,
        }],
    });
    (9, 4)
}

fn click(app: &mut App, column: u16, row: u16, action: crate::app::console_input::MouseAction) {
    use crate::platform::input::MouseButtons;
    let left = MouseButtons {
        left: true,
        right: false,
        middle: false,
    };
    let none = MouseButtons {
        left: false,
        right: false,
        middle: false,
    };
    for buttons in [left, none] {
        app.handle_mouse(crate::app::console_input::Mouse {
            column,
            row,
            buttons,
            pressed: buttons,
            action,
            modifiers: KeyModifiers::NONE,
        });
    }
}

#[test]
fn a_click_puts_the_cursor_on_a_line_the_wheel_moves_it_and_a_double_click_goes_to_the_file() {
    use crate::app::console_input::MouseAction;
    let (temp, mut app) = on_b("", "");
    let (column, top) = list_top(&mut app);
    // Lines: the folder, a.txt, a separator, the sub folder, b.txt.
    click(&mut app, column, top + 2, MouseAction::Button);
    assert_eq!(results(&app).cursor, 4, "not on the separator");
    click(&mut app, column, top + 1, MouseAction::Button);
    assert_eq!(results(&app).cursor, 1);
    app.handle_mouse(crate::app::console_input::Mouse {
        column,
        row: top,
        buttons: crate::platform::input::MouseButtons::default(),
        pressed: crate::platform::input::MouseButtons::default(),
        action: MouseAction::Wheel(-2),
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        results(&app).cursor,
        4,
        "two lines down, past the separator"
    );
    let (column, top) = list_top(&mut app);
    click(&mut app, column, top + 1, MouseAction::DoubleClick);
    assert!(app.overlay.is_none(), "Go to closes the results");
    let panel = app.active_panel();
    assert_eq!(panel.path, temp.path().to_path_buf());
    assert_eq!(panel.current().unwrap().name, "a.txt");
}

#[test]
fn the_console_title_says_the_search_is_done_while_its_results_are_up() {
    let (_temp, mut app) = on_b("", "");
    assert_eq!(
        app.screen_title().as_deref(),
        Some("Search done. Found files: 2, folders: 0")
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.screen_title(), None);
}
