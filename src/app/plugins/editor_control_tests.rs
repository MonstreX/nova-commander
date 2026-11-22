//! A plugin's editor commands (API 10): F11 in the editor lists the plugins' editor items; one chosen
//! runs with the editor up and reads and changes it through `NcHost::editor` (FAR's `EditorControl`).
#![cfg(windows)]

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::dialog::Dialog;
use crate::app::fileops::tests::app_in;
use crate::app::{App, Overlay, Screen};
use crate::plugin_host::tests::{install_probe, load as load_plugins};

/// The probe installed as `name`, loaded, the editor up on `text`.
fn editing(name: &str, text: &str) -> (tempfile::TempDir, App, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    install_probe(temp.path(), name, &manifest);
    let path = temp.path().join("e.txt");
    std::fs::write(&path, text).unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.edit_file(&path, false);
    settle(&mut app);
    (temp, app, path)
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Ticks until the editor is read and no plugin's call is waited for.
fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        let ready = match &mut app.screen {
            Screen::Editor(editor) => {
                editor.text.room.set((40, 5));
                editor.text.place().is_some() && editor.text.settled()
            }
            _ => true,
        };
        // A call that showed a window is `key_late` until it ends.
        let quiet = app.plugins.editor_asks.is_empty()
            && app.plugins.key_wait.is_none()
            && app.plugins.key_late.is_empty();
        if ready && !app.is_waiting() && quiet {
            return;
        }
        assert!(Instant::now() < deadline, "never done");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// F11's item `label` chosen.
fn run(app: &mut App, label: &str) {
    press(app, KeyCode::F(11));
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        let shown = match &app.overlay {
            Some(Overlay::Dialog(Dialog::Message { lines, .. })) => format!("{lines:?}"),
            Some(_) => "another window".to_string(),
            None => match &app.screen {
                Screen::Editor(editor) => format!("nothing, pending {:?}", editor.pending),
                _ => "no editor".to_string(),
            },
        };
        panic!("the editor's Plugin commands, not {shown}");
    };
    let at = (menu.lines.iter())
        .position(|line| line.text.contains(label))
        .unwrap_or_else(|| panic!("{label} in {:?}", menu.lines));
    for _ in 0..at {
        press(app, KeyCode::Down);
    }
    press(app, KeyCode::Enter);
    settle(app);
}

fn view(app: &App) -> &crate::editor::view::edit_view::EditView {
    match &app.screen {
        Screen::Editor(editor) => editor.text.view().unwrap(),
        _ => panic!("the editor"),
    }
}

#[test]
fn f11_in_the_editor_runs_a_plugin_s_command_on_the_editor() {
    let (_temp, mut app, path) = editing("edone", "one\r\ntwo\r\n");
    press(&mut app, KeyCode::F(11));
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the editor's Plugin commands");
    };
    let lines: Vec<&str> = menu.lines.iter().map(|line| line.text.as_str()).collect();
    assert!(
        lines.iter().any(|line| line.contains("Upper line")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("Say hello")),
        "the panels' commands not"
    );
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Down);
    settle(&mut app);
    run(&mut app, "Upper line");
    run(&mut app, "Mark start");
    assert_eq!(
        view(&app).block().unwrap(),
        Some((5, 8)),
        "TWO's first three"
    );
    press(&mut app, KeyCode::End);
    settle(&mut app);
    run(&mut app, "Two lines");
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one\r\nTWOa\r\nb\r\n"
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one\r\nTWO\r\n",
        "the two lines one undo step"
    );
}

/// The plugin's own menu up (a window it asked for while its command runs).
fn plugin_menu_up(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(app.overlay, Some(Overlay::PluginMenu(_))) {
        assert!(Instant::now() < deadline, "the plugin's menu never came");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn editcase_converts_the_word_at_the_cursor_and_the_block() {
    let temp = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(temp.path(), "editcase", "editcaseone");
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "one two\r\nthree four\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.edit_file(&path, false);
    settle(&mut app);
    let change = |app: &mut App, hotkey: char| {
        press(app, KeyCode::F(11));
        press(app, KeyCode::Enter);
        plugin_menu_up(app);
        press(app, KeyCode::Char(hotkey));
        settle(app);
    };
    for _ in 0..5 {
        press(&mut app, KeyCode::Right);
    }
    settle(&mut app);
    change(&mut app, 'u');
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one TWO\r\nthree four\r\n",
        "the word at the cursor"
    );
    let shift = KeyModifiers::SHIFT;
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Home);
    app.handle_key(KeyEvent::new(KeyCode::End, shift));
    settle(&mut app);
    change(&mut app, 't');
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one TWO\r\nThree Four\r\n",
        "the word at the cursor, then the block as Title Case"
    );
}

