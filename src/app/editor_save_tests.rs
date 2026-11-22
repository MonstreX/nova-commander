//! FAR's questions before the editor saves: overwriting another file, the file changed by another
//! program, a read-only file; F2 of a file not its own yet asking for the name.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        let ready = match &mut app.screen {
            // The file's stamp told too: what "changed by another program" compares with.
            Screen::Editor(editor) => {
                editor.text.room.set((40, 5));
                editor.text.place().is_some() && editor.saving_told.is_none()
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

/// R7 review F5: the file's stamp is looked at off the screen's thread as the editor opens, and told
/// once it is known.
#[test]
fn the_files_stamp_is_told_after_the_editor_opens() {
    let (_temp, app, path) = edited();
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert!(editor.saving_told.is_none(), "told");
    assert_eq!(editor.saving, Saving::of(&path), "as the file is");
}

/// An editor on `e.txt` ("one\r\n") with `x` typed at its start.
fn edited() -> (tempfile::TempDir, App, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "one\r\n").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('x'));
    settle(&mut app);
    (temp, app, path)
}

/// The question in front: its lines.
fn question(app: &App) -> Vec<String> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Confirm { lines, .. })) => lines.clone(),
        _ => panic!("no question"),
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn save_as_onto_another_file_asks_before_overwriting_it() {
    let (temp, mut app, _) = edited();
    let other = temp.path().join("other.txt");
    std::fs::write(&other, "kept").unwrap();
    app.save_as_named("other.txt");
    assert_eq!(question(&app)[1], "already exists");
    press(&mut app, KeyCode::Char('n'));
    settle(&mut app);
    assert_eq!(read(&other), "kept", "No: not saved");
    app.save_as_named("other.txt");
    press(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert_eq!(read(&other), "xone\r\n");
}

#[test]
fn a_file_changed_by_another_program_asks_before_saving() {
    let (_temp, mut app, path) = edited();
    std::fs::write(&path, "changed elsewhere\r\n").unwrap();
    press(&mut app, KeyCode::F(2));
    assert_eq!(
        question(&app),
        ["The file was changed by an external program. Save?"]
    );
    press(&mut app, KeyCode::Char('c'));
    settle(&mut app);
    assert_eq!(read(&path), "changed elsewhere\r\n", "Cancel: not saved");
}

#[test]
fn a_read_only_file_asks_and_yes_takes_the_attribute_off() {
    let (_temp, mut app, path) = edited();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    if let Screen::Editor(editor) = &mut app.screen {
        // The attribute's change is no content's change.
        editor.saving = Saving::of(&path);
    }
    press(&mut app, KeyCode::F(2));
    assert_eq!(question(&app)[1], "is a read-only file");
    press(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert_eq!(read(&path), "xone\r\n");
    assert!(!std::fs::metadata(&path).unwrap().permissions().readonly());
}

/// Review C11 (C1's regression): the editor's save, through its held file, keeps what the file carried
/// besides its content - a named stream, its hidden attribute - and every name of it gets the text.
#[cfg(windows)]
#[test]
fn the_editors_save_keeps_the_files_streams_attributes_and_names() {
    use std::os::windows::fs::MetadataExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "one\r\n").unwrap();
    let stream = PathBuf::from(format!("{}:extra", path.display()));
    std::fs::write(&stream, "kept").unwrap();
    let other = temp.path().join("other name.txt");
    std::fs::hard_link(&path, &other).unwrap();
    let hidden = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&path)
        .status()
        .unwrap();
    assert!(hidden.success());
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('x'));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert!(app.overlay.is_none(), "saved unasked");
    assert_eq!(read(&path), "xone\r\n");
    assert_eq!(read(&other), "xone\r\n", "the other name");
    assert_eq!(read(&stream), "kept", "the stream");
    let attributes = std::fs::metadata(&path).unwrap().file_attributes();
    assert_ne!(attributes & 0x2, 0, "hidden");
}

/// `path` replaced by another file with `text` (another program's save through a temporary file).
fn replaced(path: &Path, text: &str) {
    let other = path.with_extension("new");
    std::fs::write(&other, text).unwrap();
    std::fs::rename(&other, path).unwrap();
}

/// FAR's "changed by an external program": Yes saves the editor's text (the bytes not edited from the
/// file it read, review C2), "Save as" asks for the name.
#[test]
fn a_file_changed_elsewhere_is_saved_on_yes_or_asked_a_name() {
    let (_temp, mut app, path) = edited();
    replaced(&path, "changed elsewhere\r\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert_eq!(read(&path), "xone\r\n");
    replaced(&path, "changed again\r\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Char('a'));
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: crate::app::dialog::FormAction::EditorSaveAs,
            ..
        }))
    ));
    assert_eq!(read(&path), "changed again\r\n");
}

