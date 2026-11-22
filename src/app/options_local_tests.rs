//! Alt+Shift+F9: FAR's local settings of the open viewer and editor (S2d.6b, S2d.6d).

use super::*;

/// S2d.6b: Alt+Shift+F9 in the viewer - FAR's "Viewer" settings for this viewer alone.
#[test]
fn alt_shift_f9_sets_the_open_viewer_alone() {
    let (temp, mut app) = app();
    app.view_file(&temp.path().join("a.txt"), false);
    let alt_shift = KeyModifiers::ALT | KeyModifiers::SHIFT;
    app.handle_key(KeyEvent::new(KeyCode::F(9), alt_shift));
    let index = check_box(&app, "Show scrolling arro&ws");
    toggle(&mut app, index);
    press_button(&mut app, "OK");
    let crate::app::Screen::Viewer(viewer) = &app.screen else {
        panic!("no viewer");
    };
    assert!(
        !viewer.arrows && !viewer.settings.show_arrows,
        "the viewer's own"
    );
    assert!(app.viewer_settings.show_arrows, "the settings as they were");
}

/// S2d.6d: Alt+Shift+F9 and Ctrl+F3 in the editor - FAR's editor `EdOpt`, the open editor's alone.
#[test]
fn alt_shift_f9_and_ctrl_f3_set_the_open_editor_alone() {
    let (temp, mut app) = app();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "a b\n").unwrap();
    app.edit_file(&path, false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !matches!(&app.screen, crate::app::Screen::Editor(editor) if editor.text.settled()) {
        app.tick();
        assert!(std::time::Instant::now() < deadline, "never opened");
    }
    let alt_shift = KeyModifiers::ALT | KeyModifiers::SHIFT;
    app.handle_key(KeyEvent::new(KeyCode::F(9), alt_shift));
    assert!(
        (the_form(&app).items.iter()).all(|item| !matches!(
            &item.control,
            Control::Check { label, .. } if label.contains("Save file position")
        )),
        "FAR's local dialog: no places kept"
    );
    let index = check_box(&app, "Sh&ow white space");
    toggle(&mut app, index);
    press_button(&mut app, "OK");
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::CONTROL));
    let crate::app::Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert!(editor.settings.show_white_space && editor.text.white);
    assert!(editor.settings.show_line_numbers && editor.text.numbers);
    assert!(
        !app.editor_settings.show_white_space,
        "the settings as they were"
    );
    assert!(!app.editor_settings.show_line_numbers);
}
