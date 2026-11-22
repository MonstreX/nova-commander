//! The command prefixes (FAR's farcmds, P6.2a) through the program's command line, the plugin's library
//! as it is shipped: goto:, whereis:, clip:, link:, view:, and a command's syntax when it is wrong.
#![cfg(windows)]

use std::time::{Duration, Instant};

use super::*;
use crate::plugin_host::tests::install_shipped;
use crate::vfs::VfsPath;

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The plugin installed as `name`, the program in `folder` holding `files`, read.
fn app_with(name: &str, files: &[&str]) -> (tempfile::TempDir, tempfile::TempDir, App) {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "commands", name);
    let folder = tempfile::tempdir().unwrap();
    for file in files {
        std::fs::write(folder.path().join(file), file).unwrap();
    }
    let mut app = App::new(
        folder.path(),
        100,
        30,
        &crate::settings::Settings::default(),
    );
    let host = crate::plugin_host::PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &serde_json::Map::new(),
        &|_| true,
    );
    app.set_plugins(host);
    app.active = Side::Left;
    tick_until(&mut app, |app| !app.panels[0].is_reading());
    (plugins, folder, app)
}

fn run_line(app: &mut App, line: &str) {
    app.command_line.set(line);
    app.run_command_line();
}

fn under_cursor(app: &App) -> &str {
    let panel = &app.panels[0];
    &panel.entries[panel.cursor].name
}

#[test]
fn goto_takes_the_active_panel_to_the_file_its_cursor_on_it() {
    let (_plugins, folder, mut app) = app_with("cmdone", &[]);
    let other = tempfile::tempdir().unwrap();
    for name in ["a.txt", "B.txt", "c.txt"] {
        std::fs::write(other.path().join(name), "").unwrap();
    }
    run_line(
        &mut app,
        &format!("goto:{}", other.path().join("b.txt").display()),
    );
    let there = VfsPath::local(other.path());
    tick_until(&mut app, |app| {
        app.panels[0].path == there && !app.panels[0].is_reading() && under_cursor(app) == "B.txt"
    });
    assert_eq!(app.panels[0].offset, app.panels[0].cursor, "the top line");
    // `<file`: the path read from a file's first line.
    let list = folder.path().join("where.txt");
    std::fs::write(
        &list,
        format!("{}\r\nnext", other.path().join("c.txt").display()),
    )
    .unwrap();
    run_line(&mut app, &format!("goto:<{}", list.display()));
    tick_until(&mut app, |app| under_cursor(app) == "c.txt");
}

#[test]
fn whereis_finds_a_program_by_its_extension_in_the_current_folder() {
    let (_plugins, _folder, mut app) = app_with("cmdtwo", &["a.txt", "tool.cmd", "z.txt"]);
    run_line(&mut app, "whereis:tool");
    tick_until(&mut app, |app| {
        under_cursor(app).eq_ignore_ascii_case("tool.cmd")
    });
}

