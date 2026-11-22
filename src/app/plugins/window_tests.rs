//! A plugin's windows as the program shows them (ADR-0016 5b-1), the events sent as a plugin's thread sends
//! them: a message with its buttons, a window without buttons giving way, a menu, an input line, and what
//! the plugin hears back.

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::form::{Control, Form};
use crate::plugin_host::windows::{InputRequest, MenuLine, MenuRequest};

/// The program in a folder of its own (gone with the test).
fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// `request` asked by plugin `plugin`; the receiver of its answer.
fn ask(app: &App, plugin: &str, request: Request) -> Receiver<Answer> {
    let (reply, answer) = channel();
    app.plugins.host.send_event(Event::Ask {
        plugin: plugin.to_string(),
        request,
        reply,
    });
    answer
}

fn message(lines: &[&str], buttons: &[&str]) -> MessageRequest {
    MessageRequest {
        title: "Title".to_string(),
        lines: lines.iter().map(ToString::to_string).collect(),
        buttons: buttons.iter().map(ToString::to_string).collect(),
        ..MessageRequest::default()
    }
}

fn busy(app: &App, plugin: &str, line: &str) {
    app.plugins.host.send_event(Event::Busy {
        plugin: plugin.to_string(),
        message: message(&[line], &[]),
        call: 0,
    });
}

fn done(app: &App, plugin: &str) {
    app.plugins.host.send_event(Event::Done {
        plugin: plugin.to_string(),
    });
}

/// The form up, with its plugin's kind.
fn form(app: &App) -> (Form, PluginForm) {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Plugin(kind),
        })) => (form.clone(), *kind),
        _ => panic!("a plugin's form"),
    }
}

fn labels(form: &Form) -> Vec<(u16, String)> {
    (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Label(text) => Some((item.x, text.clone())),
            _ => None,
        })
        .collect()
}

