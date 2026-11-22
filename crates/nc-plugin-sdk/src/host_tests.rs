//! What the SDK's windows hand the host's table and how they read its answers (a host faked in this
//! process: its functions note each call and answer as the test says).

use std::ffi::c_void;
use std::sync::Mutex;

use super::*;
use crate::api::NcHost;

/// What the fake host heard, and what it answers.
#[derive(Default)]
struct Fake {
    answer: i32,
    /// A menu's break key answered.
    key: i32,
    /// An input line's text answered.
    text: String,
    heard: Mutex<Vec<String>>,
}

/// The fake behind the host's pointer.
///
/// # Safety
/// `context` is a live `Fake`.
unsafe fn fake<'a>(context: *mut c_void) -> &'a Fake {
    // SAFETY: as the caller promises.
    unsafe { &*context.cast::<Fake>() }
}

/// A plugin's string, copied.
///
/// # Safety
/// `raw` is valid UTF-8 alive through the call.
unsafe fn copied(raw: NcStr) -> String {
    // SAFETY: as the caller promises.
    unsafe { raw.as_str() }.to_string()
}

unsafe extern "C" fn message(
    context: *mut c_void,
    flags: u32,
    title: NcStr,
    text: NcStr,
    buttons: NcStr,
) -> i32 {
    // SAFETY: the tests' context is a live `Fake`; the plugin's strings live through the call.
    unsafe {
        let fake = fake(context);
        let call = format!(
            "message {flags} {} [{}] [{}]",
            copied(title),
            copied(text),
            copied(buttons)
        );
        fake.heard.lock().unwrap().push(call);
        fake.answer
    }
}

unsafe extern "C" fn menu(context: *mut c_void, menu: *const api::NcMenu, key: *mut i32) -> i32 {
    // SAFETY: the tests' context is a live `Fake`; the plugin's menu lives through the call.
    unsafe {
        let fake = fake(context);
        let menu = &*menu;
        assert_eq!(menu.size, size_of::<api::NcMenu>());
        let items = std::slice::from_raw_parts(menu.items, menu.count);
        let keys = std::slice::from_raw_parts(menu.break_keys, menu.break_count);
        let items: Vec<String> = (items.iter())
            .map(|item| format!("{}:{}", copied(item.text), item.flags))
            .collect();
        let keys: Vec<String> = keys.iter().map(|key| copied(*key)).collect();
        let mut call = format!(
            "menu {} {} [{}] {} {} [{}]",
            menu.flags,
            copied(menu.title),
            copied(menu.bottom),
            items.join(","),
            menu.selected,
            keys.join(",")
        );
        if (menu.x, menu.y, menu.max_height) != (-1, -1, 0) {
            call.push_str(&format!(
                " at {} {} most {}",
                menu.x, menu.y, menu.max_height
            ));
        }
        fake.heard.lock().unwrap().push(call);
        *key = fake.key;
        fake.answer
    }
}

unsafe extern "C" fn input(
    context: *mut c_void,
    title: NcStr,
    prompt: NcStr,
    initial: NcStr,
    history: NcStr,
    flags: u32,
    text: *mut NcStr,
) -> i32 {
    // SAFETY: the tests' context is a live `Fake`; the answer is the fake's, alive while it is.
    unsafe {
        let fake = fake(context);
        let mut call = format!(
            "input {flags} {} [{}] [{}]",
            copied(title),
            copied(prompt),
            copied(initial)
        );
        let history = copied(history);
        if !history.is_empty() {
            call.push_str(&format!(" history {history}"));
        }
        fake.heard.lock().unwrap().push(call);
        *text = NcStr::new(&fake.text);
        fake.answer
    }
}

unsafe extern "C" fn log(context: *mut c_void, text: NcStr) {
    // SAFETY: the tests' context is a live `Fake`; the plugin's text lives through the call.
    unsafe {
        let call = format!("log [{}]", copied(text));
        fake(context).heard.lock().unwrap().push(call);
    }
}

unsafe extern "C" fn progress(context: *mut c_void, done: u64, total: u64, text: NcStr) {
    // SAFETY: the tests' context is a live `Fake`; the plugin's text lives through the call.
    unsafe {
        let call = format!("progress {done}/{total} [{}]", copied(text));
        fake(context).heard.lock().unwrap().push(call);
    }
}