#[test]
fn clip_puts_a_files_text_in_the_clipboard() {
    let (_plugins, folder, mut app) = app_with("cmdthree", &[]);
    std::fs::write(folder.path().join("note.txt"), "copied text").unwrap();
    run_line(&mut app, "clip:note.txt");
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.clipboard.text().ok().flatten().as_deref() != Some("copied text") {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn link_makes_a_hard_link_and_the_panel_shows_it() {
    let (_plugins, folder, mut app) = app_with("cmdfour", &["a.txt"]);
    run_line(&mut app, "link: a.txt b.txt");
    tick_until(&mut app, |app| {
        (app.panels[0].entries.iter()).any(|entry| entry.name == "b.txt")
    });
    assert_eq!(
        std::fs::read_to_string(folder.path().join("b.txt")).unwrap(),
        "a.txt"
    );
}

#[test]
fn view_opens_the_viewer_and_a_wrong_command_shows_its_syntax() {
    let (_plugins, _folder, mut app) = app_with("cmdfive", &["a.txt"]);
    run_line(&mut app, "view:a.txt");
    tick_until(&mut app, |app| {
        matches!(app.screen, crate::app::Screen::Viewer(_))
    });
    app.screen = crate::app::Screen::Panels;
    run_line(&mut app, "link:only-one");
    tick_until(&mut app, |app| app.overlay.is_some());
    let Some(Overlay::Dialog(crate::app::dialog::Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no message");
    };
    let shown = format!("{:?}", form.items);
    assert!(
        shown.contains("link:[/msg] [/n] [/s] source destination"),
        "{shown}"
    );
}

#[test]
fn a_commands_output_goes_to_the_viewer_the_clipboard_and_a_file() {
    let (_plugins, folder, mut app) = app_with("cmdsix", &[]);
    run_line(&mut app, "view:<echo seen");
    tick_until(&mut app, |app| {
        matches!(app.screen, crate::app::Screen::Viewer(_))
    });
    let shown = app.plugins.delete_on_close.clone();
    assert_eq!(
        shown.len(),
        1,
        "the output's file goes as the viewer closes"
    );
    assert!(std::fs::read_to_string(&shown[0]).unwrap().contains("seen"));
    app.screen = crate::app::Screen::Panels;
    run_line(&mut app, "run:out.txt<echo kept");
    tick_until(&mut app, |app| !app.plugins.host.in_call("cmdsix"));
    let out = folder.path().join("out.txt");
    tick_until(&mut app, |_| {
        std::fs::read_to_string(&out).is_ok_and(|text| text.contains("kept"))
    });
    run_line(&mut app, "clip:<echo copied");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(app.clipboard.text().ok().flatten()).is_some_and(|text| text.starts_with("copied")) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn esc_ends_a_command_that_runs_long() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (_plugins, _folder, mut app) = app_with("cmdseven", &[]);
    run_line(&mut app, "clip:<ping -n 60 127.0.0.1");
    tick_until(&mut app, |app| app.plugins.host.in_call("cmdseven"));
    std::thread::sleep(crate::app::held::QUICK);
    app.tick();
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let started = Instant::now();
    tick_until(&mut app, |app| !app.plugins.host.in_call("cmdseven"));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "ended, not waited for"
    );
}

/// The file the viewer shows (deleted as it closes), the viewer then closed.
fn viewed(app: &mut App, line: &str) -> String {
    app.plugins.delete_on_close.clear();
    run_line(app, line);
    tick_until(app, |app| {
        matches!(app.screen, crate::app::Screen::Viewer(_))
    });
    app.screen = crate::app::Screen::Panels;
    let shown = app.plugins.delete_on_close.clone();
    assert_eq!(shown.len(), 1, "{line}");
    std::fs::read_to_string(&shown[0]).unwrap()
}

#[test]
fn the_streams_shown_follow_the_catch() {
    let (_plugins, _folder, mut app) = app_with("cmdeight", &[]);
    assert!(
        viewed(&mut app, "view:<echo both 1>&2").contains("both"),
        "* takes the errors too"
    );
    let apart = viewed(&mut app, "view:<? cmd /c echo oops 1>&2");
    assert!(
        apart.contains("oops"),
        "an empty output: the errors shown: {apart:?}"
    );
    run_line(&mut app, "run:<echo nowhere");
    tick_until(&mut app, |app| app.overlay.is_some());
    let Some(Overlay::Dialog(crate::app::dialog::Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no message");
    };
    assert!(format!("{:?}", form.items).contains("run:filename<command"));
}

/// R7 review C7: a file to be deleted as its window closes stays while a window behind has it open,
/// and is the user's once saved (FAR clears `FFILEEDIT_DELETEONCLOSE` on a save).
#[test]
fn a_file_deleted_on_close_waits_for_its_window_and_a_saved_one_stays() {
    let temp = tempfile::tempdir().unwrap();
    let (shown, saved) = (temp.path().join("out.txt"), temp.path().join("kept.txt"));
    std::fs::write(&shown, "output").unwrap();
    std::fs::write(&saved, "edited").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.plugins.delete_on_close = vec![shown.clone(), saved.clone()];
    app.view_file(&shown, false);
    app.keep_saved_file(&saved);
    app.handle_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Tab,
        ratatui::crossterm::event::KeyModifiers::CONTROL,
    ));
    assert!(matches!(app.screen, crate::app::Screen::Panels));
    app.delete_closed_files();
    assert!(shown.exists(), "its viewer behind still has it");
    assert!(saved.exists(), "saved: the user's");
    while !app.window_lines().is_empty() {
        let id = app.window_lines()[0].id;
        app.drop_window(id);
    }
    app.delete_closed_files();
    assert!(!shown.exists(), "its window closed: deleted");
    assert!(saved.exists());
}