/// The hardening's find: a file rewritten in place by another program (not replaced), Yes saves the
/// editor's text, its bytes not edited taken from what was read (it was "xchang": the typed `x`, then
/// the new file's bytes at the old offsets).
#[test]
fn a_file_rewritten_in_place_saves_the_editors_text_on_yes() {
    let (_temp, mut app, path) = edited();
    std::fs::write(&path, "changed elsewhere\r\n").unwrap();
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert!(app.overlay.is_none(), "saved, no error");
    assert_eq!(read(&path), "xone\r\n");
}

/// A document not read whole, its file rewritten in place: not saved (its bytes not read are gone),
/// the file left as it is.
#[test]
fn a_document_not_read_whole_is_not_saved_over_its_rewritten_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("big.txt");
    std::fs::write(&path, "new text").unwrap();
    let doc = crate::editor::view::pieces::PieceTable::new(8);
    let from = super::super::editor_save_file::Original {
        path: &path,
        held: None,
        in_place: true,
    };
    let written = std::sync::atomic::AtomicU64::new(0);
    let saved = super::super::editor_save_file::save_to(from, (&path, None), &doc, &written);
    let Err(error) = saved else {
        panic!("saved over the rewritten file");
    };
    assert!(error.to_string().contains("rewritten"), "{error}");
    assert_eq!(read(&path), "new text");
    assert_eq!(written.into_inner(), 0);
}

/// R7 review C4: the stamp, the identity and the read-only question are `a `'s, not its sibling `a`'s.
#[cfg(windows)]
#[test]
fn the_save_looks_at_the_exact_name() {
    let temp = tempfile::tempdir().unwrap();
    let exact = |name: &str| crate::platform::files::exact_path(&temp.path().join(name));
    std::fs::write(exact("a"), "the sibling, longer").unwrap();
    std::fs::write(exact("a "), "one\r\n").unwrap();
    let plain = temp.path().join("a ");
    let saving = Saving::of(&plain);
    assert_eq!(saving.stamp.map(|(_, len)| len), Some(5), "`a `'s length");
    let own = crate::platform::files::file_identity(&exact("a ")).ok();
    assert_eq!(saving.identity, own);
    let mut permissions = std::fs::metadata(exact("a ")).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(exact("a "), permissions).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&plain, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('x'));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    assert_eq!(question(&app)[1], "is a read-only file");
    press(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert_eq!(read(&exact("a ")), "xone\r\n");
    assert_eq!(read(&exact("a")), "the sibling, longer");
}

/// The file's stamp is taken again after the editor's own save: the next save asks nothing.
#[test]
fn a_file_saved_is_saved_again_unasked() {
    let (_temp, mut app, path) = edited();
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    press(&mut app, KeyCode::Char('z'));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    assert!(app.overlay.is_none(), "not asked");
    settle(&mut app);
    assert_eq!(read(&path), "xzone\r\n");
}

/// Shift+F2 onto its own name, or a file not there, asks nothing; a file not its own yet asks before
/// overwriting even its own name.
#[test]
fn save_as_asks_to_overwrite_only_another_file() {
    let (temp, mut app, path) = edited();
    app.save_as_named("e.txt");
    assert!(app.overlay.is_none(), "its own file");
    settle(&mut app);
    assert_eq!(read(&path), "xone\r\n");
    app.save_as_named("new.txt");
    assert!(app.overlay.is_none(), "a file not there");
    settle(&mut app);
    assert_eq!(read(&temp.path().join("new.txt")), "xone\r\n");
    if let Screen::Editor(editor) = &mut app.screen {
        editor.saving.ephemeral = true;
    }
    app.save_as_named("new.txt");
    assert_eq!(question(&app)[1], "already exists", "not its own yet");
}

/// Yes to one question goes on to the next: overwriting another file, then its read-only attribute.
#[test]
fn the_questions_come_one_after_another() {
    let (temp, mut app, _) = edited();
    let other = temp.path().join("other.txt");
    std::fs::write(&other, "kept").unwrap();
    let mut permissions = std::fs::metadata(&other).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&other, permissions).unwrap();
    app.save_as_named("other.txt");
    assert_eq!(question(&app)[1], "already exists");
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(question(&app)[1], "is a read-only file", "the next one");
    press(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert_eq!(read(&other), "xone\r\n");
    assert!(!std::fs::metadata(&other).unwrap().permissions().readonly());
}

#[test]
fn f2_of_a_file_not_its_own_yet_asks_for_the_name() {
    let (_temp, mut app, _) = edited();
    if let Screen::Editor(editor) = &mut app.screen {
        editor.saving.ephemeral = true;
    }
    press(&mut app, KeyCode::F(2));
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: crate::app::dialog::FormAction::EditorSaveAs,
            ..
        }))
    ));
}
