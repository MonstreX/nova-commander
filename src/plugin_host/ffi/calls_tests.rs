//! The host's functions as a plugin's thread calls them, without a library: what reaches the program, what
//! the plugin reads back, what is not shown (the plugin starting, nobody listening, a menu not read).

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Duration;

use super::*;
use crate::plugin_host::ffi::tests::answering_program;
use crate::plugin_host::log::PluginLog;
use crate::plugin_host::windows::InputRequest;

/// A plugin's context, started or not, its log in `logs`, and where its events come.
fn host_context(started: bool, logs: &Path) -> (HostContext, Receiver<Event>) {
    let (sender, events) = channel();
    let log = PluginLog::new(logs, "probe");
    let context = HostContext::new("probe", sender, Path::new(r"C:\plugins\probe"), log);
    context
        .started
        .store(started, std::sync::atomic::Ordering::Release);
    (context, events)
}

fn raw(context: &HostContext) -> *mut c_void {
    std::ptr::from_ref(context).cast_mut().cast()
}

/// The program: the next request is taken and `answer` sent (none: the window gone without one).
fn program(events: Receiver<Event>, answer: Option<Answer>) -> JoinHandle<Request> {
    std::thread::spawn(move || {
        let event = events
            .recv_timeout(Duration::from_secs(10))
            .expect("a request");
        let Event::Ask { request, reply, .. } = event else {
            panic!("a request: {event:?}");
        };
        if let Some(answer) = answer {
            reply.send(answer).unwrap();
        }
        request
    })
}

/// No log is written: under the test program's file no folder can be made.
fn no_logs() -> std::path::PathBuf {
    std::env::current_exe().unwrap().join("Logs")
}

fn message(context: &HostContext, flags: u32, body: &str, buttons: &str) -> i32 {
    // SAFETY: the context and the strings live through the call, made on this thread.
    unsafe {
        super::message(
            raw(context),
            flags,
            NcStr::new("Title"),
            NcStr::new(body),
            NcStr::new(buttons),
        )
    }
}

#[test]
fn a_message_hands_its_buttons_and_flags_and_returns_the_button() {
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Button(2)));
    let flags = nc_plugin_api::MESSAGE_WARNING | nc_plugin_api::MESSAGE_LEFT_ALIGN;
    assert_eq!(message(&context, flags, "a\nb", "&Yes\n&No\n&Cancel"), 2);
    let Request::Message(shown) = asked.join().unwrap() else {
        panic!("a message");
    };
    assert_eq!(shown.lines, ["a", "b"]);
    assert_eq!(shown.buttons, ["&Yes", "&No", "&Cancel"]);
    assert!(shown.warning && shown.left_align);
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Button(0)));
    message(&context, 0, "a", "OK");
    let Request::Message(shown) = asked.join().unwrap() else {
        panic!("a message");
    };
    assert!(!shown.warning && !shown.left_align, "no flags");
}

#[test]
fn a_message_answered_otherwise_than_by_a_button_is_esc() {
    for answer in [Some(Answer::Esc), None, Some(Answer::Text("x".to_string()))] {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, answer.clone());
        assert_eq!(message(&context, 0, "a", "OK"), -1, "{answer:?}");
        asked.join().unwrap();
    }
}

#[test]
fn a_message_without_buttons_is_shown_and_returns_at_once() {
    let (context, events) = host_context(true, &no_logs());
    assert_eq!(message(&context, 1, "Working", ""), 0);
    assert!(context.busy.get(), "up while the call runs");
    let Ok(Event::Busy {
        plugin,
        message: shown,
        ..
    }) = events.try_recv()
    else {
        panic!("a window without buttons");
    };
    assert_eq!(plugin, "probe");
    assert_eq!(shown.lines, ["Working"]);
    assert!(shown.buttons.is_empty() && shown.warning);
    // A window asked for next takes its place.
    let asked = program(events, Some(Answer::Button(0)));
    assert_eq!(message(&context, 0, "a", "OK"), 0);
    asked.join().unwrap();
    assert!(!context.busy.get());
}

#[test]
fn a_message_without_buttons_nobody_hears_is_not_up() {
    let (starting, events) = host_context(false, &no_logs());
    assert_eq!(message(&starting, 0, "Working", ""), -1, "while starting");
    assert!(events.try_recv().is_err() && !starting.busy.get());
    let (context, events) = host_context(true, &no_logs());
    drop(events);
    assert_eq!(message(&context, 0, "Working", ""), -1, "the program gone");
    assert!(!context.busy.get());
}

/// A menu of `items` with `keys`, as a plugin builds it.
struct RawMenu {
    texts: Vec<String>,
    items: Vec<NcMenuItem>,
    keys: Vec<NcStr>,
}

