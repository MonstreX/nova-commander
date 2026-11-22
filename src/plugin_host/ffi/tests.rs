//! The raw boundary without a library: a plugin's table checked, its strings copied, the host's message
//! function as a plugin's thread calls it.

use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use nc_plugin_api::NcPluginInfo;

use super::*;
use crate::plugin_host::windows::{Answer, Request};

unsafe extern "C" fn no_info(_state: *mut c_void) -> NcPluginInfo {
    NcPluginInfo {
        title: NcStr::EMPTY,
        description: NcStr::EMPTY,
        author: NcStr::EMPTY,
        version: NcStr::EMPTY,
    }
}

unsafe extern "C" fn no_count(_state: *mut c_void, _place: u32) -> usize {
    0
}

unsafe extern "C" fn no_command(_state: *mut c_void, _place: u32, _index: usize) -> NcStr {
    NcStr::EMPTY
}

unsafe extern "C" fn no_run(
    _state: *mut c_void,
    _place: u32,
    _index: usize,
    _panel: *mut NcStr,
) -> i32 {
    0
}

unsafe extern "C" fn no_list(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _entries: *mut *const nc_plugin_api::NcEntry,
    _count: *mut usize,
) -> i32 {
    0
}

unsafe extern "C" fn no_panel_info(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
) -> nc_plugin_api::NcPanelInfo {
    nc_plugin_api::NcPanelInfo {
        title: NcStr::EMPTY,
        modes: std::ptr::null(),
        mode_count: 0,
        flags: 0,
        start_mode: nc_plugin_api::START_MODE_NONE,
        start_sort: nc_plugin_api::SORT_KEEP,
        start_order: nc_plugin_api::ORDER_ASCENDING,
        format: NcStr::EMPTY,
        keys: std::ptr::null(),
        key_count: 0,
        info_lines: std::ptr::null(),
        info_line_count: 0,
    }
}

unsafe extern "C" fn no_close(_state: *mut c_void, _instance: NcStr) {}

unsafe extern "C" fn no_stat(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _entry: *mut nc_plugin_api::NcEntry,
) -> i32 {
    0
}

unsafe extern "C" fn no_read(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _offset: u64,
    _buffer: *mut u8,
    _size: usize,
    _done: *mut usize,
) -> i32 {
    0
}

unsafe extern "C" fn no_write(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _mode: u32,
    _data: *const u8,
    _size: usize,
) -> i32 {
    0
}

unsafe extern "C" fn no_path_call(_state: *mut c_void, _instance: NcStr, _path: NcStr) -> i32 {
    0
}

unsafe extern "C" fn no_rename(
    _state: *mut c_void,
    _instance: NcStr,
    _from: NcStr,
    _to: NcStr,
    _replace: u32,
) -> i32 {
    0
}

unsafe extern "C" fn no_remove(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _how: u32,
) -> i32 {
    0
}

unsafe extern "C" fn no_key(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _event: *const nc_plugin_api::NcKeyEvent,
    _answer: *mut nc_plugin_api::NcKeyAnswer,
) -> i32 {
    0
}

unsafe extern "C" fn no_put(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _put: *const nc_plugin_api::NcPut,
) -> i32 {
    0
}

unsafe extern "C" fn no_get(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _get: *const nc_plugin_api::NcGet,
) -> i32 {
    nc_plugin_api::ERROR_UNSUPPORTED
}

unsafe extern "C" fn no_delete(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _names: *const NcStr,
    _count: usize,
    _mode: u32,
) -> i32 {
    nc_plugin_api::ERROR_UNSUPPORTED
}

unsafe extern "C" fn no_make_folder(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _mode: u32,
    _name: *mut NcStr,
) -> i32 {
    nc_plugin_api::ERROR_UNSUPPORTED
}

unsafe extern "C" fn no_open_new(_state: *mut c_void, _instance: *mut NcStr) -> i32 {
    0
}

unsafe extern "C" fn no_host_file(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _names: *const NcStr,
    _count: usize,
    _mode: u32,
    _done: *mut u32,
) -> i32 {
    0
}

unsafe extern "C" fn no_error(_state: *mut c_void) -> NcStr {
    NcStr::EMPTY
}

unsafe extern "C" fn no_release(_state: *mut c_void) {}

unsafe extern "C" fn no_open_command(
    _state: *mut c_void,
    _prefix: NcStr,
    _command: NcStr,
    _folder: NcStr,
    _answer: *mut nc_plugin_api::NcKeyAnswer,
) -> i32 {
    0
}

unsafe extern "C" fn no_open_file(
    _state: *mut c_void,
    _path: NcStr,
    _alternative: u32,
    _folder: NcStr,
    _answer: *mut nc_plugin_api::NcKeyAnswer,
) -> i32 {
    0
}

