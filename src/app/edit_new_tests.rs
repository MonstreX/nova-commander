//! Shift+F4's "Editor": FAR's dialog, a file made by its first save, one there opened, a folder not
//! there asked about and made by the save, an empty name named by the time, the code page chosen, the
//! editor's Shift+F4.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::dialog::DialogOutcome;
use crate::app::fileops::tests::{app_in, submit_dialog};

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

/// The dialog in front, its name typed and the code page line `page` chosen, then OK.
fn open(app: &mut App, name: &str, page: usize) {
    press(app, KeyCode::F(4), KeyModifiers::SHIFT);
    let Some(Overlay::Dialog(Dialog::Form { form, action })) = &mut app.overlay else {
        panic!("the dialog");
    };
    assert_eq!(*action, FormAction::EditNew);
    if let Some(Control::Edit(input)) = form.control_mut(NAME) {
        *input = LineInput::with_text(name);
    }
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(CODE_PAGE) {
        *chosen = page;
    }
    submit_dialog(app, DialogOutcome::Button(OK));
}

/// Ticks until the editor has read its file and done its work.
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

fn editor_path(app: &App) -> PathBuf {
    match &app.screen {
        Screen::Editor(editor) => editor.path.clone(),
        _ => panic!("no editor"),
    }
}

#[test]
fn the_dialog_is_far_s() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    press(&mut app, KeyCode::F(4), KeyModifiers::SHIFT);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the dialog");
    };
    assert_eq!(form.title, "Editor");
    assert_eq!(form.focus, NAME);
    let history = form.items[NAME].history.as_ref().unwrap();
    assert!(history.name == "NewEdit" && history.use_last);
    let Some(Control::DropDown { lines, chosen, .. }) = form.control(CODE_PAGE) else {
        panic!("the code pages");
    };
    assert_eq!(*chosen, 0);
    assert_eq!(lines[0], ListLine::Choice("Default".to_string()));
    assert_eq!(
        lines[1],
        ListLine::Choice("Automatic detection".to_string())
    );
    assert_eq!(lines[2], ListLine::Section("System".to_string()));
}

/// A name not there: a new file made by F2 in the panel's folder; one there: that file.
#[test]
fn a_new_file_is_made_by_its_save_and_one_there_is_opened() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("old.txt"), "old").unwrap();
    let mut app = app_in(temp.path());
    open(&mut app, "\"new.txt\"", 0);
    settle(&mut app);
    let path = temp.path().join("new.txt");
    assert_eq!(editor_path(&app), path);
    assert!(!path.exists(), "not before the save");
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    settle(&mut app);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    open(&mut app, "old.txt", 0);
    settle(&mut app);
    assert_eq!(editor_path(&app), temp.path().join("old.txt"));
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert!(!editor.text.is_changed());
}

/// FAR's question about a folder not there; Yes: the save makes it. No: nothing opened.
#[test]
fn a_folder_not_there_is_asked_about_and_made_by_the_save() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app, r"deep\er\n.txt", 0);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, .. })) = &app.overlay else {
        panic!("the question");
    };
    assert_eq!(lines[2], "Continue?");
    submit_dialog(&mut app, DialogOutcome::Button(1));
    assert!(matches!(app.screen, Screen::Panels), "No: nothing");
    open(&mut app, r"deep\er\n.txt", 0);
    submit_dialog(&mut app, DialogOutcome::Button(0));
    settle(&mut app);
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    settle(&mut app);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    let made = temp.path().join("deep").join("er").join("n.txt");
    assert_eq!(std::fs::read_to_string(made).unwrap(), "y");
}

/// An empty name: FAR's `New_File_<date>_<time>.txt` in the panel's folder.
#[test]
fn an_empty_name_is_named_by_the_time() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app, "", 0);
    let path = editor_path(&app);
    assert_eq!(path.parent(), Some(temp.path()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("New_File_") && name.ends_with(".txt") && name.len() == 36,
        "{name}"
    );
}

/// The code page chosen: the file read in it.
#[test]
fn the_file_is_read_in_the_code_page_chosen() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("u.txt"), "abc").unwrap();
    let mut app = app_in(temp.path());
    let (lines, pages) = pages(&app.code_pages);
    let utf8 = (pages.iter())
        .position(|page| *page == Some(Page::Number(65001)))
        .unwrap();
    assert!(matches!(&lines[utf8], ListLine::Choice(text) if text.contains("UTF-8")));
    open(&mut app, "u.txt", utf8);
    settle(&mut app);
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert_eq!(editor.text.page_number(), Some(65001));
}

/// The editor's Shift+F4 asks the same and opens another editor window.
#[test]
fn the_editor_s_shift_f4_opens_another_editor() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::write(temp.path().join("b.txt"), "b").unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&temp.path().join("a.txt"), false);
    settle(&mut app);
    open(&mut app, "b.txt", 0);
    settle(&mut app);
    assert_eq!(editor_path(&app), temp.path().join("b.txt"));
}