unsafe extern "C" fn cancelled(context: *mut c_void) -> i32 {
    // SAFETY: the tests' context is a live `Fake`.
    unsafe { fake(context).answer }
}

/// `act` on a host faked by `fake`: what it returned and what the host heard.
fn with_host<T>(fake: Fake, act: impl FnOnce(Host) -> T) -> (T, Vec<String>) {
    let table = NcHost {
        api_version: api::API_VERSION,
        size: size_of::<NcHost>(),
        abi: api::ABI,
        context: (&raw const fake).cast_mut().cast(),
        folder: NcStr::new(r"C:\plugins\fake"),
        message,
        menu,
        input,
        log,
        dialog: crate::tests::no_dialog,
        dialog_change: crate::tests::no_dialog_change,
        setting: crate::tests::no_setting,
        set_setting: crate::tests::no_set_setting,
        decode: crate::tests::no_decode,
        progress,
        cancelled,
        panel_state: crate::tests::no_panel_state,
        panel_items: crate::tests::no_panel_items,
        panel_do: crate::tests::no_panel_do,
        synchro: crate::program::tests::no_synchro,
        post_keys: crate::program::tests::no_post_keys,
        screen: crate::program::tests::no_screen,
        filter: crate::program::tests::no_filter,
        clipboard: crate::program::tests::no_clipboard,
        show_help: crate::program::tests::no_show_help,
        plugins_control: crate::program::tests::no_plugins_control,
        confirmation: crate::program::tests::no_confirmation,
        match_masks: crate::program::tests::no_match_masks,
        resume: crate::program::tests::no_resume,
        data_folder: NcStr::new(r"C:\nc\settings\Plugins\fake"),
        transfer: crate::program::tests::no_transfer,
        editor: crate::editor::tests::fake_editor,
    };
    let result = act(Host {
        table: &raw const table,
    });
    (result, fake.heard.into_inner().unwrap())
}

fn answering(answer: i32) -> Fake {
    Fake {
        answer,
        ..Fake::default()
    }
}

/// The plugin's own folder and its data folder (API 7) are the host's texts, apart.
#[test]
fn the_folder_and_the_data_folder_are_the_hosts() {
    let ((folder, data), heard) =
        with_host(Fake::default(), |host| (host.folder(), host.data_folder()));
    assert_eq!(folder, std::path::Path::new(r"C:\plugins\fake"));
    assert_eq!(data, std::path::Path::new(r"C:\nc\settings\Plugins\fake"));
    assert!(heard.is_empty(), "{heard:?}");
}

#[test]
fn a_message_hands_its_lines_buttons_and_flags_and_reads_the_button() {
    let message = Message::new("Q", &["one", "two"]).buttons(&["&Yes", "&No"]);
    let (answer, heard) = with_host(answering(1), |host| host.show(&message));
    assert_eq!(answer, Some(1));
    assert_eq!(heard, ["message 0 Q [one\ntwo] [&Yes\n&No]"]);
    let flagged = message.clone().warning().left_align();
    let (answer, heard) = with_host(answering(-1), |host| host.show(&flagged));
    assert_eq!(answer, None, "Esc");
    assert_eq!(
        heard,
        ["message 3 Q [one\ntwo] [&Yes\n&No]"],
        "warning 1, left 2"
    );
    let (_, heard) = with_host(answering(0), |host| {
        host.show(&Message::new("Q", &["x"]).warning())
    });
    assert_eq!(heard, ["message 1 Q [x] []"]);
}

#[test]
fn message_is_ok_and_busy_has_no_buttons() {
    let (ok, heard) = with_host(answering(0), |host| host.message("T", &["a"]));
    assert!(ok);
    assert_eq!(heard, ["message 0 T [a] [OK]"]);
    let (ok, _) = with_host(answering(-1), |host| host.message("T", &["a"]));
    assert!(!ok, "Esc");
    let (ok, _) = with_host(answering(1), |host| host.message("T", &["a"]));
    assert!(!ok, "only OK is OK");
    let ((), heard) = with_host(answering(0), |host| host.busy("T", &["Working"]));
    assert_eq!(heard, ["message 0 T [Working] []"]);
}

