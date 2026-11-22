use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;

const TEXT: &str = "#include <stdio.h>\r\nint one(void) {\r\n  return two();\r\n}\r\nint two(void) { return 2; }\r\n";
/// Where the lines start.
const CALL: u64 = 37;
const TWO: u64 = 57;

fn editing() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("x.c");
    std::fs::write(&path, TEXT).unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&path, false);
    until(
        &mut app,
        |app| matches!(&app.screen, Screen::Editor(editor) if editor.text.highlighted()),
    );
    (temp, app)
}

fn until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.tick();
        if let Screen::Editor(editor) = &mut app.screen {
            editor.text.room.set((40, 5));
        }
        let settled = matches!(&app.screen, Screen::Editor(editor)
            if editor.text.settled() && editor.pending.is_none());
        if settled && done(app) {
            return;
        }
        assert!(Instant::now() < deadline, "never came");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn cursor(app: &App) -> (u64, usize) {
    match &app.screen {
        Screen::Editor(editor) => editor.text.view().unwrap().cursor(),
        _ => panic!("the editor"),
    }
}

fn place(app: &mut App, line: u64, pos: usize) {
    if let Screen::Editor(editor) = &mut app.screen {
        editor.text.press(Step::Place(Place {
            line: Some(line),
            pos: Some(pos),
            tab_pos: None,
            top: None,
            left: None,
            overtype: None,
        }));
    }
    until(app, |_| true);
}

fn menu(app: &App) -> (String, Vec<String>) {
    match &app.overlay {
        Some(Overlay::PluginMenu(menu)) => (
            menu.title.clone(),
            menu.lines.iter().map(|line| line.text.clone()).collect(),
        ),
        _ => panic!("the outliner"),
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    until(app, |_| true);
}

#[test]
fn the_outliner_lists_narrows_goes_and_esc_puts_the_cursor_back() {
    let (_temp, mut app) = editing();
    app.colorer_outline(Action::Outliner);
    until(&mut app, |app| app.overlay.is_some());
    let (title, lines) = menu(&app);
    assert_eq!(title, "Outliner []");
    assert!(lines.iter().any(|line| line.contains("one")), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.starts_with("   5 ")),
        "{lines:?}"
    );
    press(&mut app, KeyCode::Char('w'));
    let (title, lines) = menu(&app);
    assert!(title.starts_with("Outliner [w"), "{title}");
    assert!(lines.iter().all(|line| line.contains('w')), "{lines:?}");
    press(&mut app, KeyCode::Backspace);
    assert!(menu(&app).0.starts_with("Outliner ["));
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL));
    until(&mut app, |_| true);
    assert_ne!(cursor(&app), (0, 0), "the editor moved to the item");
    press(&mut app, KeyCode::Esc);
    assert_eq!(cursor(&app), (0, 0), "Esc: back where it was");
    app.colorer_outline(Action::Outliner);
    until(&mut app, |app| app.overlay.is_some());
    let at = menu(&app)
        .1
        .iter()
        .position(|line| line.contains("two"))
        .unwrap();
    if let Some(Overlay::PluginMenu(shown)) = &mut app.overlay {
        shown.selected = at;
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(cursor(&app).0, TWO, "on two's line");
}

#[test]
fn find_function_goes_to_the_word_s_item_elsewhere() {
    let (_temp, mut app) = editing();
    place(&mut app, CALL, 10);
    app.colorer_outline(Action::FindFunction);
    until(&mut app, |_| true);
    assert_eq!(cursor(&app).0, TWO, "two's definition, not its call");
    // FarColorer's loop: past the line's end there is no word - "Nothing found".
    place(&mut app, CALL, 20);
    app.colorer_outline(Action::FindFunction);
    until(&mut app, |_| true);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "past the end: Nothing found"
    );
}
