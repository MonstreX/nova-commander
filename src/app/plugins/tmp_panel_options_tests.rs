//! The temporary panel's options in effect (ADR-0016 §14, 5d-3b; FAR's TmpPanel), its library as it is
//! shipped, each test's options in the plugin's settings: safe mode (no real names, "(R) ", Enter and
//! Ctrl+PgUp its own, F3 doing nothing), panels of their own without the common panel, folders' contents
//! put, any elements kept, a gone entry's F3 swallowed while F3 on a file there is the program's.

use super::*;
use crate::app::Screen;

fn titled(app: &App, side: Side, title: &str) -> bool {
    let panel = &app.panels[side.index()];
    panel.reading().is_none() && panel.look.title.as_deref() == Some(title)
}

/// `names` on the right put into the temporary panel on the left with F5, the left one active then.
fn put(app: &mut App, names: &[&str], count: usize) {
    for name in names {
        point(app, Side::Right, name);
        app.handle_key(key(KeyCode::Insert));
    }
    copy(app, KeyCode::F(5));
    tick_until(app, |app| {
        app.overlay.is_none()
            && app.plugins.putting.is_none()
            && super::names(app, Side::Left).len() == count
    });
    app.active = Side::Left;
}

/// FAR's "Warning" up (the question about folders' contents).
fn warning_up(app: &App) -> bool {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        return false;
    };
    form.title == "Warning"
}

/// A few rounds of the event loop: a key's answer from the plugin's thread taken.
fn settle(app: &mut App) {
    for _ in 0..40 {
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
}

#[test]
fn safe_mode_has_no_real_names_and_enter_and_ctrl_pgup_its_own() {
    let (_temp, files, mut app) = opened_with("tmpoptone", &[("SafeModePanel", "1")], COMMON);
    assert!(titled(&app, Side::Left, "(R) Temporary panel [0]"));
    put(&mut app, &["a.txt", "sub"], 2);
    assert!(
        (app.panels[0].entries.iter()).all(|entry| entry.real_path.is_none()),
        "FAR's: no OPIF_REALNAMES"
    );
    let a = files.path().join("a.txt");
    point(&mut app, Side::Left, &a.display().to_string());
    // No GetFiles: F3 does nothing on it.
    app.handle_key(key(KeyCode::F(3)));
    settle(&mut app);
    assert!(matches!(app.screen, Screen::Panels) && app.overlay.is_none());
    // No DeleteFiles: Shift+Del and Alt+Del do nothing either.
    for modifiers in [KeyModifiers::SHIFT, KeyModifiers::ALT] {
        app.handle_key(KeyEvent::new(KeyCode::Delete, modifiers));
        settle(&mut app);
        assert!(app.overlay.is_none(), "nothing asked");
    }
    assert_eq!(names(&app, Side::Left).len(), 2, "nothing taken off");
    assert!(a.is_file());
    // Ctrl+PgUp: the panel to the file's folder, the cursor on it (`GoToFile`).
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL));
    let folder = VfsPath::local(files.path());
    tick_until(&mut app, |app| shows(app, Side::Left, &folder));
    let left = &app.panels[0];
    assert_eq!(
        left.current().map(|entry| entry.name.as_str()),
        Some("a.txt")
    );
}

#[test]
fn safe_mode_does_nothing_to_a_selection_with_the_cursor_on_the_parent() {
    // No GetFiles, no DeleteFiles: F5, F6, F8 and Del act on nothing from the panel, a selection too.
    let (_temp, files, mut app) = opened_with("tmpoptsel", &[("SafeModePanel", "1")], COMMON);
    put(&mut app, &["a.txt"], 1);
    let a = files.path().join("a.txt");
    point(&mut app, Side::Left, &a.display().to_string());
    app.handle_key(key(KeyCode::Insert));
    app.panels[0].cursor = 0;
    assert!(app.panels[0].current().is_some_and(|entry| entry.is_parent));
    for pressed in [
        key(KeyCode::F(5)),
        key(KeyCode::F(6)),
        key(KeyCode::F(8)),
        key(KeyCode::Delete),
    ] {
        app.handle_key(pressed);
        settle(&mut app);
        assert!(app.overlay.is_none(), "nothing asked for {pressed:?}");
    }
    assert_eq!(names(&app, Side::Left).len(), 1, "nothing taken off");
    assert!(a.is_file());
}

#[test]
fn safe_modes_enter_on_a_file_does_nothing_and_del_edits_a_command_line_with_text() {
    let (_temp, files, mut app) = opened_with("tmpoptfile", &[("SafeModePanel", "1")], COMMON);
    put(&mut app, &["a.txt"], 1);
    let a = files.path().join("a.txt");
    point(&mut app, Side::Left, &a.display().to_string());
    let shown = app.panels[0].path.clone();
    // No GetFiles: Enter on a file opens nothing.
    app.handle_key(key(KeyCode::Enter));
    settle(&mut app);
    assert!(matches!(app.screen, Screen::Panels) && app.overlay.is_none());
    assert_eq!(app.panels[0].path, shown);
    // Del with text in the command line is the line's (FAR's), not the panel's.
    app.command_line.set("abc");
    app.handle_key(key(KeyCode::Home));
    app.handle_key(key(KeyCode::Delete));
    settle(&mut app);
    assert_eq!(app.command_line.text(), "bc");
    assert_eq!(names(&app, Side::Left).len(), 1);
}