#[test]
fn brackets_finds_the_pair_and_marks_through_it() {
    let temp = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(temp.path(), "brackets", "bracketsone");
    let path = temp.path().join("b.txt");
    std::fs::write(&path, "f(a, [b])\r\nx\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.edit_file(&path, false);
    settle(&mut app);
    let matching = |app: &mut App, hotkey: char| {
        press(app, KeyCode::F(11));
        press(app, KeyCode::Enter);
        plugin_menu_up(app);
        press(app, KeyCode::Char(hotkey));
        settle(app);
    };
    press(&mut app, KeyCode::Right);
    settle(&mut app);
    matching(&mut app, 'e');
    assert_eq!(view(&app).cursor(), (0, 8), "Search: on the pair");
    matching(&mut app, 's');
    assert_eq!(
        view(&app).cursor(),
        (0, 1),
        "back on the first, the pair's pair"
    );
    assert_eq!(
        view(&app).block().unwrap(),
        Some((1, 9)),
        "Select: from the pair through the bracket"
    );
}

#[test]
fn align_reformats_the_block_at_the_margin_as_one_undo_step() {
    let temp = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(temp.path(), "align", "alignone");
    let path = temp.path().join("a.txt");
    std::fs::write(&path, "  one two\r\n    three\r\nnext\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.edit_file(&path, false);
    settle(&mut app);
    for _ in 0..2 {
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    }
    settle(&mut app);
    press(&mut app, KeyCode::F(11));
    press(&mut app, KeyCode::Enter);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(app.overlay, Some(Overlay::Dialog(_))) {
        assert!(Instant::now() < deadline, "Align's dialog never came");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    press(&mut app, KeyCode::Enter);
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "  one two three\r\nnext\r\n",
        "joined under the least indent"
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "  one two\r\n    three\r\nnext\r\n",
        "one undo step"
    );
}

#[test]
fn autowrap_breaks_a_line_typed_past_the_margin() {
    let temp = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(temp.path(), "autowrap", "wrapone");
    let path = temp.path().join("w.txt");
    std::fs::write(&path, "aaa bbb ccc\r\n").unwrap();
    let mut settings = serde_json::Map::new();
    let setting = |key: &str| format!("plugins.wrapone.settings.{key}");
    settings.insert(setting("Wrap"), serde_json::Value::from("1"));
    settings.insert(setting("RightMargin"), serde_json::Value::from(10));
    let mut app = app_in(temp.path());
    let plugins = crate::plugin_host::PluginHost::load(
        temp.path(),
        &temp.path().join("Logs"),
        &settings,
        &|_| true,
    );
    app.set_plugins(plugins);
    app.edit_file(&path, false);
    settle(&mut app);
    press(&mut app, KeyCode::End);
    settle(&mut app);
    for ch in " ddd".chars() {
        press(&mut app, KeyCode::Char(ch));
        settle(&mut app);
    }
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "aaa bbb \r\nccc ddd\r\n",
        "broken at the last blank before the margin"
    );
    assert_eq!(
        view(&app).cursor(),
        (10, 7),
        "at the wrapped line's end (its start's byte, the column)"
    );
}

/// Review D10: the first character typed in an editor (no line known before it) is wrapped too.
#[test]
fn autowrap_wraps_the_first_character_typed() {
    let temp = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(temp.path(), "autowrap", "wrapfirst");
    let path = temp.path().join("w.txt");
    std::fs::write(&path, "aaa bbb ccc\r\n").unwrap();
    let mut settings = serde_json::Map::new();
    let setting = |key: &str| format!("plugins.wrapfirst.settings.{key}");
    settings.insert(setting("Wrap"), serde_json::Value::from("1"));
    settings.insert(setting("RightMargin"), serde_json::Value::from(10));
    let mut app = app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    // At the line's end before the plugin hears any key.
    press(&mut app, KeyCode::End);
    settle(&mut app);
    let plugins = crate::plugin_host::PluginHost::load(
        temp.path(),
        &temp.path().join("Logs"),
        &settings,
        &|_| true,
    );
    app.set_plugins(plugins);
    press(&mut app, KeyCode::Char('d'));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "aaa bbb \r\ncccd\r\n"
    );
}

/// Ticks until a plugin's key loop waits for the next key with the editor's steps done.
fn reading(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        let settled = match &app.screen {
            Screen::Editor(editor) => editor.text.settled(),
            _ => false,
        };
        if settled && app.plugin_reads_input() {
            return;
        }
        assert!(Instant::now() < deadline, "the plugin never read a key");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn plugin_bars(app: &App) -> &crate::app::editing::PluginBars {
    match &app.screen {
        Screen::Editor(editor) => &editor.plugin_bars,
        _ => panic!("the editor"),
    }
}

#[test]
fn drawline_draws_box_lines_with_shift_and_the_arrows_as_one_undo_step() {
    let temp = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(temp.path(), "drawline", "drawone");
    let path = temp.path().join("d.txt");
    // UTF-8 (its BOM): the box characters saved as they are.
    std::fs::write(&path, "\u{FEFF}ab\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.edit_file(&path, false);
    settle(&mut app);
    press(&mut app, KeyCode::F(11));
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the editor's Plugin commands");
    };
    // After the program's own "Colorer".
    let at = (menu.lines.iter())
        .position(|line| line.text.contains("Draw lines"))
        .unwrap();
    for _ in 0..at {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Enter);
    reading(&mut app);
    let bars = plugin_bars(&app);
    assert_eq!(bars.title.as_deref(), Some("Shift→←↑↓ draw ┼"));
    let plain = crate::platform::input::Modifiers::default();
    assert!(bars.labels.contains(&(2, plain, "╠═╬═╣".to_string())));
    let shift = KeyModifiers::SHIFT;
    for code in [
        KeyCode::Right,
        KeyCode::Right,
        KeyCode::Right,
        KeyCode::Down,
        KeyCode::Down,
    ] {
        app.handle_key(KeyEvent::new(code, shift));
        reading(&mut app);
    }
    press(&mut app, KeyCode::Esc);
    settle(&mut app);
    assert_eq!(plugin_bars(&app).title, None, "the file's name again");
    assert!(plugin_bars(&app).labels.is_empty(), "the editor's own keys");
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "\u{FEFF}──┐\r\n  │\r\n",
        "a line right, then down through the corner; the last line set gets the file's break, as \
         FAR's ECTL_SETSTRING gives it (review D4)"
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "\u{FEFF}ab\r\n",
        "the drawing one undo step"
    );
}

