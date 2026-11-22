//! The editor's search: F7 from the cursor, the found marked with the cursor at its start, Shift+F7
//! going on, the question at the end, Alt+F7 back, an edit's text found.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        let ready = match &mut app.screen {
            Screen::Editor(editor) => {
                editor.text.room.set((40, 5));
                editor.text.settled() && editor.pending.is_none()
            }
            _ => true,
        };
        if ready && !app.is_waiting() {
            return;
        }
        assert!(Instant::now() < deadline, "never done");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn place(app: &mut App) -> (u64, usize) {
    match &app.screen {
        Screen::Editor(editor) => editor.text.place().map(|place| (place.0, place.2)).unwrap(),
        _ => panic!("no editor"),
    }
}

#[test]
fn f7_finds_from_the_cursor_and_shift_f7_and_alt_f7_go_on() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "one two\r\nthree two\r\nfour\r\n").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    app.viewer_search = SearchParams {
        text: "two".to_string(),
        ..SearchParams::default()
    };
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(place(&mut app), (1, 5), "the cursor at its start");
    let Screen::Editor(editor) = &app.screen else {
        unreachable!()
    };
    let marks = editor.text.screen(40, 5).unwrap().marks;
    assert_eq!(marks[0], Some((4, 7)), "marked");
    press(&mut app, KeyCode::F(7), KeyModifiers::SHIFT);
    settle(&mut app);
    assert_eq!(place(&mut app), (2, 7));
    press(&mut app, KeyCode::F(7), KeyModifiers::SHIFT);
    settle(&mut app);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "the end's question"
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(place(&mut app), (1, 5), "round from the beginning");
    // An edit's text is found too.
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('Z'), KeyModifiers::SHIFT);
    settle(&mut app);
    app.viewer_search.text = "Z".to_string();
    app.viewer_search.case_sensitive = true;
    press(&mut app, KeyCode::Home, KeyModifiers::CONTROL);
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(place(&mut app).0, 3);
    app.viewer_search.text = "one".to_string();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    settle(&mut app);
    assert_eq!(place(&mut app), (1, 1), "back");
}

/// R7 review F6: the search reads the document's own text, not a file another program put at its path
/// since (replaced, or rewritten in place).
#[test]
fn f7_searches_the_document_not_the_file_put_at_its_path_since() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "one two\r\nfour\r\n").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    let other = temp.path().join("e.new");
    std::fs::write(&other, "xxx yyy\r\nzzzz\r\n").unwrap();
    std::fs::rename(&other, &path).unwrap();
    app.viewer_search.text = "two".to_string();
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert!(app.overlay.is_none(), "found, no question");
    assert_eq!(place(&mut app), (1, 5), "replaced: the document's text");
    std::fs::write(&path, "aaa bbb\r\ncccc\r\n").unwrap();
    press(&mut app, KeyCode::Home, KeyModifiers::CONTROL);
    press(&mut app, KeyCode::F(7), KeyModifiers::SHIFT);
    settle(&mut app);
    assert_eq!(place(&mut app), (1, 5), "rewritten in place: the same");
}