unsafe extern "C" fn no_find_list(
    _state: *mut c_void,
    _found: *const nc_plugin_api::NcEntry,
    _count: usize,
    _answer: *mut nc_plugin_api::NcKeyAnswer,
) -> i32 {
    0
}

unsafe extern "C" fn no_set_times(
    _state: *mut c_void,
    _instance: NcStr,
    _path: NcStr,
    _times: *const nc_plugin_api::NcTimes,
) -> i32 {
    0
}

unsafe extern "C" fn no_synchro(_state: *mut c_void, _param: u64) {}

unsafe extern "C" fn no_editor_key(_state: *mut c_void, _key: NcStr) -> i32 {
    0
}

/// A plugin's table telling `version` and `size`.
fn table(version: u32, size: usize) -> NcPlugin {
    NcPlugin {
        api_version: version,
        size,
        abi: nc_plugin_api::ABI,
        state: std::ptr::null_mut(),
        info: no_info,
        item_count: no_count,
        item: no_command,
        run_item: no_run,
        last_error: no_error,
        release: no_release,
        list: no_list,
        panel_info: no_panel_info,
        close_panel: no_close,
        stat: no_stat,
        read: no_read,
        write: no_write,
        make_dir: no_path_call,
        rename: no_rename,
        remove: no_remove,
        panel_key: no_key,
        put: no_put,
        open_command: no_open_command,
        open_file: no_open_file,
        find_list: no_find_list,
        set_times: no_set_times,
        synchro: no_synchro,
        get: no_get,
        delete_files: no_delete,
        make_folder: no_make_folder,
        open_new: no_open_new,
        process_host_file: no_host_file,
        editor_key: no_editor_key,
    }
}

#[test]
fn a_table_of_this_version_and_exactly_this_size_is_taken() {
    let own = size_of::<NcPlugin>();
    assert_eq!(
        check(std::ptr::null()),
        Err("The plugin did not start".to_string())
    );
    assert_eq!(check(std::ptr::from_ref(&table(API_VERSION, own))), Ok(()));
    let built = |size: usize| {
        format!(
            "The plugin was built for another build of API version {API_VERSION}: its table has \
             {size} bytes, this program's {own}"
        )
    };
    // Changed (review R3): a larger table was taken before, but the ABI's other structs carry no size of
    // their own, so a plugin of another build would write past the host's answers; now refused.
    assert_eq!(
        check(std::ptr::from_ref(&table(API_VERSION, own + 8))),
        Err(built(own + 8)),
        "a later build's table, larger: its structs may be too"
    );
    assert_eq!(
        check(std::ptr::from_ref(&table(API_VERSION, own - 1))),
        Err(built(own - 1)),
        "a smaller table lacks functions"
    );
    let speaks = |version: u32| {
        format!("The plugin speaks API version {version}; this program speaks {API_VERSION}")
    };
    for version in [0, API_VERSION + 1] {
        assert_eq!(
            check(std::ptr::from_ref(&table(version, own))),
            Err(speaks(version))
        );
    }
    // R7 review A3: the same version and size, other structs: refused by the fingerprint.
    let other = NcPlugin {
        abi: nc_plugin_api::ABI ^ 1,
        ..table(API_VERSION, own)
    };
    let refused = check(std::ptr::from_ref(&other)).unwrap_err();
    assert!(refused.contains("its structs differ"), "{refused}");
}

#[test]
fn a_plugins_text_is_copied_and_broken_utf8_replaced() {
    let bytes = b"a\xffb";
    let broken = NcStr {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    };
    assert_eq!(text(broken), "a\u{FFFD}b");
    let none = NcStr {
        ptr: bytes.as_ptr(),
        len: 0,
    };
    assert_eq!(text(none), "", "nothing read when the length is 0");
    let null = NcStr {
        ptr: std::ptr::null(),
        len: 3,
    };
    assert_eq!(text(null), "", "a null pointer is not read");
    assert_eq!(text(NcStr::new("Привет")), "Привет");
    // Review R46: a length no text has (past `isize::MAX` too) is never read.
    let huge = NcStr {
        ptr: bytes.as_ptr(),
        len: usize::MAX,
    };
    assert_eq!(text(huge), "");
    // SAFETY: a list too long is never read, whatever the pointer.
    let values: &[u8] = unsafe { super::calls::slice(bytes.as_ptr(), usize::MAX / 2) };
    assert!(values.is_empty());
}

const FOLDER: &str = r"C:\plugins\probe";

