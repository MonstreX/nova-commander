//! Quick view's file contents: read in the background, moved by the keys, following a growing file.

use std::io::Write;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::{App, Showing, Shown};

/// Pumps until the rows of a 20 by 5 room are what `done` waits for.
fn settle(text: &mut FileText, done: impl Fn(&Screen) -> bool) -> Screen {
    text.room.set((20, 5));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        text.pump();
        if let Some(screen) = text.screen(20, 5)
            && done(&screen)
        {
            return screen;
        }
        assert!(
            Instant::now() < deadline,
            "not shown: {:?}",
            text.screen(20, 5)
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

fn file_with(temp: &tempfile::TempDir, text: &str) -> VfsPath {
    let path = temp.path().join("file.txt");
    std::fs::write(&path, text).unwrap();
    VfsPath::local(&path)
}

#[test]
fn a_file_s_first_rows_come() {
    let temp = tempfile::tempdir().unwrap();
    let mut text = FileText::open(file_with(&temp, "one\ntwo\n"));
    let screen = settle(&mut text, |screen| screen.rows.len() == 2);
    assert_eq!(screen.rows, ["one", "two"]);
    assert!(text.failed.is_none());
}

#[test]
fn end_reads_the_last_block_not_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let mut text = FileText::open(file_with(&temp, &numbered(40_000)));
    settle(&mut text, |screen| {
        screen.rows.first().is_some_and(|row| row == "line 1")
    });
    text.press(Move::End);
    let screen = settle(&mut text, |screen| screen.at_end);
    assert_eq!(screen.rows.last().unwrap(), "line 40000");
    text.press(Move::Home);
    settle(&mut text, |screen| screen.rows[0] == "line 1");
}

#[test]
fn a_growing_file_shown_to_its_end_shows_what_was_added() {
    let temp = tempfile::tempdir().unwrap();
    let path = file_with(&temp, "first\n");
    let mut text = FileText::open(path.clone());
    settle(&mut text, |screen| {
        screen.at_end && screen.rows == ["first"]
    });
    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(path.to_string())
        .unwrap();
    log.write_all(numbered(10).as_bytes()).unwrap();
    let screen = settle(&mut text, |screen| {
        screen.rows.last().is_some_and(|row| row == "line 10")
    });
    assert!(screen.at_end, "the end stays on screen");
}

#[test]
fn a_file_that_cannot_be_read_says_why() {
    let temp = tempfile::tempdir().unwrap();
    let mut text = FileText::open(VfsPath::local(temp.path().join("gone.txt")));
    let deadline = Instant::now() + Duration::from_secs(10);
    while text.failed.is_none() {
        text.pump();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(text.screen(20, 5).is_none());
}

#[cfg(windows)]
#[test]
fn a_system_page_decodes_its_bytes() {
    let page = SystemPage::new(1252);
    assert_eq!(page.decode(&[0xE9]), ('é', 1));
    assert_eq!(page.decode(b"a"), ('a', 1));
    let japanese = SystemPage::new(932);
    // "日" in Shift-JIS: a lead byte and its trail.
    assert_eq!(japanese.decode(&[0x93, 0xFA]), ('日', 2));
    let (_, took) = japanese.decode(&[0x93, b'\n']);
    assert_eq!(took, 1, "a lead byte leaves the line break after it");
}

#[test]
fn a_log_started_empty_is_told_again_as_it_grows() {
    let temp = tempfile::tempdir().unwrap();
    let path = file_with(&temp, "");
    let mut text = FileText::open(path.clone());
    settle(&mut text, |screen| screen.at_end);
    std::fs::write(path.to_string(), "привет\n").unwrap();
    let screen = settle(&mut text, |screen| screen.rows.len() == 1);
    assert_eq!(screen.rows, ["привет"], "UTF-8, not the ANSI page");
}

#[test]
fn the_quick_view_s_keys_move_the_file_shown() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), numbered(100)).unwrap();
    let mut app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
    let path = app.active_panel().path.clone();
    app.active_panel_mut()
        .change_dir(&path, Some("a.txt"))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let first_row = |app: &mut App| {
        let Some(Shown {
            what: Showing::File { text, .. },
            ..
        }) = app.quick_view.as_mut().and_then(|view| view.shown.as_mut())
        else {
            panic!("a file shown");
        };
        settle(text, |screen| !screen.rows.is_empty()).rows[0].clone()
    };
    assert_eq!(first_row(&mut app), "line 1");
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(first_row(&mut app), "line 2");
    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    assert_eq!(first_row(&mut app), "line 97", "the last screen of 5 rows");
}

#[cfg(windows)]
#[test]
fn a_file_saved_over_shows_its_new_contents() {
    let temp = tempfile::tempdir().unwrap();
    let path = file_with(&temp, "old text\n");
    let mut text = FileText::open(path.clone());
    settle(&mut text, |screen| screen.rows == ["old text"]);
    // An editor's save: a new file put in its place by a rename.
    let saved = temp.path().join("file.new");
    std::fs::write(&saved, "new text\nmore\n").unwrap();
    std::fs::rename(&saved, path.to_string()).unwrap();
    let screen = settle(&mut text, |screen| screen.rows == ["new text", "more"]);
    assert!(screen.at_end);
}

/// R7 review F9: a file read past 300 ms with nothing to show yet says so (and for how long); once
/// shown, or failed, nothing more.
#[test]
fn a_file_not_read_in_time_says_it_reads() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("f.txt");
    std::fs::write(&path, "text\n").unwrap();
    let mut text = FileText::open(VfsPath::local(&path));
    assert_eq!(text.reading_for(), None, "not yet");
    text.opened = Instant::now() - Duration::from_secs(2);
    let reading = text.reading_for().expect("reading");
    assert!(reading >= Duration::from_secs(2));
    settle(&mut text, |screen| screen.rows == ["text"]);
    assert_eq!(text.reading_for(), None, "shown");
}
