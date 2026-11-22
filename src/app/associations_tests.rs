use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::{Overlay, Screen};

fn on(command: &str) -> AssociationCommand {
    AssociationCommand {
        command: command.to_string(),
        on: true,
    }
}

fn association(masks: &str, description: &str, execute: &str, view: &str) -> Association {
    Association {
        masks: masks.to_string(),
        description: description.to_string(),
        execute: on(execute),
        view: on(view),
        ..Association::default()
    }
}

/// An app on a folder with `notes.txt` under the cursor.
fn app(associations: Vec<Association>) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "text").unwrap();
    let mut app = app_in(temp.path());
    app.config.associations = associations;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.panels[0]
        .entries
        .iter()
        .any(|entry| entry.name == "notes.txt")
    {
        assert!(std::time::Instant::now() < deadline);
        app.tick();
    }
    app.panels[0].go_to_name("notes.txt");
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    for _ in 0..5 {
        app.tick();
    }
}

fn variable(app: &App) -> Option<String> {
    app.environment.get("NC_ASSOC")
}

#[test]
fn only_the_associations_whose_masks_take_the_name_and_have_the_command_count() {
    let mut off = association("*.txt", "Off", "set NC_ASSOC=off", "");
    off.execute.on = false;
    let all = [
        association("*.rs", "Rust", "set NC_ASSOC=rust", ""),
        off,
        association("*.txt|notes.*", "Not notes", "set NC_ASSOC=x", ""),
        association("*.txt,*.md", "Text", "set NC_ASSOC=text", ""),
    ];
    let found = matching(&all, "notes.txt", AssocMode::Execute, &FileMask::parse);
    assert_eq!(
        found,
        [("Text".to_string(), "set NC_ASSOC=text".to_string())]
    );
    assert!(matching(&all, "notes.txt", AssocMode::View, &FileMask::parse).is_empty());
}

#[test]
fn enter_runs_the_one_association_with_the_files_name_and_notes_it() {
    let (_temp, mut app) = app(vec![association("*.txt", "", "set NC_ASSOC=!.!", "")]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(variable(&app).as_deref(), Some("notes.txt"));
    assert_eq!(app.commands().last(), Some("set NC_ASSOC=notes.txt"));
}

#[test]
fn several_associations_ask_which_with_their_descriptions() {
    let (_temp, mut app) = app(vec![
        association("*.txt", "First for !.!", "set NC_ASSOC=one", ""),
        association("*.*", "", "set NC_ASSOC=two", ""),
    ]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::List { title, items, .. })) = &app.overlay else {
        panic!("no choice");
    };
    assert_eq!(title, "Select association");
    assert_eq!(items, &["First for notes.txt", "set NC_ASSOC=two"]);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(variable(&app).as_deref(), Some("two"));
}

#[test]
fn f3_takes_the_view_association_and_without_one_the_viewer() {
    let (_temp, mut app) = app(vec![association("*.txt", "", "", "set NC_ASSOC=viewed")]);
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert_eq!(variable(&app).as_deref(), Some("viewed"));
    assert!(matches!(app.screen, Screen::Panels));
    app.config.associations.clear();
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Viewer(_)));
}

#[test]
fn alt_f4_takes_its_own_association_and_else_opens_the_editor() {
    let mut edit = association("*.txt", "", "", "");
    edit.alt_edit = on("set NC_ASSOC=alt-edit");
    let (_temp, mut app) = app(vec![edit]);
    press(&mut app, KeyCode::F(4), KeyModifiers::ALT);
    assert_eq!(variable(&app).as_deref(), Some("alt-edit"));
    app.config.associations.clear();
    press(&mut app, KeyCode::F(4), KeyModifiers::ALT);
    assert!(matches!(app.screen, Screen::Editor(_)));
}

