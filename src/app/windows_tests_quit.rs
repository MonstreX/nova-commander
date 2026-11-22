//! Quitting's questions waited for across the ticks, asked once; the modal editor's New instance; the
//! panels never dropped as a window behind.

use super::*;

/// The editor on `a.txt` with `x` typed, the panels in front.
fn edited_behind() -> (tempfile::TempDir, App, PathBuf) {
    let (temp, mut app, a, _) = two_files();
    app.edit_file(&a, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    settle(&mut app);
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "Panels");
    (temp, app, a)
}

/// FAR's `ExitAll`: the question stays the quitting's while it is up, ticks or not; Yes saves and the
/// program ends.
#[test]
fn quitting_waits_for_its_question_across_the_ticks() {
    let (_temp, mut app, a) = edited_behind();
    app.finish_quit();
    for _ in 0..5 {
        app.tick();
    }
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "xfirst\r\n");
    assert!(app.quit, "saved: the program ends");
}

/// Review C10: an editor's own close asked ahead goes when quitting asks about it: Cancel's answer
/// is not followed by that close's question again.
#[test]
fn quitting_asks_once_about_an_editor_with_its_close_asked_ahead() {
    let (_temp, mut app, _) = edited_behind();
    let editor = app.window_lines()[0].id;
    assert!(app.bring_window(editor));
    if let Screen::Editor(editor) = &mut app.screen {
        editor.pending = Some(crate::app::editing::EditorAction::Close);
    }
    app.finish_quit();
    assert_eq!(front(&app), "Edit a.txt");
    press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    settle(&mut app);
    for _ in 0..5 {
        app.tick();
    }
    assert!(app.overlay.is_none(), "not asked again");
    assert!(!app.quit);
}

/// Review C9: New instance of the modal question opens another editor (button 0 there is FAR's 1).
#[test]
fn new_instance_of_the_modal_question_opens_another_editor() {
    let (_temp, mut app, a, _) = two_files();
    app.edit_file(&a, false);
    settle(&mut app);
    ctrl_tab(&mut app);
    app.user_menu.reread = Some(crate::app::user_menu::Look::Main);
    app.edit_file(&a, false);
    assert_eq!(question(&app), ["&New instance", "&Cancel"]);
    press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(app.window_lines().len(), 2, "another window");
}

/// The panels are never dropped from behind as a viewer an editor took the place of is: Ctrl+Tab
/// still reaches them.
#[test]
fn the_panels_are_never_dropped_from_behind() {
    let (_temp, mut app, a, b) = two_files();
    app.view_file(&b, false);
    app.edit_file(&a, false);
    settle(&mut app);
    app.drop_window(crate::app::windows::PANELS);
    let mut fronts = Vec::new();
    for _ in 0..3 {
        ctrl_tab(&mut app);
        fronts.push(front(&app));
    }
    assert!(fronts.iter().any(|front| front == "Panels"), "{fronts:?}");
}
