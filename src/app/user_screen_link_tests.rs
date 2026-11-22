//! The command line, the user screen and starting what is typed.

use super::*;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

/// Runs the app's periodic work until `done` or 20 s have passed (no sleeping: yields between ticks).
pub(in crate::app) fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        app.tick();
        if done(app) {
            return true;
        }
        std::thread::yield_now();
    }
    false
}

fn screen_text(app: &App) -> String {
    app.user_screen
        .as_ref()
        .map(|screen| screen.screen().contents())
        .unwrap_or_default()
}

fn app_in(dir: &Path) -> App {
    App::new(dir, 100, 30, &crate::settings::Settings::default())
}

fn run(app: &mut App, line: &str) {
    app.command_line.set(line);
    app.run_command_line();
}

#[cfg(windows)]
#[test]
fn shift_enter_runs_nothing_where_the_interpreter_cannot_stand() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    // A folder that is there, in the form cmd cannot stand in.
    let verbatim = format!(r"\\?\{}", temp.path().display());
    app.active_panel_mut().path = crate::vfs::VfsPath::local(verbatim);
    app.command_line.set("echo x");

    app.run_in_new_window();
    crate::app::waiting::finish_waiting(&mut app);

    let Some(super::super::Overlay::Dialog(Dialog::Message { lines, error, .. })) = &app.overlay
    else {
        panic!("no message");
    };
    assert!(*error);
    assert_eq!(
        lines,
        &[crate::platform::process::INTERPRETER_FOLDER_REFUSAL.to_string()]
    );
}

#[test]
fn pushd_and_popd_move_the_panel_and_remember_the_folder() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let mut app = app_in(temp.path());

    run(&mut app, "pushd sub");
    assert_eq!(app.active_panel().path, sub);
    assert_eq!(
        app.environment.get("FARDIRSTACK").as_deref(),
        Some(temp.path().to_string_lossy().as_ref())
    );
    run(&mut app, "popd");
    assert_eq!(app.active_panel().path, temp.path());
    assert_eq!(app.environment.get("FARDIRSTACK"), None);
    run(&mut app, "popd");
    assert_eq!(app.active_panel().path, temp.path(), "nothing remembered");
    assert_eq!(app.main_view, MainView::Panels, "nothing runs");
}

#[test]
fn a_failed_pushd_remembers_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());

    run(&mut app, "pushd no-such-folder");
    assert!(app.overlay.is_some(), "the panel says so");
    assert_eq!(app.environment.get("FARDIRSTACK"), None);
    assert_eq!(app.environment.pop_dir(), None);
}

#[test]
fn a_pushd_read_in_the_background_remembers_the_folder_only_once_it_is_entered() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let mut app = app_in(temp.path());
    // A slow folder: the panel goes on before its reading ends.
    app.active_panel_mut().set_read_wait(Duration::ZERO);

    run(&mut app, "pushd no-such-folder");
    assert!(tick_until(&mut app, |app| app
        .active_panel()
        .reading()
        .is_none()));
    app.tick();
    assert_eq!(app.environment.get("FARDIRSTACK"), None, "it failed late");

    app.overlay = None;
    run(&mut app, "pushd sub");
    assert!(tick_until(&mut app, |app| app.active_panel().path == sub));
    app.tick();
    assert_eq!(
        app.environment.get("FARDIRSTACK").as_deref(),
        Some(temp.path().to_string_lossy().as_ref()),
        "remembered once entered"
    );
}

#[test]
fn an_exit_given_up_leaves_no_code_behind() {
    let mut environment = crate::terminal::Environment::default();
    environment.ask_exit_code(5);
    assert_eq!(environment.exit_code(), 0, "not confirmed yet");
    environment.ask_exit_code(0);
    environment.confirm_exit_code();
    assert_eq!(environment.exit_code(), 0, "F10 after an `exit` given up");
    environment.ask_exit_code(5);
    environment.confirm_exit_code();
    assert_eq!(environment.exit_code(), 5);
}

#[test]
fn exit_ends_the_program_with_its_code() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());

    run(&mut app, "exit 7");
    assert!(app.should_quit());
    assert_eq!(app.environment.exit_code(), 7);
}

#[test]
fn title_is_kept_for_whoever_shows_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());

    run(&mut app, "title my work");
    assert_eq!(app.take_user_title().as_deref(), Some("my work"));
    assert_eq!(app.main_view, MainView::Panels, "nothing runs");
}

#[test]
fn a_failure_comes_up_as_fars_message_once_the_panels_are_back() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("broken.exe"), "not a program").unwrap();
    let mut app = app_in(temp.path());

    run(&mut app, "broken.exe");
    assert!(tick_until(&mut app, |app| app.overlay.is_some()));

    assert_eq!(app.main_view, MainView::Panels);
    let Some(super::super::Overlay::Dialog(Dialog::Message { lines, error, .. })) = &app.overlay
    else {
        panic!("no message");
    };
    assert!(*error);
    assert_eq!(lines[..2], ["Cannot execute", "broken.exe"]);
    assert!(!lines[2].contains("%1"), "{}", lines[2]);
}

