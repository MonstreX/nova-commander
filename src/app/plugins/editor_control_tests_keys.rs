//! An editor key's plugins told in turn only while its editor stays in front; a command the call of
//! the editor in front.

use super::*;

/// Review D2: a command run from F11 is the call of the editor in front, whatever window a call
/// before it began in.
#[test]
fn an_editor_command_is_the_call_of_the_editor_in_front() {
    let (_temp, mut app, path) = editing("edone", "one\r\ntwo\r\n");
    app.plugins.editor_window = Some(app.front_window() + 1);
    run(&mut app, "Upper line");
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ONE\r\ntwo\r\n");
}

/// Review D11: the plugins after the first are not told the key once its editor is no longer in front
/// (the panels brought up meanwhile): the rest dropped, as FAR's key goes to no editor gone.
#[test]
fn an_editor_key_is_not_told_on_once_its_editor_is_left() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["wrapa", "wrapb"] {
        crate::plugin_host::tests::install_shipped(temp.path(), "autowrap", name);
    }
    let path = temp.path().join("w.txt");
    std::fs::write(&path, "one\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.edit_file(&path, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('x'));
    let waited = |app: &App| (app.plugins.key_wait.as_ref()).map(|wait| wait.plugin.clone());
    assert_eq!(waited(&app).as_deref(), Some("wrapa"), "the first told");
    assert!(app.bring_window(crate::app::windows::PANELS));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut second = false;
    loop {
        app.tick();
        second |= waited(&app).as_deref() == Some("wrapb");
        let quiet = app.plugins.key_wait.is_none()
            && app.plugins.key_late.is_empty()
            && app.plugins.editor_key_rest.is_none();
        if quiet {
            break;
        }
        assert!(Instant::now() < deadline, "never done");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!second, "the second plugin not told");
}
