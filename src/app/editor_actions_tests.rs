//! What the editor asks of the app: F2's save beside and in place, the file read again; FAR's question
//! at Esc; Shift+F10; a new file made by its save; the clipboard (Ctrl+Gray + adding to it); Alt+F8.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::*;
use crate::app::Overlay;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

fn editor(app: &mut App) -> &mut EditorScreen {
    match &mut app.screen {
        Screen::Editor(editor) => editor,
        _ => panic!("no editor"),
    }
}

/// Ticks until the editor's file is read and its work done.
fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        let ready = match &mut app.screen {
            Screen::Editor(editor) => {
                editor.text.room.set((40, 5));
                editor.text.place().is_some()
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

fn opened(text: &str) -> (tempfile::TempDir, App, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, text).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    (temp, app, path)
}

#[test]
fn f2_saves_beside_and_in_place_and_the_file_is_read_again() {
    let (_temp, mut app, path) = opened("one\r\ntwo\r\n");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    typed(&mut app, "> ");
    settle(&mut app);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\r\n> two\r\n");
    assert!(
        !editor(&mut app).text.is_changed(),
        "read again: nothing unsaved"
    );
    assert_eq!(
        editor(&mut app).text.place().map(|place| place.0),
        Some(2),
        "the cursor's line"
    );
    let leftovers = std::fs::read_dir(path.parent().unwrap()).unwrap().count();
    assert_eq!(leftovers, 1, "nothing left beside it");
}

#[test]
fn esc_asks_far_s_question_when_edited() {
    let (_temp, mut app, path) = opened("text");
    typed(&mut app, "x");
    settle(&mut app);
    // FAR's F4 as Esc (R7 review D2), the question in warning colours (D10).
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(
        matches!(
            &app.overlay,
            Some(Overlay::Dialog(Dialog::Confirm { danger: true, .. }))
        ),
        "the question"
    );
    // No: closed, the file as it was.
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Panels));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "text");
    let (_temp, mut app, path) = opened("text");
    typed(&mut app, "y");
    settle(&mut app);
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    settle(&mut app);
    assert!(
        matches!(app.screen, Screen::Panels),
        "Shift+F10: saved and closed"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ytext");
}

#[test]
fn a_new_file_is_made_by_its_save() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("new.txt");
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.open_new_file(&path);
    typed(&mut app, "made");
    settle(&mut app);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "made");
}

#[test]
fn alt_f8_goes_to_a_line() {
    let lines: String = (1..=30).map(|n| format!("line {n}\n")).collect();
    let (_temp, mut app, _path) = opened(&lines);
    press(&mut app, KeyCode::F(8), KeyModifiers::ALT);
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    typed(&mut app, "20");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(editor(&mut app).text.place().map(|place| place.0), Some(20));
}

/// F8: the file read again in the next code page while unedited; an edited one keeps its page.
#[test]
fn f8_reads_the_file_in_the_next_code_page_until_it_is_edited() {
    let (_temp, mut app, _path) = opened("plain");
    let ansi = crate::platform::locale::ansi_code_page();
    assert_eq!(editor(&mut app).text.page_number(), Some(ansi));
    assert_eq!(editor(&mut app).next_page_label(), "OEM");
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    settle(&mut app);
    let oem = crate::platform::locale::oem_code_page();
    assert_eq!(editor(&mut app).text.page_number(), Some(oem));
    typed(&mut app, "x");
    settle(&mut app);
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    settle(&mut app);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "refused: said so"
    );
    assert_eq!(editor(&mut app).text.page_number(), Some(oem));
}

/// S2d.6d: FAR's "Save file position" - the file edited again where it was left, in its code page.
#[test]
fn an_edited_file_opens_again_where_it_was_left() {
    let lines: String = (1..=50).map(|n| format!("line {n}\r\n")).collect();
    let (temp, mut app, path) = opened(&lines);
    app.positions = Some(temp.path().join("positions.json"));
    for _ in 0..12 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    settle(&mut app);
    let page = editor(&mut app).text.page_number();
    for _ in 0..12 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    settle(&mut app);
    let left = editor(&mut app).text.place();
    assert_eq!(left.map(|place| place.0), Some(13));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Panels));
    app.edit_file(&path, false);
    settle(&mut app);
    assert_eq!(editor(&mut app).text.place(), left, "the line and column");
    assert_eq!(editor(&mut app).text.page_number(), page, "the code page");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.editor_settings.save_position = false;
    app.edit_file(&path, false);
    settle(&mut app);
    assert_eq!(editor(&mut app).text.place().map(|place| place.0), Some(1));
}