/// Review D3: the commands' menu brought back (after F3's information, F4's hotkey, Shift+F9's
/// settings) over the editor is the editor's F11 menu, not the panels'.
#[test]
fn the_plugins_menu_back_over_the_editor_is_the_editors() {
    let (_temp, mut app, _) = editing("probe", "one\r\n");
    app.open_program_plugin_menu(crate::app::plugins::hotkeys::HotkeyMenu::Commands, 0);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("no menu");
    };
    assert!(
        menu.editor,
        "the editor's items, Enter runs them for the editor"
    );
}

/// Review D2: a request of a call begun in one editor, coming with another window in front (F12 or
/// Ctrl+Tab meanwhile), is not answered against that one: no editor for it.
#[test]
fn a_request_of_another_editors_call_fails() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, "one\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    let asked = |app: &mut App| {
        let (reply, answer) = std::sync::mpsc::channel();
        app.plugin_editor_ask(
            crate::plugin_host::editor_requests::EditorRequest::Info,
            reply,
        );
        answer.recv_timeout(Duration::from_secs(5)).unwrap()
    };
    app.plugins.editor_window = Some(app.front_window());
    assert!(matches!(
        asked(&mut app),
        crate::plugin_host::editor_requests::EditorAnswer::Info(_)
    ));
    app.plugins.editor_window = Some(app.front_window() + 1);
    assert!(matches!(
        asked(&mut app),
        crate::plugin_host::editor_requests::EditorAnswer::Done(false)
    ));
    // R7 review A6: refused before anything of it is done - the editor in front keeps its title.
    let (reply, answer) = std::sync::mpsc::channel();
    let title = Some("the other call's".to_string());
    let request = crate::plugin_host::editor_requests::EditorRequest::SetTitle(title);
    app.plugin_editor_ask(request, reply);
    assert!(matches!(
        answer.recv_timeout(Duration::from_secs(5)).unwrap(),
        crate::plugin_host::editor_requests::EditorAnswer::Done(false)
    ));
    assert_eq!(plugin_bars(&app).title, None);
}

/// Review D2: a key pressed after the plugin's window closed, its call still running, waits for the
/// call's end as after FAR's synchronous call, not typed among the plugin's changes.
#[test]
fn keys_are_held_again_once_the_plugins_window_is_gone() {
    let (_temp, mut app, path) = editing("probe", "one\r\n");
    press(&mut app, KeyCode::F(11));
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the editor's Plugin commands");
    };
    let at = (menu.lines.iter())
        .position(|line| line.text.contains("Ask, then type"))
        .unwrap();
    for _ in 0..at {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Enter);
    let deadline = Instant::now() + Duration::from_secs(10);
    let message = |app: &App| {
        matches!(
            &app.overlay,
            Some(Overlay::Dialog(Dialog::Form {
                action: crate::app::dialog::FormAction::Plugin(
                    crate::app::plugins::PluginForm::Message { .. }
                ),
                ..
            }))
        )
    };
    while !message(&app) {
        assert!(Instant::now() < deadline, "the probe's message never came");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('q'));
    settle(&mut app);
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "zqone\r\n",
        "the plugin's z first, then the q held for it"
    );
}

/// Review D11: an editor key's plugins hear it one after another, as FAR's `ProcessEditorInput` calls
/// them; the keys typed meanwhile wait for the last one, not only for the one told it last.
#[test]
fn an_editor_key_goes_to_its_plugins_in_turn() {
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
    let rest = app.plugins.editor_key_rest.as_ref().unwrap();
    assert_eq!(rest.takers.len(), 1, "the second not yet");
    press(&mut app, KeyCode::Char('y'));
    let deadline = Instant::now() + Duration::from_secs(10);
    while waited(&app).as_deref() != Some("wrapb") {
        assert!(Instant::now() < deadline, "the second never told");
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    let held = &app.plugins.key_wait.as_ref().unwrap().held.keys;
    assert_eq!(held.len(), 1, "y still held for the second");
    settle(&mut app);
    assert!(app.plugins.editor_key_rest.is_none());
    press(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "xyone\r\n");
}

#[path = "editor_control_tests_keys.rs"]
mod keys;