/// A plugin's context, started or not, and where its events come.
fn context(started: bool) -> (HostContext, Receiver<Event>) {
    let (sender, events) = channel();
    // Under the test program's file: a folder that cannot be made, so nothing is ever written.
    let log = PluginLog::new(&std::env::current_exe().unwrap().join("Logs"), "probe");
    let context = HostContext::new("probe", sender, Path::new(FOLDER), log);
    context.started.store(started, Ordering::Release);
    (context, events)
}

/// The host's message function as the plugin calls it.
fn ask(context: &HostContext, title: &str, body: &str) -> i32 {
    let raw = std::ptr::from_ref(context).cast_mut().cast();
    // SAFETY: the context and the strings live through the call, made on this one thread.
    unsafe {
        calls::message(
            raw,
            0,
            NcStr::new(title),
            NcStr::new(body),
            NcStr::new("OK"),
        )
    }
}

#[test]
fn a_context_names_its_plugin_and_folder() {
    let (context, _events) = context(false);
    assert_eq!(context.plugin, "probe");
    assert_eq!(context.folder, FOLDER);
    assert_eq!(context.data_folder, "", "none until the host names it");
    let named = context.with_data_folder(Path::new(r"C:\nc\settings\Plugins\probe"));
    assert_eq!(named.data_folder, r"C:\nc\settings\Plugins\probe");
    let context = named;
    assert!(
        !context.started.load(Ordering::Acquire),
        "not listened to yet"
    );
}

/// The program answering any window at once (OK, the first line, an empty text), so that a call which
/// should not reach it returns instead of waiting forever. True when something reached it (known once the
/// plugin's context, the sending side, is gone).
pub(super) fn answering_program(events: Receiver<Event>) -> std::thread::JoinHandle<bool> {
    std::thread::spawn(move || {
        let Ok(event) = events.recv() else {
            return false;
        };
        if let Event::Ask { request, reply, .. } = event {
            let answer = match request {
                Request::Message(_) => Answer::Button(0),
                Request::Menu(_) => Answer::Chosen {
                    item: Some(0),
                    key: None,
                },
                Request::Input(_) => Answer::Text(String::new()),
            };
            let _ = reply.send(answer);
        }
        true
    })
}

#[test]
fn a_message_before_the_start_is_esc_and_not_sent() {
    let (context, events) = context(false);
    let heard = answering_program(events);
    assert_eq!(ask(&context, "T", "x"), -1);
    drop(context);
    assert!(!heard.join().unwrap(), "nothing to show");
}

#[test]
fn a_message_with_nobody_listening_is_esc() {
    let (context, events) = context(true);
    drop(events);
    assert_eq!(ask(&context, "T", "x"), -1);
}

/// What the program heard of a message and how it answered: `Some` the user's answer, `None` the window
/// gone without one.
fn answered(answer: Option<bool>, body: &str) -> (i32, String, String, Vec<String>) {
    let (context, events) = context(true);
    let program = std::thread::spawn(move || {
        let event = events
            .recv_timeout(Duration::from_secs(10))
            .expect("the message");
        let Event::Ask {
            plugin,
            request: Request::Message(message),
            reply,
        } = event
        else {
            panic!("a message: {event:?}");
        };
        match answer {
            Some(true) => reply.send(Answer::Button(0)).unwrap(),
            Some(false) => reply.send(Answer::Esc).unwrap(),
            None => {}
        }
        (plugin, message.title, message.lines)
    });
    let result = ask(&context, "Title", body);
    let (plugin, title, lines) = program.join().unwrap();
    (result, plugin, title, lines)
}

#[test]
fn a_message_is_shown_and_waits_for_its_answer() {
    let (result, plugin, title, lines) = answered(Some(true), "one\r\ntwo\n\nfour\n");
    assert_eq!(result, 0, "OK");
    assert_eq!((plugin.as_str(), title.as_str()), ("probe", "Title"));
    assert_eq!(
        lines,
        ["one", "two", "", "four", ""],
        "CR LF or LF between lines; an empty line at the end stays (the SDK's [.., \"\"])"
    );
    let (_, _, _, lines) = answered(Some(true), "a\n");
    assert_eq!(
        lines,
        ["a", ""],
        "the lines [\"a\", \"\"] as the SDK joins them"
    );
    assert_eq!(answered(Some(false), "x").0, -1, "Esc");
    assert_eq!(answered(None, "x").0, -1, "no answer is Esc");
    let (_, _, _, lines) = answered(Some(true), "");
    assert!(lines.is_empty(), "no text, no lines");
}

#[cfg(windows)]
#[path = "own_ops_tests.rs"]
mod own_ops;

#[cfg(windows)]
#[path = "disk_files_tests.rs"]
mod disk_files;
