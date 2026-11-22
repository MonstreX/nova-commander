//! Shift+F2 / Shift+F3 on a disk's file at the plugin's boundary (`plugin_host::disk_files::offered`, API
//! 5's `OPEN_EXTRACT` / `OPEN_COMMANDS`) on a table without a library: the reason handed to `open_file`,
//! the root listed, its entries got with `GET_TOP_LEVEL` (and `GET_SILENT` when silent) or handed to
//! `process_host_file` with `GET_TOP_LEVEL`, the panel closed after every open, and each end as the
//! program hears it.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::path::Path;

use nc_plugin_api::{
    ERROR_CANCELLED, ERROR_NOT_FOUND, GET_SILENT, GET_TOP_LEVEL, HOST_FILE_CHANGED,
    HOST_FILE_UNSELECT, KEY_ACTIVE_FOLDER, KEY_OPEN, KEY_PASS, NcEntry, NcGet, NcKeyAnswer,
    OPEN_COMMANDS, OPEN_EXTRACT, PANEL_GIVES_FILES,
};

use super::*;
use crate::plugin_host::KeyAnswer;
use crate::plugin_host::disk_files::{DiskEnd, DiskUse, offered};

/// What the stub's `open_file` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opens {
    /// Its panel `7` (`KEY_OPEN`).
    Panel,
    /// The press passed on (`KEY_PASS`).
    Pass,
    /// Nothing opened (no flags: FAR's `PANEL_STOP`).
    Nothing,
    /// An answer that goes elsewhere, not a panel of its own.
    Elsewhere,
    /// A failure (its last error).
    Fails,
}

/// How the stub plugin behaves in the call at hand.
#[derive(Debug, Clone, Copy)]
struct Script {
    opens: Opens,
    list_fails: bool,
    gives_files: bool,
    /// `get`'s and `process_host_file`'s code.
    code: i32,
    /// `process_host_file`'s bits written.
    bits: u32,
}

const PLAIN: Script = Script {
    opens: Opens::Panel,
    list_fails: false,
    gives_files: true,
    code: 0,
    bits: HOST_FILE_CHANGED,
};

thread_local! {
    static SCRIPT: Cell<Script> = const { Cell::new(PLAIN) };
    /// What the stubs were handed on this thread, one line a call.
    static HANDED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn handed() -> Vec<String> {
    HANDED.with(|handed| handed.borrow_mut().drain(..).collect())
}

fn hand(line: String) {
    HANDED.with(|handed| handed.borrow_mut().push(line));
}

fn script() -> Script {
    SCRIPT.with(Cell::get)
}

static REASON: &str = "the plugin's reason";
static INSTANCE: &str = "7";
static ELSEWHERE: &str = r"C:\elsewhere";

unsafe extern "C" fn reason(_state: *mut c_void) -> NcStr {
    NcStr::new(REASON)
}

unsafe extern "C" fn opened(
    _state: *mut c_void,
    path: NcStr,
    alternative: u32,
    folder: NcStr,
    answer: *mut NcKeyAnswer,
) -> i32 {
    hand(format!(
        "open {} {alternative} [{}]",
        text(path),
        text(folder)
    ));
    let (flags, path) = match script().opens {
        Opens::Panel => (KEY_OPEN, INSTANCE),
        Opens::Pass => (KEY_PASS, ""),
        Opens::Nothing => (0, ""),
        Opens::Elsewhere => (KEY_ACTIVE_FOLDER, ELSEWHERE),
        Opens::Fails => return -7,
    };
    let told = NcKeyAnswer {
        flags,
        path: NcStr::new(path),
    };
    // SAFETY: the host's answer lives through the call; the text is static.
    unsafe { *answer = told };
    0
}

unsafe extern "C" fn listed(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    entries: *mut *const NcEntry,
    count: *mut usize,
) -> i32 {
    hand(format!("list {} {}", text(instance), text(path)));
    if script().list_fails {
        return ERROR_NOT_FOUND;
    }
    let named = |name: &'static str| NcEntry {
        name: NcStr::new(name),
        ..NcEntry::EMPTY
    };
    // Kept for the rest of the test program: the host reads them after the call.
    let list: &'static [NcEntry] =
        Box::leak(Box::new([named(".."), named("a.txt"), named("docs")]));
    // SAFETY: the host's answer places live through the call.
    unsafe {
        *entries = list.as_ptr();
        *count = list.len();
    }
    0
}