fn buttons(form: &Form) -> Vec<String> {
    (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Button { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_message_shows_its_buttons_and_answers_the_one_pressed() {
    let (_temp, mut app) = app();
    let mut request = message(
        &["Go on?", "Really, all of it"],
        &["&Yes", "&No", "&Cancel"],
    );
    request.warning = true;
    let answer = ask(&app, "probe", Request::Message(request));
    assert!(app.pump_plugins());
    let (form, kind) = form(&app);
    assert!(form.warning, "the warning colours");
    assert_eq!(form.title, "Title");
    assert_eq!(buttons(&form), ["&Yes", "&No", "&Cancel"]);
    let shown = labels(&form);
    assert!(shown[0].0 > shown[1].0, "centred: {shown:?}");
    assert_eq!(
        kind,
        PluginForm::Message {
            first_button: form.items.len() - 3
        }
    );
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);
    assert_eq!(answer.try_recv(), Ok(Answer::Button(1)));
    assert!(app.overlay.is_none());
}

#[test]
fn a_message_from_the_left_esc_and_a_window_gone_are_esc() {
    let (_temp, mut app) = app();
    let mut request = message(&["Go on?", "Really, all of it"], &["OK"]);
    request.left_align = true;
    let answer = ask(&app, "probe", Request::Message(request));
    app.pump_plugins();
    let (form, _) = form(&app);
    let shown = labels(&form);
    assert_eq!(shown[0].0, shown[1].0, "from the left: {shown:?}");
    assert!(!form.warning);
    press(&mut app, KeyCode::Esc);
    assert_eq!(answer.try_recv(), Ok(Answer::Esc));
    let answer = ask(&app, "probe", Request::Message(message(&["x"], &["OK"])));
    app.pump_plugins();
    app.overlay = None;
    assert_eq!(answer.try_recv(), Err(TryRecvError::Empty));
    app.pump_plugins();
    assert_eq!(answer.try_recv(), Ok(Answer::Esc), "gone another way");
}

#[test]
fn a_window_without_buttons_stays_until_its_plugins_call_ends() {
    let (_temp, mut app) = app();
    busy(&app, "probe", "Working...");
    assert!(app.pump_plugins());
    let (form, kind) = form(&app);
    assert_eq!(kind, PluginForm::Busy);
    assert!(buttons(&form).is_empty());
    assert_eq!(labels(&form).len(), 1);
    done(&app, "other");
    assert!(!app.pump_plugins(), "another plugin's end");
    assert_eq!(form_kind(&app), PluginForm::Busy);
    done(&app, "probe");
    assert!(app.pump_plugins());
    assert!(app.overlay.is_none());
    assert!(app.plugins.busy.is_none());
}

#[test]
fn a_window_without_buttons_gives_way_to_the_next() {
    let (_temp, mut app) = app();
    busy(&app, "probe", "Working...");
    app.pump_plugins();
    busy(&app, "probe", "Still working...");
    app.pump_plugins();
    let (shown, _) = form(&app);
    assert_eq!(
        labels(&shown)[0].1,
        "Still working...",
        "another in its place"
    );
    let answer = ask(&app, "other", Request::Message(message(&["Hi"], &["OK"])));
    assert!(app.pump_plugins());
    assert!(matches!(form_kind(&app), PluginForm::Message { .. }));
    assert!(app.plugins.busy.is_none());
    done(&app, "probe");
    assert!(!app.pump_plugins(), "the message stays");
    press(&mut app, KeyCode::Enter);
    assert_eq!(answer.try_recv(), Ok(Answer::Button(0)));
}

fn form_kind(app: &App) -> PluginForm {
    form(app).1
}

#[test]
fn a_window_without_buttons_closed_by_esc_is_not_shown_again() {
    let (_temp, mut app) = app();
    busy(&app, "probe", "Working...");
    app.pump_plugins();
    press(&mut app, KeyCode::Esc);
    assert!(app.overlay.is_none());
    done(&app, "probe");
    assert!(!app.pump_plugins());
    assert!(app.overlay.is_none() && app.plugins.busy.is_none());
}

#[test]
fn a_window_without_buttons_waits_behind_the_programs_own() {
    let (_temp, mut app) = app();
    let own = Dialog::message("Own", vec![]);
    app.show_dialog(own.clone());
    busy(&app, "probe", "Working...");
    assert!(!app.pump_plugins());
    assert!(matches!(&app.overlay, Some(Overlay::Dialog(dialog)) if *dialog == own));
    app.overlay = None;
    assert!(app.pump_plugins());
    assert_eq!(form_kind(&app), PluginForm::Busy);
}

fn line(text: &str) -> MenuLine {
    MenuLine {
        text: text.to_string(),
        ..MenuLine::default()
    }
}

fn menu_request() -> MenuRequest {
    MenuRequest {
        title: "Menu".to_string(),
        lines: vec![line("&One"), line("&Two")],
        break_keys: vec!["F4".to_string()],
        ..MenuRequest::default()
    }
}

#[test]
fn a_menu_answers_the_line_chosen_and_the_break_key() {
    let (_temp, mut app) = app();
    let answer = ask(&app, "probe", Request::Menu(menu_request()));
    assert!(app.pump_plugins());
    assert!(matches!(app.overlay, Some(Overlay::PluginMenu(_))));
    press(&mut app, KeyCode::Down);
    assert!(matches!(app.overlay, Some(Overlay::PluginMenu(_))), "stays");
    press(&mut app, KeyCode::Enter);
    let chosen = |item, key| Answer::Chosen {
        item: Some(item),
        key,
    };
    assert_eq!(answer.try_recv(), Ok(chosen(1, None)));
    assert!(app.overlay.is_none());
    let answer = ask(&app, "probe", Request::Menu(menu_request()));
    app.pump_plugins();
    press(&mut app, KeyCode::F(4));
    assert_eq!(answer.try_recv(), Ok(chosen(0, Some(0))));
    let answer = ask(&app, "probe", Request::Menu(menu_request()));
    app.pump_plugins();
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(answer.try_recv(), Ok(chosen(1, None)), "a hotkey");
}

#[test]
fn a_menu_closed_by_esc_or_gone_is_esc() {
    let (_temp, mut app) = app();
    let answer = ask(&app, "probe", Request::Menu(menu_request()));
    app.pump_plugins();
    press(&mut app, KeyCode::Esc);
    assert_eq!(answer.try_recv(), Ok(Answer::Esc));
    assert!(app.overlay.is_none());
    let answer = ask(&app, "probe", Request::Menu(menu_request()));
    app.pump_plugins();
    app.overlay = None;
    app.pump_plugins();
    assert_eq!(answer.try_recv(), Ok(Answer::Esc));
}

fn input_request() -> InputRequest {
    InputRequest {
        title: "Probe".to_string(),
        prompt: "Name:".to_string(),
        text: "start".to_string(),
        ..InputRequest::default()
    }
}

#[test]
fn an_input_line_starts_with_the_text_and_answers_what_was_typed() {
    let (_temp, mut app) = app();
    let answer = ask(&app, "probe", Request::Input(input_request()));
    assert!(app.pump_plugins());
    let (shown, kind) = form(&app);
    assert_eq!(
        kind,
        PluginForm::Input {
            enable_empty: false,
            expand_env: false
        }
    );
    assert_eq!(shown.title, "Probe");
    assert_eq!(labels(&shown)[0].1, "Name:");
    assert_eq!(shown.edit_text(1), "start");
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(answer.try_recv(), Ok(Answer::Text("startx".to_string())));
    assert!(app.overlay.is_none());
    let answer = ask(&app, "probe", Request::Input(input_request()));
    app.pump_plugins();
    press(&mut app, KeyCode::Esc);
    app.pump_plugins();
    assert_eq!(answer.try_recv(), Ok(Answer::Esc));
}

#[test]
fn windows_come_one_at_a_time_in_their_order() {
    let (_temp, mut app) = app();
    let first = ask(&app, "a", Request::Menu(menu_request()));
    let second = ask(&app, "b", Request::Input(input_request()));
    app.pump_plugins();
    assert!(matches!(app.overlay, Some(Overlay::PluginMenu(_))));
    assert!(!app.pump_plugins(), "the second waits");
    press(&mut app, KeyCode::Esc);
    assert_eq!(first.try_recv(), Ok(Answer::Esc));
    assert!(app.pump_plugins());
    assert!(matches!(form(&app).1, PluginForm::Input { .. }));
    press(&mut app, KeyCode::Enter);
    assert_eq!(second.try_recv(), Ok(Answer::Text("start".to_string())));
}

/// The probe's window commands through the program, as the user meets them.
#[cfg(windows)]
#[test]
fn the_probes_menu_is_shown_and_its_break_key_heard() {
    let folder = crate::plugin_host::tests::probe_with_markers(&["windows"]);
    let (_temp, mut app) = app();
    app.set_plugins(crate::plugin_host::tests::load(folder.path(), &|_| true));
    app.run_command_for_test(0, 4, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !matches!(app.overlay, Some(Overlay::PluginMenu(_))) {
        app.pump_plugins();
        assert!(std::time::Instant::now() < deadline, "no menu");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        unreachable!();
    };
    let texts: Vec<&str> = menu.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        ["&First", "", "&Grey", "&Third"],
        "automatic hotkeys"
    );
    assert_eq!(menu.selected, 3);
    press(&mut app, KeyCode::Up);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the menu");
    };
    assert_eq!(menu.selected, 0, "past the grey line and the separator");
    app.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::SHIFT));
    while !matches!(app.overlay, Some(Overlay::Dialog(Dialog::Form { .. }))) {
        app.pump_plugins();
        assert!(std::time::Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let (form, _) = form(&app);
    assert_eq!(form.title, "Answer");
    assert_eq!(
        labels(&form)[0].1,
        "Some(Chosen { item: Some(0), key: Some(1) })"
    );
    press(&mut app, KeyCode::Enter);
}

#[test]
fn a_message_longer_than_the_screen_keeps_the_lines_that_fit() {
    let (_temp, mut app) = app();
    let lines: Vec<String> = (0..100).map(|line| format!("line {line}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let answer = ask(&app, "probe", Request::Message(message(&refs, &["OK"])));
    app.pump_plugins();
    let (form, _) = form(&app);
    let shown = labels(&form);
    assert_eq!(
        shown.len(),
        23,
        "30 rows less the box, separator, buttons and margins"
    );
    assert_eq!(shown[0].1, "line 0");
    assert!(form.height <= 30, "{}", form.height);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        answer.try_recv(),
        Ok(Answer::Button(0)),
        "its buttons still there"
    );
}

#[test]
fn a_plugins_window_waits_for_work_the_screen_waits_for() {
    let (_temp, mut app) = app();
    app.overlay = None;
    let answer = ask(&app, "probe", Request::Menu(menu_request()));
    let (go, gate) = channel::<()>();
    crate::app::waiting::quick_as_the_program();
    app.wait_for(Vec::new(), move || {
        let _ = gate.recv();
        Box::new(|_: &mut App| {})
    });
    assert!(
        app.is_waiting(),
        "work the screen waits for, no window of its own"
    );
    assert!(!app.pump_plugins(), "keys typed meanwhile are the panels'");
    assert!(app.overlay.is_none());
    go.send(()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.is_waiting() {
        app.pump_waiting();
        assert!(std::time::Instant::now() < deadline, "the work did not end");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(app.pump_plugins());
    assert!(matches!(app.overlay, Some(Overlay::PluginMenu(_))));
    press(&mut app, KeyCode::Esc);
    assert_eq!(answer.try_recv(), Ok(Answer::Esc));
}

#[path = "window_edge_tests.rs"]
mod edge;

/// FAR's `GetString` answers: an empty line is none unless the plugin takes it, `%NAME%` replaced when
/// asked, "Cancel" none, "OK" the text.
#[test]
fn an_input_lines_answer_follows_its_options() {
    let (_temp, mut app) = app();
    app.environment.set("NC_TEST_NAME", "there");
    let answered = |app: &mut App, request: InputRequest, keys: &[KeyEvent]| {
        let answer = ask(app, "probe", Request::Input(request));
        app.pump_plugins();
        for key in keys {
            app.handle_key(*key);
        }
        app.pump_plugins();
        answer.try_recv()
    };
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    let clear = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL);
    let enter = key(KeyCode::Enter);
    assert_eq!(
        answered(&mut app, input_request(), &[clear, enter]),
        Ok(Answer::Esc),
        "empty: none"
    );
    let empty_taken = InputRequest {
        enable_empty: true,
        ..input_request()
    };
    assert_eq!(
        answered(&mut app, empty_taken, &[clear, enter]),
        Ok(Answer::Text(String::new()))
    );
    let expanded = InputRequest {
        text: "hi %NC_TEST_NAME%".to_string(),
        expand_env: true,
        ..input_request()
    };
    assert_eq!(
        answered(&mut app, expanded.clone(), &[enter]),
        Ok(Answer::Text("hi there".to_string()))
    );
    let secret = InputRequest {
        password: true,
        ..expanded
    };
    // FAR expands no password line (`dialog.cpp`).
    assert_eq!(
        answered(&mut app, secret, &[enter]),
        Ok(Answer::Text("hi %NC_TEST_NAME%".to_string()))
    );
    let with_buttons = InputRequest {
        buttons: true,
        ..input_request()
    };
    let tab = key(KeyCode::Tab);
    assert_eq!(
        answered(&mut app, with_buttons.clone(), &[enter]),
        Ok(Answer::Text("start".to_string())),
        "Enter is OK"
    );
    assert_eq!(
        answered(&mut app, with_buttons, &[tab, tab, enter]),
        Ok(Answer::Esc),
        "Cancel"
    );
}