#[test]
fn safe_modes_enter_on_a_folder_goes_there() {
    let (_temp, files, mut app) = opened_with("tmpopttwo", &[("SafeModePanel", "1")], COMMON);
    put(&mut app, &["sub"], 1);
    let sub = files.path().join("sub");
    point(&mut app, Side::Left, &sub.display().to_string());
    app.handle_key(key(KeyCode::Enter));
    let real = VfsPath::local(&sub);
    tick_until(&mut app, |app| shows(app, Side::Left, &real));
}

#[test]
fn without_the_common_panel_each_panel_has_its_own_list_gone_as_it_closes() {
    let (temp, _files, mut app) = opened_with("tmpoptthree", &[("CommonPanel", "0")], "local");
    assert!(titled(&app, Side::Left, "Temporary panel"), "no [n]");
    let labels = app.plugin_key_labels(crate::platform::input::Modifiers {
        shift: true,
        alt: true,
        ctrl: false,
    });
    assert!(
        !labels.iter().any(|(number, _)| *number == 12),
        "no Switch: {labels:?}"
    );
    put(&mut app, &["a.txt"], 1);
    // Another one opened on the right: a list of its own, empty.
    let first = app.panels[0].path.clone();
    app.open_disk_for_test(0, 0, Side::Right.index());
    tick_until(&mut app, |app| {
        shows_instance(app, Side::Right, "tmpoptthree", "local") && app.panels[1].path != first
    });
    assert!(names(&app, Side::Right).is_empty());
    assert_eq!(names(&app, Side::Left).len(), 1);
    // The left one closed (FAR's `ClosePanel`): its list is gone, its path opens nothing any more.
    let disk = VfsPath::local(temp.path());
    app.panels[0].change_dir(&disk, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Left, &disk));
    // The next ticks tell the plugin its panel is no longer shown.
    settle(&mut app);
    app.active = Side::Left;
    let _ = app.panels[0].change_dir(&first, None);
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    settle(&mut app);
    assert_ne!(app.panels[0].path, first, "the closed panel's list is gone");
}

#[test]
fn copy_folder_contents_on_puts_the_folders_files() {
    let (_temp, files, mut app) = opened_with("tmpoptfour", &[("CopyContents", "1")], COMMON);
    put(&mut app, &["sub"], 2);
    let mut shown = names(&app, Side::Left);
    shown.sort();
    let sub = files.path().join("sub");
    assert_eq!(
        shown,
        [
            sub.display().to_string(),
            sub.join("b.txt").display().to_string()
        ]
    );
}

#[test]
fn copy_folder_contents_asked_is_asked_once_and_esc_is_no() {
    let (_temp, files, mut app) = opened_with("tmpoptfive", &[("CopyContents", "2")], COMMON);
    std::fs::create_dir(files.path().join("other")).unwrap();
    std::fs::write(files.path().join("other").join("c.txt"), "c").unwrap();
    app.panels[1].reload();
    for name in ["sub", "other"] {
        point(&mut app, Side::Right, name);
        app.handle_key(key(KeyCode::Insert));
    }
    copy(&mut app, KeyCode::F(5));
    tick_until(&mut app, warning_up);
    // FAR's `!Message(...)`: Esc is No, for every folder of the put.
    app.handle_key(key(KeyCode::Esc));
    tick_until(&mut app, |app| {
        app.overlay.is_none() && app.plugins.putting.is_none() && names(app, Side::Left).len() == 2
    });
    settle(&mut app);
    assert_eq!(
        names(&app, Side::Left).len(),
        2,
        "the folders alone, asked once"
    );
}

#[test]
fn any_elements_keep_a_gone_file_and_enter_puts_it_in_the_command_line() {
    let (_temp, files, mut app) = opened_with("tmpoptsix", &[("AnyInPanel", "1")], COMMON);
    put(&mut app, &["a.txt"], 1);
    let a = files.path().join("a.txt");
    std::fs::remove_file(&a).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    assert_eq!(names(&app, Side::Left), [a.display().to_string()], "kept");
    point(&mut app, Side::Left, &a.display().to_string());
    app.handle_key(key(KeyCode::Enter));
    tick_until(&mut app, |app| {
        app.command_line.text() == a.display().to_string()
    });
}

#[test]
fn f3_on_a_gone_entry_does_nothing_and_on_a_file_there_views_it() {
    let (_temp, files, mut app) = opened("tmpoptseven");
    put(&mut app, &["a.txt", "sub"], 2);
    let a = files.path().join("a.txt");
    let b = files.path().join("sub").join("b.txt");
    std::fs::remove_file(&b).unwrap();
    std::fs::remove_dir(files.path().join("sub")).unwrap();
    // The entry is still shown (not read again): TmpPanel's check finds its file gone.
    let sub = files.path().join("sub").display().to_string();
    point(&mut app, Side::Left, &sub);
    app.handle_key(key(KeyCode::F(3)));
    settle(&mut app);
    assert!(matches!(app.screen, Screen::Panels) && app.overlay.is_none());
    point(&mut app, Side::Left, &a.display().to_string());
    app.handle_key(key(KeyCode::F(3)));
    tick_until(&mut app, |app| matches!(app.screen, Screen::Viewer(_)));
}