fn probe_menu() -> Menu {
    Menu::new(
        "M",
        vec![
            MenuItem::new("&One"),
            MenuItem::separator(),
            MenuItem::new("Two").checked(true).disabled(true),
        ],
    )
    .bottom("keys")
    .selected(2)
    .break_keys(&["F4", "Shift+Del"])
}

#[test]
fn a_menu_hands_its_items_and_keys() {
    let (_, heard) = with_host(answering(-1), |host| host.menu(&probe_menu()));
    assert_eq!(
        heard,
        ["menu 0 M [keys] &One:0,:1,Two:6 2 [F4,Shift+Del]"],
        "separator 1, checked 2, disabled 4"
    );
    let (_, heard) = with_host(answering(-1), |host| {
        host.menu(&probe_menu().auto_hotkeys())
    });
    assert!(heard[0].starts_with("menu 1 M"), "{heard:?}");
    let (_, heard) = with_host(answering(-1), |host| {
        host.menu(&Menu::new("Empty", Vec::new()))
    });
    assert_eq!(heard, ["menu 0 Empty []  0 []"], "no items, no keys");
}

#[test]
fn a_menu_reads_the_line_and_the_break_key() {
    let menu = |answer: i32, key: i32| {
        let fake = Fake {
            answer,
            key,
            ..Fake::default()
        };
        with_host(fake, |host| host.menu(&probe_menu())).0
    };
    let chosen = |item, key| Some(Chosen { item, key });
    assert_eq!(menu(2, -1), chosen(Some(2), None), "Enter");
    assert_eq!(menu(0, 1), chosen(Some(0), Some(1)), "a break key");
    assert_eq!(menu(-1, 0), chosen(None, Some(0)), "a break key, no line");
    assert_eq!(menu(-1, -1), None, "Esc");
}

#[test]
fn an_input_line_hands_its_text_and_reads_the_answer() {
    let fake = Fake {
        answer: 0,
        text: "typed".to_string(),
        ..Fake::default()
    };
    let (typed, heard) = with_host(fake, |host| host.input("T", "Name:", "start"));
    assert_eq!(typed.as_deref(), Some("typed"));
    assert_eq!(heard, ["input 0 T [Name:] [start]"]);
    for (options, flags) in [
        (
            InputOptions {
                enable_empty: true,
                ..InputOptions::default()
            },
            1,
        ),
        (
            InputOptions {
                password: true,
                ..InputOptions::default()
            },
            2,
        ),
        (
            InputOptions {
                expand_env: true,
                ..InputOptions::default()
            },
            4,
        ),
        (
            InputOptions {
                buttons: true,
                ..InputOptions::default()
            },
            0x10,
        ),
        (
            InputOptions {
                no_ampersand: true,
                ..InputOptions::default()
            },
            0x20,
        ),
        (
            InputOptions {
                no_last_history: true,
                ..InputOptions::default()
            },
            0x8,
        ),
        (
            InputOptions {
                edit_path: true,
                ..InputOptions::default()
            },
            0x40,
        ),
    ] {
        let fake = Fake {
            answer: 0,
            ..Fake::default()
        };
        let (_, heard) = with_host(fake, |host| host.input_with("T", "P", "x", options));
        assert_eq!(heard, [format!("input {flags} T [P] [x]")], "{options:?}");
    }
    let fake = Fake {
        answer: -1,
        text: "ignored".to_string(),
        ..Fake::default()
    };
    assert_eq!(
        with_host(fake, |host| host.input("T", "", "")).0,
        None,
        "Esc"
    );
}

#[test]
fn a_log_line_is_handed_as_it_is() {
    let ((), heard) = with_host(Fake::default(), |host| host.log("one\ntwo"));
    assert_eq!(heard, ["log [one\ntwo]"]);
}