unsafe extern "C" fn looked(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
) -> nc_plugin_api::NcPanelInfo {
    hand(format!("look {} {}", text(instance), text(path)));
    nc_plugin_api::NcPanelInfo {
        title: NcStr::EMPTY,
        modes: std::ptr::null(),
        mode_count: 0,
        flags: if script().gives_files {
            PANEL_GIVES_FILES
        } else {
            0
        },
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

/// The names of a host's list.
/// # Safety
/// `names` is the host's list of `count` texts, valid through the call.
unsafe fn names_of(names: *const NcStr, count: usize) -> String {
    // SAFETY: as the caller says.
    let list: &[NcStr] = unsafe { super::super::calls::slice(names, count) };
    let names: Vec<String> = list.iter().map(|name| text(*name)).collect();
    names.join("|")
}

unsafe extern "C" fn got(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    get: *const NcGet,
) -> i32 {
    // SAFETY: the host's request lives through the call.
    let get = unsafe { &*get };
    // SAFETY: its names are the host's list of `count` texts.
    let names = unsafe { names_of(get.names, get.count) };
    hand(format!(
        "get {} {} {names} -> {} moving={} mode={} [{}]",
        text(instance),
        text(path),
        text(get.destination),
        get.moving,
        get.mode,
        text(get.folder)
    ));
    script().code
}

unsafe extern "C" fn processed(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
    done: *mut u32,
) -> i32 {
    // SAFETY: the host's list of `count` texts lives through the call.
    let names = unsafe { names_of(names, count) };
    hand(format!(
        "host {} {} {names} mode={mode}",
        text(instance),
        text(path)
    ));
    // SAFETY: the host's place lives through the call.
    unsafe { *done = script().bits };
    script().code
}

unsafe extern "C" fn closed(_state: *mut c_void, instance: NcStr) {
    hand(format!("close {}", text(instance)));
}

/// The stub plugin "started" without a library of its own (kernel32's stands in), behaving as `how`
/// says. Never dropped: it has no host's table to free, and its `release` is not to be called. Its
/// context is a real one (the panel's quiet calls mark it), kept for the rest of the test program.
fn loaded(how: Script) -> ManuallyDrop<Loaded> {
    SCRIPT.with(|script| script.set(how));
    handed();
    let mut plugin = table(API_VERSION, size_of::<NcPlugin>());
    plugin.open_file = opened;
    plugin.list = listed;
    plugin.panel_info = looked;
    plugin.get = got;
    plugin.process_host_file = processed;
    plugin.close_panel = closed;
    plugin.last_error = reason;
    let plugin: &'static NcPlugin = Box::leak(Box::new(plugin));
    let (context, _events) = context(true);
    let system = crate::platform::process::system_folder();
    let library = Library::open(&system.join("System32").join("kernel32.dll")).unwrap();
    ManuallyDrop::new(Loaded {
        table: plugin,
        host: std::ptr::null_mut(),
        context: Box::into_raw(Box::new(context)),
        _library: library,
    })
}

const FILE: &str = r"C:\files\a.zip";
const FOLDER: &str = r"C:\files";
const OUT: &str = r"D:\out";

fn extract(silent: bool) -> DiskUse {
    DiskUse::Extract {
        destination: OUT.to_string(),
        silent,
    }
}

/// `offered` for the file [`FILE`] with the program's folder [`FOLDER`], the press 42 on the right panel.
fn offer(plugin: &Loaded, purpose: &DiskUse) -> Event {
    let folder = Some(Path::new(FOLDER));
    offered(plugin, "probe", (Path::new(FILE), purpose, folder), (1, 42))
}

/// The end of a [`Event::DiskFile`] for the press 42 of `probe`.
fn ended(event: Event) -> (DiskEnd, Option<String>) {
    match event {
        Event::DiskFile {
            plugin,
            serial,
            end,
            error,
        } => {
            assert_eq!((plugin.as_str(), serial), ("probe", 42));
            (end, error)
        }
        other => panic!("a disk file's end: {other:?}"),
    }
}

fn open_line(how: u32, folder: &str) -> String {
    format!("open {FILE} {how} [{folder}]")
}

#[test]
fn shift_f2_opens_with_extract_gets_the_root_top_level_and_closes_the_panel() {
    let plugin = loaded(PLAIN);
    assert_eq!(
        ended(offer(&plugin, &extract(false))),
        (DiskEnd::Done, None)
    );
    assert_eq!(
        handed(),
        [
            open_line(OPEN_EXTRACT, FOLDER),
            "list 7 /".to_string(),
            "look 7 /".to_string(),
            format!("get 7 / a.txt|docs -> {OUT} moving=0 mode={GET_TOP_LEVEL} [{FOLDER}]"),
            "close 7".to_string(),
        ],
        "OFP_EXTRACT (2); the root's entries but `..`, OPM_TOPLEVEL alone, copied; closed after"
    );
}

#[test]
fn shift_f2_after_the_plugins_first_file_gets_silently() {
    let plugin = loaded(PLAIN);
    let event = offered(
        &plugin,
        "probe",
        (Path::new(FILE), &extract(true), None),
        (1, 42),
    );
    assert_eq!(ended(event), (DiskEnd::Done, None));
    let mode = GET_TOP_LEVEL | GET_SILENT;
    assert_eq!(
        handed(),
        [
            open_line(OPEN_EXTRACT, ""),
            "list 7 /".to_string(),
            "look 7 /".to_string(),
            format!("get 7 / a.txt|docs -> {OUT} moving=0 mode={mode} []"),
            "close 7".to_string(),
        ],
        "OPM_SILENT added; no program's folder: none handed"
    );
}

#[test]
fn shift_f2_stopped_or_failed_is_not_done_and_the_panel_is_closed_all_the_same() {
    let plugin = loaded(Script {
        code: ERROR_CANCELLED,
        ..PLAIN
    });
    assert_eq!(
        ended(offer(&plugin, &extract(false))),
        (DiskEnd::NotDone, None)
    );
    assert_eq!(handed().last().map(String::as_str), Some("close 7"));
    let plugin = loaded(Script { code: -7, ..PLAIN });
    assert_eq!(
        ended(offer(&plugin, &extract(false))),
        (DiskEnd::NotDone, Some(REASON.to_string())),
        "its last error"
    );
    assert_eq!(handed().last().map(String::as_str), Some("close 7"));
}

#[test]
fn shift_f2_on_a_panel_that_writes_no_files_out_gets_nothing_and_is_not_done() {
    let plugin = loaded(Script {
        gives_files: false,
        ..PLAIN
    });
    assert_eq!(
        ended(offer(&plugin, &extract(false))),
        (DiskEnd::NotDone, None)
    );
    assert_eq!(
        handed(),
        [
            open_line(OPEN_EXTRACT, FOLDER),
            "list 7 /".to_string(),
            "look 7 /".to_string(),
            "close 7".to_string(),
        ],
        "no get"
    );
}

#[test]
fn shift_f3_opens_with_commands_and_hands_the_root_top_level() {
    let plugin = loaded(PLAIN);
    assert_eq!(
        ended(offer(&plugin, &DiskUse::Commands)),
        (DiskEnd::Done, None)
    );
    assert_eq!(
        handed(),
        [
            open_line(OPEN_COMMANDS, FOLDER),
            "list 7 /".to_string(),
            format!("host 7 / a.txt|docs mode={GET_TOP_LEVEL}"),
            "close 7".to_string(),
        ],
        "OFP_COMMANDS (3); the root's entries but `..` with OPM_TOPLEVEL alone; no look asked"
    );
}

#[test]
fn shift_f3_is_done_only_when_the_plugin_changed_something() {
    for (bits, end) in [
        (HOST_FILE_CHANGED, DiskEnd::Done),
        (HOST_FILE_CHANGED | HOST_FILE_UNSELECT, DiskEnd::Done),
        (HOST_FILE_UNSELECT, DiskEnd::NotDone),
        (0, DiskEnd::NotDone),
    ] {
        let plugin = loaded(Script { bits, ..PLAIN });
        assert_eq!(
            ended(offer(&plugin, &DiskUse::Commands)),
            (end, None),
            "bits {bits}"
        );
        assert_eq!(handed().last().map(String::as_str), Some("close 7"));
    }
}

#[test]
fn shift_f3_stopped_or_failed_is_not_done_and_the_panel_is_closed_all_the_same() {
    let plugin = loaded(Script {
        code: ERROR_CANCELLED,
        ..PLAIN
    });
    assert_eq!(
        ended(offer(&plugin, &DiskUse::Commands)),
        (DiskEnd::NotDone, None),
        "the user's stop: the bits written are not taken"
    );
    assert_eq!(handed().last().map(String::as_str), Some("close 7"));
    let plugin = loaded(Script { code: -7, ..PLAIN });
    assert_eq!(
        ended(offer(&plugin, &DiskUse::Commands)),
        (DiskEnd::NotDone, Some(REASON.to_string()))
    );
    assert_eq!(handed().last().map(String::as_str), Some("close 7"));
}

#[test]
fn a_root_that_cannot_be_listed_is_not_opened_and_its_panel_closed() {
    for purpose in [extract(false), DiskUse::Commands] {
        let plugin = loaded(Script {
            list_fails: true,
            ..PLAIN
        });
        assert_eq!(
            ended(offer(&plugin, &purpose)),
            (DiskEnd::NotOpened, None),
            "FAR's GetFindData failing: the next file's turn ({purpose:?})"
        );
        let calls = handed();
        assert_eq!(calls[1..], ["list 7 /", "close 7"], "{purpose:?}");
    }
}

#[test]
fn a_file_the_plugin_passes_goes_back_as_the_press_passed_on_with_no_panel_closed() {
    for purpose in [extract(false), DiskUse::Commands] {
        let plugin = loaded(Script {
            opens: Opens::Pass,
            ..PLAIN
        });
        match offer(&plugin, &purpose) {
            Event::Key {
                plugin,
                side,
                serial,
                answer,
            } => {
                assert_eq!((plugin.as_str(), side, serial), ("probe", 1, 42));
                assert_eq!(
                    answer,
                    KeyAnswer {
                        pass: true,
                        ..KeyAnswer::default()
                    }
                );
            }
            other => panic!("the press passed on: {other:?}"),
        }
        assert_eq!(handed().len(), 1, "opening asked alone ({purpose:?})");
    }
}

#[test]
fn an_answer_that_opens_no_panel_is_not_opened_and_nothing_closed() {
    for opens in [Opens::Nothing, Opens::Elsewhere] {
        let plugin = loaded(Script { opens, ..PLAIN });
        assert_eq!(
            ended(offer(&plugin, &DiskUse::Commands)),
            (DiskEnd::NotOpened, None),
            "{opens:?}"
        );
        assert_eq!(handed(), [open_line(OPEN_COMMANDS, FOLDER)], "{opens:?}");
    }
}

#[test]
fn opening_that_fails_is_not_opened_with_the_reason() {
    let plugin = loaded(Script {
        opens: Opens::Fails,
        ..PLAIN
    });
    assert_eq!(
        ended(offer(&plugin, &extract(false))),
        (DiskEnd::NotOpened, Some(REASON.to_string()))
    );
    assert_eq!(
        handed(),
        [open_line(OPEN_EXTRACT, FOLDER)],
        "nothing closed"
    );
}