impl RawMenu {
    fn new(items: &[(&str, u32)], keys: &'static [&'static str]) -> Self {
        let texts: Vec<String> = items.iter().map(|(text, _)| text.to_string()).collect();
        let items = (texts.iter().zip(items))
            .map(|(text, (_, flags))| NcMenuItem {
                text: NcStr::new(text),
                flags: *flags,
            })
            .collect();
        let keys = keys.iter().map(|key| NcStr::new(key)).collect();
        Self { texts, items, keys }
    }

    fn table(&self) -> NcMenu {
        NcMenu {
            size: size_of::<NcMenu>(),
            flags: MENU_AUTO_HOTKEYS,
            title: NcStr::new("Menu"),
            bottom: NcStr::new("keys"),
            items: self.items.as_ptr(),
            count: self.items.len(),
            selected: 2,
            break_keys: self.keys.as_ptr(),
            break_count: self.keys.len(),
            x: 5,
            y: -1,
            max_height: 3,
        }
    }
}

fn call_menu(context: &HostContext, menu: *const NcMenu, key: *mut i32) -> i32 {
    // SAFETY: the context and the menu (or null) live through the call; `key` is writable or null.
    unsafe { super::menu(raw(context), menu, key) }
}

#[test]
fn a_menu_hands_its_lines_and_returns_the_line_and_the_key() {
    let raw_menu = RawMenu::new(
        &[
            ("&One", 0),
            ("", MENU_ITEM_SEPARATOR),
            ("Two", MENU_ITEM_CHECKED | MENU_ITEM_DISABLED),
        ],
        &["F4", "Shift+Del"],
    );
    assert_eq!(raw_menu.texts.len(), 3);
    let (context, events) = host_context(true, &no_logs());
    let answer = Answer::Chosen {
        item: Some(2),
        key: Some(1),
    };
    let asked = program(events, Some(answer));
    let mut key = 7;
    let table = raw_menu.table();
    assert_eq!(call_menu(&context, &raw const table, &raw mut key), 2);
    assert_eq!(key, 1);
    let Request::Menu(shown) = asked.join().unwrap() else {
        panic!("a menu");
    };
    let line = |text: &str, separator, checked, disabled| MenuLine {
        text: text.to_string(),
        separator,
        checked,
        disabled,
        ..MenuLine::default()
    };
    assert_eq!(
        shown,
        MenuRequest {
            title: "Menu".to_string(),
            bottom: "keys".to_string(),
            lines: vec![
                line("&One", false, false, false),
                line("", true, false, false),
                line("Two", false, true, true),
            ],
            selected: 2,
            break_keys: vec!["F4".to_string(), "Shift+Del".to_string()],
            auto_hotkeys: true,
            x: Some(5),
            max_height: Some(3),
            ..MenuRequest::default()
        },
        "a row of -1 centred"
    );
}

#[test]
fn a_menu_closed_by_enter_or_esc_tells_no_key() {
    let raw_menu = RawMenu::new(&[("One", 0)], &[]);
    let cases = [
        (
            Answer::Chosen {
                item: Some(0),
                key: None,
            },
            0,
        ),
        (Answer::Esc, -1),
        (Answer::Button(0), -1),
    ];
    for (answer, expected) in cases {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, Some(answer.clone()));
        let mut key = 7;
        let table = raw_menu.table();
        let item = call_menu(&context, &raw const table, &raw mut key);
        assert_eq!((item, key), (expected, -1), "{answer:?}");
        asked.join().unwrap();
    }
    let (context, events) = host_context(true, &no_logs());
    let answer = Answer::Chosen {
        item: None,
        key: Some(0),
    };
    let asked = program(events, Some(answer));
    let table = raw_menu.table();
    let item = call_menu(&context, &raw const table, std::ptr::null_mut());
    assert_eq!(item, -1, "no line; the key not asked for");
    asked.join().unwrap();
}

