use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::fileops::tests::app_in;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    let app = app_in(temp.path());
    (temp, app)
}

fn run(app: &mut App, commands: &[&str]) {
    let commands: Vec<String> = commands.iter().map(|line| (*line).to_string()).collect();
    app.run_menu_commands("Item", &commands);
    for _ in 0..10 {
        app.tick();
    }
}

fn variable(app: &App, name: &str) -> Option<String> {
    app.environment.get(name)
}

#[test]
fn the_lines_run_one_after_another_and_comments_are_left_out() {
    let (temp, mut app) = app();
    run(
        &mut app,
        &[
            "rem a note",
            ":: another",
            "cd sub",
            r"set NC_DIR=!\",
            "set NC_REM=x",
        ],
    );
    assert_eq!(app.active_panel().path, temp.path().join("sub"));
    let dir = format!("{}\\", temp.path().join("sub").display());
    assert_eq!(
        variable(&app, "NC_DIR"),
        Some(dir),
        "after the cd before it"
    );
    assert_eq!(variable(&app, "NC_REM").as_deref(), Some("x"));
    assert!(app.user_menu.run.is_none(), "done");
}

#[test]
fn questions_are_asked_in_one_dialog_titled_with_the_item() {
    let (_temp, mut app) = app();
    app.run_menu_commands(
        "Find",
        &["set NC_ASK=!?What:?first! and !?Where:?second!".to_string()],
    );
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no questions");
    };
    assert_eq!(form.title, "Find");
    assert_eq!(form.edit_text(1), "first");
    form.set_edit_text(3, "there");
    let ok = form.items.len() - 2;
    form.focus_item(ok);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.tick();
    assert_eq!(variable(&app, "NC_ASK").as_deref(), Some("first and there"));
}

#[test]
fn cancel_of_the_questions_stops_what_is_left() {
    let (_temp, mut app) = app();
    run(&mut app, &["set NC_A=!?A?!", "set NC_B=b"]);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    for _ in 0..5 {
        app.tick();
    }
    assert_eq!(variable(&app, "NC_A"), None);
    assert_eq!(variable(&app, "NC_B"), None);
    assert!(app.user_menu.run.is_none());
}

#[test]
fn a_list_file_holds_the_names_and_goes_a_while_after_the_item() {
    let (temp, mut app) = app();
    let lists = temp.path().join("lists");
    std::fs::create_dir(&lists).unwrap();
    app.user_menu.list_dir = Some(lists.clone());
    std::fs::write(temp.path().join("a b.txt"), "x").unwrap();
    let panel = PanelFacts {
        dir: temp.path().display().to_string(),
        current: "a b.txt".to_string(),
        selected: vec!["a b.txt".to_string(), "c.txt".to_string()],
    };
    let file = list_file(&panel, "FQ", &lists).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    let full = temp.path().join("a b.txt");
    assert!(
        text.starts_with(&format!("\"{}\"\r\n", full.display())),
        "{text}"
    );
    std::fs::remove_file(file).unwrap();

    let unicode = list_file(&panel, "W", &lists).unwrap();
    let bytes = std::fs::read(&unicode).unwrap();
    assert_eq!(&bytes[..2], b"a\0", "no signature");
    std::fs::remove_file(unicode).unwrap();

    run(&mut app, &["set NC_LIST=!@!"]);
    let list = variable(&app, "NC_LIST").unwrap();
    assert!(
        std::path::Path::new(&list).exists(),
        "{list} stays for a program that reads it late"
    );
    app.remove_list_files(false);
    assert!(std::path::Path::new(&list).exists(), "not yet old");
    app.close_user_menu();
    assert!(
        !std::path::Path::new(&list).exists(),
        "{list} is gone at the end"
    );
}

#[test]
fn the_name_under_the_cursor_is_the_one_the_item_was_chosen_on() {
    let (temp, mut app) = app();
    std::fs::write(temp.path().join("chosen.txt"), "x").unwrap();
    app.panels[0].reload();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.panels[0]
        .entries
        .iter()
        .any(|entry| entry.name == "chosen.txt")
    {
        assert!(std::time::Instant::now() < deadline);
        app.tick();
    }
    app.panels[0].go_to_name("chosen.txt");
    run(&mut app, &["cd sub", "set NC_NAME=!.!"]);
    assert_eq!(variable(&app, "NC_NAME").as_deref(), Some("chosen.txt"));
}

#[cfg(windows)]
#[test]
fn a_command_sent_to_the_background_stops_what_is_left_of_the_item() {
    let (_temp, mut app) = app();
    app.run_menu_commands(
        "Item",
        &[
            "ping -n 5 127.0.0.1 >nul".to_string(),
            "set NC_AFTER=x".to_string(),
        ],
    );
    assert_eq!(app.main_view, MainView::CommandOutput);
    app.detach_command();
    for _ in 0..10 {
        app.tick();
    }
    assert_eq!(variable(&app, "NC_AFTER"), None);
    assert!(app.user_menu.run.is_none());
    for index in 0..app.detached.len() {
        // Ctrl+C ends the ping.
        let _ = app.send_to_detached(index, &[3]);
    }
}