#[test]
fn only_a_hosts_zero_is_a_text_and_any_negative_answer_is_none() {
    let fake = Fake {
        answer: 1,
        text: "not an answer".to_string(),
        ..Fake::default()
    };
    assert_eq!(
        with_host(fake, |host| host.input("T", "", "")).0,
        None,
        "only 0 hands a text"
    );
    let fake = Fake {
        answer: -2,
        key: -7,
        ..Fake::default()
    };
    assert_eq!(
        with_host(fake, |host| host.menu(&probe_menu())).0,
        None,
        "no line and no key"
    );
    assert_eq!(
        with_host(answering(-2), |host| host
            .show(&Message::new("Q", &["x"]).buttons(&["OK"])))
        .0,
        None
    );
    assert!(!with_host(answering(-2), |host| host.message("T", &["a"])).0);
}

#[test]
fn a_message_hands_each_flag_by_itself() {
    let message = || Message::new("Q", &["x"]).buttons(&["OK"]);
    let (_, heard) = with_host(answering(0), |host| host.show(&message().left_align()));
    assert_eq!(heard, ["message 2 Q [x] [OK]"], "left only");
    let (_, heard) = with_host(answering(0), |host| host.show(&message().warning()));
    assert_eq!(heard, ["message 1 Q [x] [OK]"], "warning only");
    let (_, heard) = with_host(answering(0), |host| host.show(&message().error()));
    assert_eq!(heard, ["message 4 Q [x] [OK]"], "the system's error only");
    for (set, flags) in [
        (ButtonSet::Ok, 0x10),
        (ButtonSet::OkCancel, 0x20),
        (ButtonSet::AbortRetryIgnore, 0x30),
        (ButtonSet::YesNo, 0x40),
        (ButtonSet::YesNoCancel, 0x50),
        (ButtonSet::RetryCancel, 0x60),
    ] {
        let (_, heard) = with_host(answering(0), |host| host.show(&message().button_set(set)));
        assert_eq!(heard, [format!("message {flags} Q [x] [OK]")], "{set:?}");
    }
    let separated = Message::new("Q", &[Message::SEPARATOR, "x", Message::DOUBLE_SEPARATOR]);
    assert_eq!(separated.lines, ["\u{1}", "x", "\u{2}"]);
}

#[test]
fn a_menu_items_flags_are_handed_each_by_itself() {
    let menu = Menu::new(
        "M",
        vec![
            MenuItem::new("c").checked(true),
            MenuItem::new("d").disabled(true),
            MenuItem::new("n").checked(false).disabled(false),
        ],
    );
    let (_, heard) = with_host(answering(-1), |host| host.menu(&menu));
    assert_eq!(heard, ["menu 0 M [] c:2,d:4,n:0 0 []"]);
    let menu = Menu::new(
        "M",
        vec![
            MenuItem::new("g").grayed(true),
            MenuItem::new("h").hidden(true),
        ],
    );
    let (_, heard) = with_host(answering(-1), |host| host.menu(&menu));
    assert_eq!(heard, ["menu 0 M [] g:8,h:16 0 []"]);
    for (menu, flags) in [
        (menu.clone().wrap(), 2),
        (menu.clone().show_ampersand(), 4),
        (menu.clone().reverse_auto_hotkeys(), 8),
    ] {
        let (_, heard) = with_host(answering(-1), |host| host.menu(&menu));
        assert_eq!(heard, [format!("menu {flags} M [] g:8,h:16 0 []")]);
    }
}

#[test]
fn a_call_tells_its_progress_and_asks_whether_it_was_stopped() {
    let ((), heard) = with_host(Fake::default(), |host| host.progress(3, 10, "a.txt"));
    assert_eq!(heard, ["progress 3/10 [a.txt]"]);
    let (stopped, _) = with_host(answering(1), |host| host.cancelled());
    assert!(stopped);
    let (stopped, _) = with_host(answering(0), |host| host.cancelled());
    assert!(!stopped);
}

#[test]
fn a_menu_goes_where_the_plugin_puts_it_and_an_input_line_hands_its_history() {
    let (_, heard) = with_host(answering(-1), |host| {
        host.menu(&Menu::new("Placed", Vec::new()).at(4, 2).max_height(6))
    });
    assert_eq!(heard, ["menu 0 Placed []  0 [] at 4 2 most 6"]);
    let fake = Fake {
        answer: 0,
        ..Fake::default()
    };
    let (_, heard) = with_host(fake, |host| {
        host.input_history("T", "P", "", "Folders", InputOptions::default())
    });
    assert_eq!(heard, ["input 0 T [P] [] history Folders"]);
}