/// S2d.6d: FAR's read-only files - "Do you wish to edit it?" first, then lock mode (`-` on the status
/// line) that Ctrl+L lets go; Ctrl+F7 does nothing while locked.
#[test]
fn a_read_only_file_is_asked_about_and_opens_locked() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ro.txt");
    std::fs::write(&path, "ab\r\n").unwrap();
    crate::platform::files::set_read_only(&path, true).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.editor_settings.read_only_warn = true;
    app.editor_settings.read_only_lock = true;
    app.edit_file(&path, false);
    assert!(matches!(&app.overlay, Some(Overlay::Dialog(_))), "asked");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Panels), "not opened");
    app.edit_file(&path, false);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert!(editor(&mut app).text.is_locked());
    assert!(editor(&mut app).status(80, 0).contains("│ -│"));
    typed(&mut app, "x");
    press(&mut app, KeyCode::F(7), KeyModifiers::CONTROL);
    settle(&mut app);
    assert!(app.overlay.is_none(), "no replace while locked");
    assert!(!editor(&mut app).text.is_changed(), "not edited");
    press(&mut app, KeyCode::Char('l'), KeyModifiers::CONTROL);
    typed(&mut app, "x");
    settle(&mut app);
    assert!(editor(&mut app).text.is_changed(), "edited once let go");
    crate::platform::files::set_read_only(&path, false).unwrap();
}

/// Shift+F2 to another name: the document's unedited bytes come from the file edited, not from the
/// file saved to (none yet, or another one).
#[test]
fn shift_f2_saves_the_document_under_another_name() {
    let (temp, mut app, path) = opened("one\r\ntwo\r\n");
    std::fs::write(temp.path().join("other.txt"), "something else").unwrap();
    typed(&mut app, "> ");
    settle(&mut app);
    app.save_as_named("new.txt");
    settle(&mut app);
    assert!(app.overlay.is_none(), "no error");
    let new = temp.path().join("new.txt");
    assert_eq!(std::fs::read_to_string(&new).unwrap(), "> one\r\ntwo\r\n");
    assert_eq!(editor(&mut app).path, new);
    app.save_as_named("other.txt");
    // FAR's "already exists. Do you wish to overwrite it?" (`DoNotOverwrite`): Yes.
    assert!(app.overlay.is_some(), "asked");
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    settle(&mut app);
    let other = std::fs::read_to_string(temp.path().join("other.txt")).unwrap();
    assert_eq!(other, "> one\r\ntwo\r\n", "over another file");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\r\ntwo\r\n");
}

/// S2d.6f: the editor's bookmarks by FAR's keys, kept for the file with "Save bookmarks" (the cursor
/// put back by "Save file position" alone).
#[test]
fn bookmarks_are_set_gone_to_and_kept_for_the_file() {
    let lines: String = (1..=30).map(|n| format!("line {n}\r\n")).collect();
    let (temp, mut app, path) = opened(&lines);
    app.positions = Some(temp.path().join("positions.json"));
    for _ in 0..9 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    let right = crate::app::shortcuts::tests::right_ctrl('2');
    app.handle_key(right);
    press(&mut app, KeyCode::Home, KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('2'), KeyModifiers::CONTROL);
    settle(&mut app);
    assert_eq!(editor(&mut app).text.place().map(|p| p.0), Some(10));
    press(&mut app, KeyCode::Home, KeyModifiers::CONTROL);
    settle(&mut app);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.editor_settings.save_position = false;
    app.edit_file(&path, false);
    settle(&mut app);
    assert_eq!(editor(&mut app).text.place().map(|p| p.0), Some(1));
    press(&mut app, KeyCode::Char('2'), KeyModifiers::CONTROL);
    settle(&mut app);
    assert_eq!(
        editor(&mut app).text.place().map(|p| p.0),
        Some(10),
        "the bookmark kept"
    );
}

