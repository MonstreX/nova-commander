use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;

fn window(pid: u32, window: usize, title: &str) -> TaskWindow {
    TaskWindow {
        window,
        pid,
        title: title.to_string(),
    }
}

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = app_in(temp.path());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    app.tick();
}

fn ctrl_w(app: &mut App) {
    press(app, KeyCode::Char('w'), KeyModifiers::CONTROL);
}

fn task_menu(app: &App) -> Option<&PluginMenu> {
    match &app.overlay {
        Some(Overlay::PluginMenu(menu)) if menu.tasks.is_some() => Some(menu),
        _ => None,
    }
}

/// FAR's `AddMenuItem`: the numbers right-aligned to the greatest one's digits, `│`, the title or the
/// image ("???" unknown); this program's process or console window checked.
#[test]
fn each_window_is_its_number_and_title() {
    let mut list = TaskWindows {
        windows: vec![
            window(8, 1, "Notes & more"),
            window(1234, 2, "Shell"),
            window(77, 3, "Other"),
        ],
        show_image: false,
    };
    let image = |pid: u32| match pid {
        8 => r"C:\notes.exe".to_string(),
        _ => String::new(),
    };
    let shown = lines(&list, &image, (1234, 3));
    let texts: Vec<&str> = shown.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        ["   8 │ Notes & more", "1234 │ Shell", "  77 │ Other"]
    );
    let checked: Vec<bool> = shown.iter().map(|line| line.checked).collect();
    assert_eq!(checked, [false, true, true], "its own process, its console");
    list.show_image = true;
    let images: Vec<String> = (lines(&list, &image, (0, 0)).into_iter())
        .map(|line| line.text)
        .collect();
    assert_eq!(images[0], r"   8 │ C:\notes.exe");
    assert_eq!(images[1], "1234 │ ???");
    assert_eq!(pid_width(&[]), 1);
    assert_eq!(pid_width(&[window(0, 0, "")]), 1);
    assert_eq!(pid_width(&[window(10, 0, "")]), 2);
}

/// FAR's `VMenu2`: "Task list", the keys below, `&` shown as it is, wrapping.
#[test]
fn the_menu_is_fars() {
    let list = TaskWindows {
        windows: vec![window(8, 1, "R&D"), window(9, 2, "Two")],
        show_image: false,
    };
    let menu = menu(list.clone(), 1, 30);
    assert_eq!(menu.title, "Task list");
    assert_eq!(menu.bottom, "Del F2 Ctrl+R");
    assert_eq!(menu.max_height, Some(25), "FAR's ScrY - 4");
    assert_eq!(menu.selected, 1);
    assert!(
        menu.lines[0].text.contains("R&&D"),
        "{}",
        menu.lines[0].text
    );
    assert_eq!(menu.tasks, Some(list));
}

/// Ctrl+W over a dialog: the list in front, Esc brings the dialog back.
#[test]
fn ctrl_w_stands_over_what_is_in_front() {
    let (_temp, mut app) = app();
    ctrl_w(&mut app);
    assert!(task_menu(&app).is_some(), "over the panels");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());

    app.show_dialog(Dialog::message("Note", vec!["Under".to_string()]));
    ctrl_w(&mut app);
    assert!(task_menu(&app).is_some(), "over the dialog");
    ctrl_w(&mut app);
    assert!(task_menu(&app).is_some(), "not opened twice");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Note"),
        "the dialog back"
    );
}

/// The menu's Commands item opens it as Ctrl+W.
#[test]
fn the_commands_menu_opens_it() {
    let (_temp, mut app) = app();
    app.execute(super::super::Command::TaskList);
    assert!(task_menu(&app).is_some());
}

/// A program running in the terminal keeps its Ctrl+W: the guest shell's, a command's from the command
/// line, a detached one's.
#[test]
fn a_running_program_keeps_ctrl_w() {
    use super::super::MainView;
    let (_temp, mut app) = app();
    let key = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
    for view in [
        MainView::Guest,
        MainView::CommandOutput,
        MainView::Detached(0),
    ] {
        app.main_view = view;
        assert!(!app.task_list_key(key), "{view:?}");
        assert!(app.overlay.is_none());
    }
}