#[test]
fn a_menu_the_program_cannot_read_is_not_shown_and_logged() {
    let temp = tempfile::tempdir().unwrap();
    let raw_menu = RawMenu::new(&[("One", 0)], &["F4"]);
    let older = NcMenu {
        size: size_of::<NcMenu>() - 1,
        ..raw_menu.table()
    };
    let no_items = NcMenu {
        items: std::ptr::null(),
        ..raw_menu.table()
    };
    let no_keys = NcMenu {
        break_keys: std::ptr::null(),
        ..raw_menu.table()
    };
    let (context, events) = host_context(true, temp.path());
    for menu in [
        std::ptr::null(),
        &raw const older,
        &raw const no_items,
        &raw const no_keys,
    ] {
        let mut key = 7;
        assert_eq!(call_menu(&context, menu, &raw mut key), -1);
        assert_eq!(key, -1);
    }
    assert!(events.try_recv().is_err(), "nothing shown");
    let log = std::fs::read_to_string(temp.path().join("probe.log")).unwrap();
    assert_eq!(
        log.matches("A menu the program cannot read: not shown")
            .count(),
        4
    );
    let empty = NcMenu {
        items: std::ptr::null(),
        count: 0,
        break_keys: std::ptr::null(),
        break_count: 0,
        ..raw_menu.table()
    };
    let asked = program(events, Some(Answer::Esc));
    assert_eq!(
        call_menu(&context, &raw const empty, std::ptr::null_mut()),
        -1
    );
    let Request::Menu(shown) = asked.join().unwrap() else {
        panic!("a menu");
    };
    assert!(
        shown.lines.is_empty() && shown.break_keys.is_empty(),
        "nothing at null is read"
    );
}

#[test]
fn a_menu_while_starting_is_esc_at_once() {
    let raw_menu = RawMenu::new(&[("One", 0)], &[]);
    let (context, events) = host_context(false, &no_logs());
    let heard = answering_program(events);
    let mut key = 7;
    let table = raw_menu.table();
    assert_eq!(call_menu(&context, &raw const table, &raw mut key), -1);
    assert_eq!(key, -1);
    drop(context);
    assert!(!heard.join().unwrap(), "nothing shown");
}

fn call_input(context: &HostContext, out: *mut NcStr) -> i32 {
    // SAFETY: the context and the strings live through the call; `out` is writable or null.
    unsafe {
        super::input(
            raw(context),
            NcStr::new("Title"),
            NcStr::new("Name:"),
            NcStr::new("start"),
            NcStr::EMPTY,
            0,
            out,
        )
    }
}

#[test]
fn an_input_lines_history_is_its_name() {
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Esc));
    // SAFETY: the context and the strings live through the call; no place for the answer.
    let code = unsafe {
        super::input(
            raw(&context),
            NcStr::new("T"),
            NcStr::new("P"),
            NcStr::EMPTY,
            NcStr::new("Folders"),
            nc_plugin_api::INPUT_NO_LAST_HISTORY,
            std::ptr::null_mut(),
        )
    };
    assert_eq!(code, -1);
    let Request::Input(shown) = asked.join().unwrap() else {
        panic!("an input line");
    };
    assert_eq!(shown.history.as_deref(), Some("Folders"));
    assert!(shown.no_last_history);
}

#[test]
fn an_input_line_hands_its_text_and_returns_what_was_typed() {
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Text("typed".to_string())));
    let mut out = NcStr::EMPTY;
    assert_eq!(call_input(&context, &raw mut out), 0);
    // SAFETY: the host's answer lives in the context until its next answer.
    assert_eq!(unsafe { out.as_str() }, "typed");
    let Request::Input(shown) = asked.join().unwrap() else {
        panic!("an input line");
    };
    assert_eq!(
        shown,
        InputRequest {
            title: "Title".to_string(),
            prompt: "Name:".to_string(),
            text: "start".to_string(),
            ..InputRequest::default()
        }
    );
    assert_eq!(*context.answer.borrow(), "typed", "kept for the plugin");
}

#[test]
fn an_input_line_closed_otherwise_is_esc_and_a_null_answer_is_not_written() {
    for answer in [Some(Answer::Esc), None, Some(Answer::Button(0))] {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, answer.clone());
        let mut out = NcStr::new("unchanged");
        assert_eq!(call_input(&context, &raw mut out), -1, "{answer:?}");
        // SAFETY: the plugin's own text, not replaced.
        assert_eq!(unsafe { out.as_str() }, "unchanged");
        asked.join().unwrap();
    }
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Text("t".to_string())));
    assert_eq!(call_input(&context, std::ptr::null_mut()), 0);
    asked.join().unwrap();
    let (starting, events) = host_context(false, &no_logs());
    let heard = answering_program(events);
    assert_eq!(
        call_input(&starting, std::ptr::null_mut()),
        -1,
        "while starting"
    );
    drop(starting);
    assert!(!heard.join().unwrap(), "nothing shown");
}

#[test]
fn a_log_line_is_written_even_while_starting() {
    let temp = tempfile::tempdir().unwrap();
    let (context, events) = host_context(false, temp.path());
    // SAFETY: the context and the text live through the call.
    unsafe { super::log(raw(&context), NcStr::new("one\ntwo")) };
    let log = std::fs::read_to_string(temp.path().join("probe.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[0].ends_with(" one") && lines[1].ends_with(" two"),
        "{log}"
    );
    assert!(events.try_recv().is_err(), "nothing shown");
}

#[path = "calls_edge_tests.rs"]
mod edge;