/// S2d.6e: FAR's undo over a save - the edits before it undone, the text deleted before it back, the
/// document unchanged again when undone to the saved state.
#[test]
fn edits_are_undone_over_a_save() {
    let (_temp, mut app, path) = opened("one two\r\n");
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    typed(&mut app, "!");
    settle(&mut app);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one!\r\n");
    assert!(!editor(&mut app).text.is_changed());
    assert_eq!(
        editor(&mut app).text.place().map(|p| p.2),
        Some(5),
        "the column kept"
    );
    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
    settle(&mut app);
    assert!(editor(&mut app).text.is_changed());
    for _ in 0..4 {
        press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
    }
    settle(&mut app);
    let row = editor(&mut app).text.screen(40, 5).unwrap().rows[0].clone();
    assert_eq!(row, "one two", "back to the text as it was opened");
    for _ in 0..5 {
        press(
            &mut app,
            KeyCode::Char('z'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
    }
    settle(&mut app);
    assert!(
        !editor(&mut app).text.is_changed(),
        "redone to the saved state"
    );
}

/// S2d.6f: Ctrl+F10 in the viewer and the editor (FAR's `GoToFile`): the panel in the file's folder,
/// its cursor on the file, the viewer or the editor left open.
#[test]
fn ctrl_f10_puts_the_panel_onto_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(sub.join(name), "text").unwrap();
    }
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    let on_file = |app: &mut App, name: &str| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.tick();
            let panel = &app.panels[app.active.index()];
            let there = panel.path == crate::vfs::VfsPath::local(&sub);
            if there && panel.current().is_some_and(|entry| entry.name == name) {
                return;
            }
            assert!(Instant::now() < deadline, "not on {name}");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    app.view_file(&sub.join("b.txt"), false);
    press(&mut app, KeyCode::F(10), KeyModifiers::CONTROL);
    on_file(&mut app, "b.txt");
    assert!(
        matches!(app.screen, Screen::Viewer(_)),
        "the viewer left open"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.edit_file(&sub.join("c.txt"), false);
    settle(&mut app);
    press(&mut app, KeyCode::F(10), KeyModifiers::CONTROL);
    on_file(&mut app, "c.txt");
    assert!(matches!(app.screen, Screen::Editor(_)));
}

/// S2d.6f: Ctrl+Q puts the next key's character in as it is (FAR's `"` on the status line meanwhile);
/// Ins makes the cursor FAR's big one.
#[test]
fn ctrl_q_puts_a_control_character_in_and_ins_makes_the_cursor_big() {
    let (_temp, mut app, path) = opened("ab\r\n");
    press(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
    assert!(editor(&mut app).status(80, 0).contains("│ \"│"));
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    settle(&mut app);
    assert!(!editor(&mut app).status(80, 0).contains('"'));
    assert!(!app.block_cursor());
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    settle(&mut app);
    assert!(app.block_cursor(), "overtype");
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read(&path).unwrap(), b"\x01ab\r\n");
}

#[test]
fn ctrl_gray_plus_adds_the_block_to_the_clipboard_s_text() {
    let (_temp, mut app, _path) = opened("ab\ncd\n");
    let marked = |app: &mut App| {
        for _ in 0..2 {
            press(app, KeyCode::Right, KeyModifiers::SHIFT);
        }
        settle(app);
    };
    marked(&mut app);
    press(&mut app, KeyCode::Insert, KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    marked(&mut app);
    // Gray + is the keypad's; the "+" typed above "=" is a character.
    app.handle_key(KeyEvent::new_with_kind_and_state(
        KeyCode::Char('+'),
        KeyModifiers::CONTROL,
        KeyEventKind::Press,
        KeyEventState::KEYPAD,
    ));
    assert_eq!(app.clipboard.text().unwrap().as_deref(), Some("abcd"));
}

#[test]
fn ctrl_bracket_puts_the_panel_s_folder_in_and_ctrl_f_the_file_s_name() {
    let (temp, mut app, path) = opened("ab\r\n");
    press(&mut app, KeyCode::Char('['), KeyModifiers::CONTROL);
    settle(&mut app);
    let folder = crate::app::edit_insert::quote_spaces(
        &crate::app::cmdline_insert::with_end_slash(temp.path()),
    );
    press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
    settle(&mut app);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    let name = path.display().to_string();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("{folder}{name}ab\r\n")
    );
}

#[test]
fn ctrl_ins_with_no_block_copies_the_cursor_s_line() {
    let (_temp, mut app, _path) = opened("ab\r\ncd\r\n");
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Insert, KeyModifiers::CONTROL);
    assert_eq!(app.clipboard.text().unwrap().as_deref(), Some("ab\r\n"));
    assert!(!app.clipboard.is_column().unwrap());
    let view = editor(&mut app).text.view().unwrap();
    assert_eq!(view.block().unwrap(), Some((0, 4)), "the line marked");
    assert_eq!(view.cursor(), (0, 1), "the cursor where it was");
}

#[test]
fn a_vertical_block_goes_to_the_clipboard_as_one_and_comes_back_as_one() {
    let (_temp, mut app, path) = opened("ab\r\ncd\r\n");
    let alt_shift = KeyModifiers::ALT | KeyModifiers::SHIFT;
    press(&mut app, KeyCode::Right, alt_shift);
    press(&mut app, KeyCode::Down, alt_shift);
    settle(&mut app);
    press(&mut app, KeyCode::Insert, KeyModifiers::CONTROL);
    assert_eq!(app.clipboard.text().unwrap().as_deref(), Some("a\r\nc\r\n"));
    assert!(
        app.clipboard.is_column().unwrap(),
        "marked as a vertical block"
    );
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('v'), KeyModifiers::CONTROL);
    settle(&mut app);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read(&path).unwrap(), b"aba\r\ncdc\r\n");
}