/// Past the end after the windows changed: on the last line (FAR's `SetSelectPos` without a direction).
#[test]
fn the_cursor_stays_within_the_windows() {
    assert_eq!(kept(1, 5), 1);
    assert_eq!(kept(7, 5), 4);
    assert_eq!(kept(3, 0), 0);
    let (_temp, mut app) = app();
    assert!(app.open_task_list(false, usize::MAX));
    let menu = task_menu(&app).expect("the list");
    assert_eq!(menu.selected, menu.lines.len().saturating_sub(1));
}

/// Esc on the question brings the list back; the window Ctrl+W stood over survives the question and
/// comes back once the list closes.
#[test]
fn the_question_and_what_was_under_come_back() {
    let (_temp, mut app) = app();
    app.show_dialog(Dialog::message("Note", vec!["Under".to_string()]));
    ctrl_w(&mut app);
    let list = TaskWindows {
        windows: vec![window(u32::MAX - 2, 0, "Gone")],
        show_image: false,
    };
    app.overlay = Some(Overlay::PluginMenu(Box::new(menu(list, 0, 30))));
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm { .. }))
    ));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        task_menu(&app).is_some(),
        "Esc on the question: the list again"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Note"),
        "the dialog under it back"
    );
}

/// F2 shows the images and keeps the cursor's line; Ctrl+R reads the windows again from the first.
#[test]
fn f2_and_ctrl_r_list_anew() {
    let (_temp, mut app) = app();
    let list = TaskWindows {
        windows: vec![window(1, 1, "a"), window(2, 2, "b")],
        show_image: false,
    };
    let mut shown = menu(list, 1, 30);
    shown.selected = 1;
    app.overlay = Some(Overlay::PluginMenu(Box::new(shown)));
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    let menu = task_menu(&app).expect("still the list");
    assert!(menu.tasks.as_ref().is_some_and(|list| list.show_image));
    let count = menu.lines.len();
    assert_eq!(menu.selected, 1.min(count.saturating_sub(1)), "kept");
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    let menu = task_menu(&app).expect("read again");
    assert!(menu.tasks.as_ref().is_some_and(|list| list.show_image));
    assert_eq!(menu.selected, 0);
}

/// Del asks FAR's question with the window's title; Kill ends the process (the test's own) and the list
/// comes back; a process that cannot be ended is told.
#[test]
fn del_asks_and_kills() {
    let (_temp, mut app) = app();
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "pause"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let list = TaskWindows {
        windows: vec![window(child.id(), 0, "The child")],
        show_image: false,
    };
    app.overlay = Some(Overlay::PluginMenu(Box::new(menu(list, 0, 30))));
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm {
        title,
        lines,
        buttons,
        ..
    })) = &app.overlay
    else {
        panic!("the question: {:?}", app.overlay.is_some());
    };
    assert_eq!(title, "Kill task");
    assert_eq!(
        lines,
        &[
            "Do you wish to kill selected task?",
            "The child",
            "You will lose any unsaved information in this program"
        ]
    );
    assert_eq!(buttons, &["Kill", "Cancel"]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(child.wait().is_ok_and(|status| !status.success()));
    assert!(task_menu(&app).is_some(), "the list again");

    let none = TaskWindows {
        windows: vec![window(u32::MAX - 2, 0, "Gone")],
        show_image: false,
    };
    app.overlay = Some(Overlay::PluginMenu(Box::new(menu(none, 0, 30))));
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Message { title, lines, .. })) = &app.overlay else {
        panic!("the failure told");
    };
    assert_eq!(title, "Kill task");
    assert_eq!(lines[0], "Cannot kill the specified task");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(task_menu(&app).is_some(), "the list after the message");
}