/// Only what would be started: an automatic test never brings up the administrator's prompt.
#[test]
fn ctrl_alt_enter_takes_the_line_or_the_file_under_the_cursor() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("tool.exe"), "").unwrap();
    std::fs::write(temp.path().join("notes.txt"), "").unwrap();
    let mut app = app_in(temp.path());
    crate::app::waiting::finish_waiting(&mut app);

    app.command_line.set("  tool -x ");
    let (line, mode) = app.administrator_start().unwrap();
    assert_eq!(line, "tool -x");
    assert!(mode.run_as && !mode.file);
    assert!(app.command_line.is_empty());
    assert_eq!(app.commands().last(), Some("tool -x"));

    for name in ["tool.exe", "notes.txt"] {
        assert!(tick_until(&mut app, |app| {
            app.active_panel()
                .entries
                .iter()
                .any(|entry| entry.name == name)
        }));
        app.active_panel_mut().home();
        for _ in 0..5 {
            if app
                .active_panel()
                .current()
                .map(|entry| entry.name.as_str())
                == Some(name)
            {
                break;
            }
            app.active_panel_mut().move_cursor(1);
        }
        let (line, mode) = app.administrator_start().unwrap();
        assert_eq!(line, temp.path().join(name).to_string_lossy());
        assert!(mode.run_as && mode.file);
    }
}

#[test]
fn a_command_runs_in_the_panels_folder_and_its_output_is_shown_until_it_ends() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("marker-5c1.txt"), "").unwrap();
    let mut app = app_in(temp.path());

    run(&mut app, "dir /b");
    assert_eq!(app.main_view, MainView::CommandOutput);

    assert!(tick_until(&mut app, |app| app.main_view == MainView::Panels));
    let prompt = format!("{}>dir /b", temp.path().display());
    assert!(tick_until(&mut app, |app| {
        let text = screen_text(app);
        text.contains(&prompt) && text.contains("marker-5c1.txt")
    }));
    assert_eq!(app.commands().last(), Some("dir /b"));
}

#[test]
fn the_panels_folder_can_be_moved_after_a_command_ran_in_it() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("work");
    std::fs::create_dir(&folder).unwrap();
    let mut app = app_in(&folder);

    run(&mut app, "echo in-work");
    assert!(tick_until(&mut app, |app| app.main_view == MainView::Panels));

    // The old persistent shell held this folder for good: moving it failed with os error 5.
    crate::terminal::rename_soon(&folder, &temp.path().join("moved")).unwrap();
}

#[test]
fn cd_and_set_are_carried_out_by_the_program_and_shown() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let mut app = app_in(temp.path());

    run(&mut app, "cd sub");
    assert_eq!(app.active_panel().path, sub);
    assert_eq!(app.main_view, MainView::Panels, "nothing runs");
    run(&mut app, "set NC_APP_TEST=from-set");
    run(&mut app, "echo [%NC_APP_TEST%]");
    assert!(tick_until(&mut app, |app| screen_text(app).contains("\n[from-set]")));

    let text = screen_text(&app);
    assert!(text.contains(&format!("{}>cd sub", temp.path().display())));
    assert!(text.contains(&format!("{}>set NC_APP_TEST=from-set", sub.display())));
}

#[test]
fn keys_go_to_the_running_command() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());

    // The command line drops trailing spaces, so the question has none.
    run(&mut app, "set /p NC_ANSWER=Question:");
    assert!(tick_until(&mut app, |app| screen_text(app).contains("Question:")));
    for ch in "yes-42".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.command_line.is_empty(),
        "nothing reaches the command line"
    );

    // The program read the line (it ends only then) and echoed it.
    assert!(tick_until(&mut app, |app| app.main_view == MainView::Panels));
    assert!(tick_until(&mut app, |app| screen_text(app).contains("Question:yes-42")));
}

#[test]
fn an_idle_app_asks_for_no_redraw() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    run(&mut app, "echo started");
    assert!(tick_until(&mut app, |app| app.main_view == MainView::Panels));
    // Once the output is consumed, ticks report no change (the cursor keeps its normal blink
    // because the screen is not redrawn).
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut idle = false;
    while !idle && Instant::now() < deadline {
        idle = !app.tick() && !app.tick();
        std::thread::yield_now();
    }
    assert!(idle);
}

#[cfg(windows)]
#[test]
fn a_change_of_the_systems_environment_reaches_the_commands_only_when_asked() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.environment.set("SystemRoot", r"Z:\nowhere");
    // Off by default, as FAR's `UpdateEnvironment`.
    app.environment_changed();
    assert_eq!(
        app.environment.get("SystemRoot").as_deref(),
        Some(r"Z:\nowhere")
    );
    app.prefs.update_environment = true;
    app.environment_changed();
    let system = app.environment.get("SystemRoot").unwrap();
    assert!(system.to_uppercase().ends_with(r"\WINDOWS"), "{system}");
}

#[cfg(windows)]
#[test]
fn a_reload_keeps_what_set_did_to_variables_the_registry_does_not_have() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.prefs.update_environment = true;
    app.environment.set("NC_P44_MINE", "mine");
    // Cargo gives the tests this one; no registry has it.
    assert!(app.environment.get("CARGO_PKG_NAME").is_some());
    app.environment.set("CARGO_PKG_NAME", "");
    app.environment_changed();
    assert_eq!(app.environment.get("NC_P44_MINE").as_deref(), Some("mine"));
    assert_eq!(
        app.environment.get("CARGO_PKG_NAME"),
        None,
        "removed stays removed"
    );
}