/// F2 in the editor opened on `path`, after typing `x` at its start.
#[cfg(windows)]
fn typed_and_saved(app: &mut App, path: &Path) {
    app.edit_file(path, false);
    settle(app);
    typed(app, "x");
    settle(app);
    press(app, KeyCode::F(2), KeyModifiers::NONE);
    settle(app);
    assert!(app.overlay.is_none(), "saved");
}

/// The save as `vfs::local::ops::write_atomic`'s (review C1): the name exactly (`a ` never reaches
/// its sibling `a`), every name of a file with several, its attributes.
#[cfg(windows)]
#[test]
fn a_save_keeps_the_exact_name_every_name_and_the_attributes() {
    use std::os::windows::fs::MetadataExt;
    let temp = tempfile::tempdir().unwrap();
    let exact = |name: &str| crate::platform::files::exact_path(&temp.path().join(name));
    std::fs::write(exact("a"), "sibling").unwrap();
    std::fs::write(exact("a "), "one\r\n").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    typed_and_saved(&mut app, &exact("a "));
    assert_eq!(std::fs::read_to_string(exact("a ")).unwrap(), "xone\r\n");
    assert_eq!(std::fs::read_to_string(exact("a")).unwrap(), "sibling");

    let (linked, other) = (exact("b.txt"), exact("c.txt"));
    std::fs::write(&linked, "two\r\n").unwrap();
    std::fs::hard_link(&linked, &other).unwrap();
    typed_and_saved(&mut app, &linked);
    assert_eq!(std::fs::read_to_string(&other).unwrap(), "xtwo\r\n");

    let hidden = exact("h.txt");
    std::fs::write(&hidden, "three\r\n").unwrap();
    let status = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&hidden)
        .status()
        .unwrap();
    assert!(status.success());
    typed_and_saved(&mut app, &hidden);
    assert_eq!(std::fs::read_to_string(&hidden).unwrap(), "xthree\r\n");
    let attributes = std::fs::metadata(&hidden).unwrap().file_attributes();
    assert_ne!(attributes & 0x2, 0, "still hidden");
    let left = std::fs::read_dir(temp.path()).unwrap().count();
    assert_eq!(left, 5, "no temporary file left");
}

/// R7 review F1: an editor whose file another program holds unshared (nothing read) closes on Esc; it
/// waited for ever.
#[cfg(windows)]
#[test]
fn an_editor_of_a_file_that_cannot_be_read_closes() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("held.txt");
    std::fs::write(&path, "one\r\n").unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    let deadline = Instant::now() + Duration::from_secs(10);
    while editor(&mut app).text.failed.is_none() {
        assert!(Instant::now() < deadline, "the failure never came");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Panels), "closed");
    drop(held);
}

/// R7 review F10: Ctrl+A, Ctrl+C right after a file of several blocks opened copies it whole (the
/// copy done again once the blocks it needs are read; it did nothing).
#[test]
fn a_block_over_blocks_not_read_yet_is_copied_once_they_are() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("big.txt");
    let text = "line of text\r\n".repeat(30_000);
    std::fs::write(&path, &text).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    let deadline = Instant::now() + Duration::from_secs(10);
    while editor(&mut app).text.view().is_none() {
        assert!(Instant::now() < deadline, "never opened");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    while editor(&mut app).pending.is_some() {
        assert!(Instant::now() < deadline, "never copied");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    let copied = app.clipboard.text().unwrap().unwrap_or_default();
    assert!(copied == text, "{} of {} bytes", copied.len(), text.len());
}

/// R7 review D6: F6 in an edited editor asks FAR's "Save?": Cancel stays, No views the file as it
/// was, Yes saves, then views.
#[test]
fn f6_when_edited_asks_to_save_then_views() {
    let (_temp, mut app, path) = opened("text");
    typed(&mut app, "x");
    settle(&mut app);
    let asked = |app: &App| {
        matches!(
            &app.overlay,
            Some(Overlay::Dialog(Dialog::Confirm {
                action: ConfirmAction::EditorToViewer,
                ..
            }))
        )
    };
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    assert!(asked(&app), "the question");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Editor(_)), "Cancel: stays");
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Viewer(_)), "No: the viewer");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "text");
    let (_temp, mut app, path) = opened("text");
    typed(&mut app, "y");
    settle(&mut app);
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert!(
        matches!(app.screen, Screen::Viewer(_)),
        "Yes: saved, viewed"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ytext");
}