#[test]
fn ctrl_pgdn_takes_its_own_association() {
    let mut alt = association("*.txt", "", "set NC_ASSOC=enter", "");
    alt.alt_execute = on("set NC_ASSOC=ctrl-pgdn");
    let (_temp, mut app) = app(vec![alt]);
    press(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
    assert_eq!(variable(&app).as_deref(), Some("ctrl-pgdn"));
}

#[test]
fn ctrl_pgdn_without_its_own_association_does_not_take_enters() {
    let (temp, mut app) = app(vec![association("*.txt", "", "set NC_ASSOC=enter", "")]);
    // The file goes before the key: the system's own way of opening it fails with an error instead of
    // starting a program on the desktop.
    std::fs::remove_file(temp.path().join("notes.txt")).unwrap();
    let before = app.commands().count();
    press(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
    assert_eq!(variable(&app), None);
    assert_eq!(app.commands().count(), before, "nothing noted");
    assert!(
        app.overlay.is_none(),
        "FAR: no plugin takes it, nothing more (not opened as Enter, which would tell the file is gone)"
    );
}

#[test]
fn alt_f3_without_its_own_association_opens_the_viewer() {
    let (_temp, mut app) = app(vec![association("*.txt", "", "", "set NC_ASSOC=f3")]);
    press(&mut app, KeyCode::F(3), KeyModifiers::ALT);
    assert_eq!(variable(&app), None, "F3's association is not Alt+F3's");
    assert!(matches!(app.screen, Screen::Viewer(_)));
}

#[test]
fn f4_takes_the_edit_association_and_view_or_edit_ones_are_not_noted() {
    let mut edit = association("*.txt", "", "", "");
    edit.edit = on("set NC_ASSOC=edited");
    let (_temp, mut app) = app(vec![edit]);
    let before = app.commands().count();
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert_eq!(variable(&app).as_deref(), Some("edited"));
    assert_eq!(app.commands().count(), before);
}

#[test]
fn enters_history_entry_is_the_line_as_it_ran_with_its_answers() {
    let (_temp, mut app) = app(vec![association(
        "*.txt",
        "",
        "set NC_ASSOC=!?Value?x!",
        "",
    )]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no question");
    };
    form.set_edit_text(1, "answer");
    let ok = form.items.len() - 2;
    form.focus_item(ok);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(variable(&app).as_deref(), Some("answer"));
    assert_eq!(app.commands().last(), Some("set NC_ASSOC=answer"));
}

/// S2d.6c: FAR's external editor - Alt+F4 runs its command, F4 the program's editor.
#[test]
fn the_external_editor_takes_alt_f4() {
    let (_temp, mut app) = app(Vec::new());
    app.editor_settings.external_command = "set NC_ASSOC=edit-!.!".to_string();
    press(&mut app, KeyCode::F(4), KeyModifiers::ALT);
    assert_eq!(variable(&app).as_deref(), Some("edit-notes.txt"));
    assert!(matches!(app.screen, Screen::Panels));
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(
        matches!(app.screen, Screen::Editor(_)),
        "F4: the program's editor"
    );
}

/// S2d.6b: FAR's external viewer - Alt+F3 runs its command, F3 the program's viewer; "Use external
/// viewer for F3" swaps them; no command, no external viewer.
#[test]
fn the_external_viewer_takes_alt_f3_or_f3_when_asked() {
    let (_temp, mut app) = app(Vec::new());
    app.viewer_settings.external_command = "set NC_ASSOC=!.!".to_string();
    press(&mut app, KeyCode::F(3), KeyModifiers::ALT);
    assert_eq!(variable(&app).as_deref(), Some("notes.txt"));
    assert!(matches!(app.screen, Screen::Panels));
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert!(
        matches!(app.screen, Screen::Viewer(_)),
        "F3: the program's viewer"
    );
    app.screen = Screen::Panels;
    app.viewer_settings.external_command = "set NC_ASSOC=f3-!.!".to_string();
    app.viewer_settings.use_external = true;
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert_eq!(
        variable(&app).as_deref(),
        Some("f3-notes.txt"),
        "F3: the external one"
    );
    press(&mut app, KeyCode::F(3), KeyModifiers::ALT);
    assert!(
        matches!(app.screen, Screen::Viewer(_)),
        "Alt+F3: the program's"
    );
    app.screen = Screen::Panels;
    app.viewer_settings.external_command.clear();
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert!(
        matches!(app.screen, Screen::Viewer(_)),
        "no command: the program's"
    );
}
