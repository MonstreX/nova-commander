//! Shift+F2's "Save file as": FAR's dialog, the signature open for the Unicode pages, a save converted
//! to another code page, signature and line breaks, a save as it is, the editor in the new code page.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::dialog::DialogOutcome;
use crate::app::fileops::tests::{app_in, submit_dialog};

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

fn editing(bytes: &[u8]) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("in.txt");
    std::fs::write(&path, bytes).unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    (temp, app)
}

fn dialog(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::EditorSaveAs,
        })) => form,
        _ => panic!("the dialog"),
    }
}

/// The line of code page `page` in the list.
fn line_of(app: &App, page: u32) -> usize {
    let (_, pages) = pages(&app.code_pages);
    pages.iter().position(|it| *it == Some(page)).unwrap()
}

/// `name`, code page `page`, the signature, the line break radio `breaks` (0: do not change), Save.
fn save_as(app: &mut App, name: &str, page: u32, (bom, breaks): (bool, usize)) {
    let line = line_of(app, page);
    press(app, KeyCode::F(2), KeyModifiers::SHIFT);
    let form = dialog(app);
    if let Some(Control::Edit(input)) = form.control_mut(NAME) {
        *input = LineInput::with_text(name);
    }
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(CODE_PAGE) {
        *chosen = line;
    }
    if let Some(Control::Check { checked, .. }) = form.control_mut(SIGNATURE) {
        *checked = bom;
    }
    for at in 0..4 {
        if let Some(Control::Radio { checked, .. }) = form.control_mut(KEEP_BREAKS + at) {
            *checked = at == breaks;
        }
    }
    submit_dialog(app, DialogOutcome::Button(SAVE));
    settle(app);
}

#[test]
fn the_dialog_is_far_s_and_the_signature_is_for_the_unicode_pages() {
    let (_temp, mut app) = editing(b"\xEF\xBB\xBFone\r\n");
    let utf8 = line_of(&app, 65001);
    let ansi = line_of(&app, crate::platform::locale::ansi_code_page());
    press(&mut app, KeyCode::F(2), KeyModifiers::SHIFT);
    let form = dialog(&mut app);
    assert_eq!(form.title, "Editor");
    assert_eq!(form.chosen(CODE_PAGE), utf8, "the file's code page");
    assert!(form.is_checked(SIGNATURE), "it has a signature");
    assert!(form.is_checked(KEEP_BREAKS));
    assert_eq!(form.items[NAME].history.as_ref().unwrap().name, "NewEdit");
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(CODE_PAGE) {
        *chosen = ansi;
    }
    let mut taken = app.overlay.take();
    let Some(Overlay::Dialog(dialog)) = taken.as_mut() else {
        panic!("the dialog");
    };
    follow(dialog, &app.code_pages);
    let Dialog::Form { form, .. } = dialog else {
        panic!("the form");
    };
    assert!(
        form.items[SIGNATURE].disabled && !form.is_checked(SIGNATURE),
        "ANSI: none"
    );
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(CODE_PAGE) {
        *chosen = utf8;
    }
    follow(dialog, &app.code_pages);
    let Dialog::Form { form, .. } = dialog else {
        panic!("the form");
    };
    assert!(
        !form.items[SIGNATURE].disabled && form.is_checked(SIGNATURE),
        "FAR's AddUnicodeBOM"
    );
}

/// To UTF-16 LE with its signature and Unix line breaks; the editor then in UTF-16 LE.
#[test]
fn a_save_as_converts_the_code_page_the_signature_and_the_line_breaks() {
    let (temp, mut app) = editing(b"\xEF\xBB\xBFa\r\n\xC3\xA9\r");
    save_as(&mut app, "out.txt", 1200, (true, 2));
    let out = std::fs::read(temp.path().join("out.txt")).unwrap();
    let mut wanted = vec![0xFF, 0xFE];
    wanted.extend("a\n\u{e9}\n".encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(out, wanted);
    let Screen::Editor(editor) = &app.screen else {
        panic!("the editor");
    };
    assert_eq!(editor.path, temp.path().join("out.txt"));
    assert_eq!(editor.text.page_number(), Some(1200));
    assert_eq!(
        std::fs::read(temp.path().join("in.txt")).unwrap(),
        b"\xEF\xBB\xBFa\r\n\xC3\xA9\r",
        "the source as it was"
    );
}

/// The same code page, signature and line breaks: the bytes as they are; no signature asked: it goes.
#[test]
fn a_save_as_without_changes_keeps_the_bytes_and_one_without_the_signature_drops_it() {
    let (temp, mut app) = editing(b"\xEF\xBB\xBFx\r\ny");
    save_as(&mut app, "same.txt", 65001, (true, 0));
    assert_eq!(
        std::fs::read(temp.path().join("same.txt")).unwrap(),
        b"\xEF\xBB\xBFx\r\ny"
    );
    save_as(&mut app, "plain.txt", 65001, (false, 0));
    assert_eq!(
        std::fs::read(temp.path().join("plain.txt")).unwrap(),
        b"x\r\ny"
    );
}
