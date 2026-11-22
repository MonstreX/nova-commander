//! The editor's replace: Ctrl+F7's dialog, FAR's question for each match - Replace, Skip, All.

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

fn rows(app: &App) -> Vec<String> {
    match &app.screen {
        Screen::Editor(editor) => editor.text.screen(40, 5).unwrap().rows,
        _ => panic!("no editor"),
    }
}

fn asked(app: &App) -> bool {
    matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::EditorReplaceAsk { .. },
            ..
        }))
    )
}

#[test]
fn ctrl_f7_asks_for_each_match_and_all_replaces_the_rest() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "a cat, a cat\r\na cat\r\ncat").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    app.viewer_search.text = "cat".to_string();
    app.viewer_search.replace = "dog".to_string();
    press(&mut app, KeyCode::F(7), KeyModifiers::CONTROL);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "the dialog"
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert!(asked(&app), "the first match's question");
    // Replace.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(rows(&app)[0], "a dog, a cat");
    assert!(asked(&app), "the next one's");
    // Skip (the third button).
    for _ in 0..2 {
        press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(rows(&app)[0], "a dog, a cat", "skipped");
    assert!(asked(&app));
    // All: the rest without asking.
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(rows(&app), ["a dog, a cat", "a dog", "dog"]);
    assert!(app.overlay.is_none());
}

/// R7 review F2, F3: All over a document of several blocks with thousands of matches, asked right
/// after it opened: every match replaced at its place (not at offsets of a copy without the ones still
/// waiting for their blocks), none dropped, one undo step.
#[test]
fn all_replaces_thousands_of_matches_across_blocks_as_one_step() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("big.log");
    let text = "ERROR x\r\n".repeat(20_000);
    std::fs::write(&path, &text).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    app.viewer_search.text = "ERROR".to_string();
    app.viewer_search.replace = "E".to_string();
    press(&mut app, KeyCode::F(7), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert!(asked(&app), "the first match's question");
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert!(app.overlay.is_none());
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    let replaced = std::fs::read_to_string(&path).unwrap();
    assert!(
        replaced == text.replace("ERROR", "E"),
        "every match at its place"
    );
    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert!(
        std::fs::read_to_string(&path).unwrap() == text,
        "one undo step"
    );
}