#[test]
fn comment_lines_are_rem_and_double_colons_only() {
    assert!(is_comment("REM"));
    assert!(is_comment("rem something"));
    assert!(is_comment(":: note"));
    assert!(!is_comment("remove x"));
    assert!(!is_comment("echo rem"));
}

/// An app on a folder of `names`, all of them selected.
fn app_with_selected(names: &[&str]) -> (tempfile::TempDir, App) {
    let (temp, mut app) = app();
    for name in names {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    app.panels[0].reload();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !names.iter().all(|name| {
        app.panels[0]
            .entries
            .iter()
            .any(|entry| entry.name == *name)
    }) {
        assert!(std::time::Instant::now() < deadline);
        app.tick();
    }
    for name in names {
        app.panels[0].selected.insert((*name).to_string());
    }
    (temp, app)
}

fn submit(app: &mut App, text: &str) {
    let Some(Overlay::Dialog(Dialog::Input {
        title,
        prompt,
        input,
        ..
    })) = &mut app.overlay
    else {
        panic!("no question");
    };
    assert_eq!(title, "Apply command");
    assert_eq!(prompt, "Enter command to process selected files");
    input.set(text);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for _ in 0..10 {
        app.tick();
    }
}

#[test]
fn ctrl_g_runs_the_command_for_each_selected_file_and_unselects_it() {
    let (_temp, mut app) = app_with_selected(&["a.txt", "b.txt"]);
    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
    submit(&mut app, "  set NC_APPLY_!=!.!");
    assert_eq!(variable(&app, "NC_APPLY_a").as_deref(), Some("a.txt"));
    assert_eq!(variable(&app, "NC_APPLY_b").as_deref(), Some("b.txt"));
    assert!(
        app.panels[0].selected.is_empty(),
        "each file leaves the selection"
    );
}

#[test]
fn ctrl_g_offers_the_last_command_and_runs_on_the_file_under_the_cursor() {
    let (_temp, mut app) = app_with_selected(&["one.txt"]);
    app.panels[0].selected.clear();
    app.panels[0].go_to_name("one.txt");
    app.execute(crate::app::Command::Apply);
    submit(&mut app, "set NC_ONE=!.!");
    assert_eq!(variable(&app, "NC_ONE").as_deref(), Some("one.txt"));
    app.execute(crate::app::Command::Apply);
    let Some(Overlay::Dialog(Dialog::Input { input, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(input.text(), "set NC_ONE=!.!");
}

#[test]
fn apply_keeps_the_selection_for_ctrl_m_and_a_cancelled_file_stays_selected() {
    let (_temp, mut app) = app_with_selected(&["a.txt", "b.txt"]);
    app.execute(crate::app::Command::Apply);
    submit(&mut app, "set NC_ASK_!=!?Value for !.!?!");
    // The first file's question: OK; the second's: Cancel.
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no question");
    };
    let ok = form.items.len() - 2;
    form.focus_item(ok);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for _ in 0..5 {
        app.tick();
    }
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.tick();
    let selected: Vec<&str> = app.panels[0].selected.iter().map(String::as_str).collect();
    assert_eq!(selected, ["b.txt"], "the cancelled one is still selected");
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL));
    assert_eq!(
        app.panels[0].selected.len(),
        2,
        "Ctrl+M: as before the command"
    );
}

#[test]
fn the_selected_names_of_a_line_still_hold_its_own_file() {
    let (_temp, mut app) = app_with_selected(&["a.txt", "b.txt"]);
    app.execute(crate::app::Command::Apply);
    submit(&mut app, "set NC_ALL_!=!&");
    assert_eq!(
        variable(&app, "NC_ALL_a").as_deref(),
        Some(r#""a.txt" "b.txt""#)
    );
}

#[test]
fn a_run_asked_for_while_one_goes_on_comes_after_it_and_lives_on_its_own() {
    let (temp, mut app) = app();
    // The first run's second line moves the panel; the second run's line says where it is.
    app.run_menu_commands("One", &["set NC_FIRST=1".to_string(), "cd sub".to_string()]);
    app.run_menu_commands("Two", &[r"set NC_AFTER=!\".to_string()]);
    for _ in 0..10 {
        app.tick();
    }
    let sub = format!("{}\\", temp.path().join("sub").display());
    assert_eq!(
        variable(&app, "NC_AFTER"),
        Some(sub),
        "after the first run's cd"
    );

    // A Cancel in the first run's questions leaves the run asked for after it.
    app.run_menu_commands("Ask", &["set NC_ASKED=!?Value?!".to_string()]);
    app.run_menu_commands("Later", &["set NC_LATER=yes".to_string()]);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(form.title, "Ask");
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    for _ in 0..5 {
        app.tick();
    }
    assert_eq!(variable(&app, "NC_ASKED"), None);
    assert_eq!(variable(&app, "NC_LATER").as_deref(), Some("yes"));
}
