//! F8's and F7's calls of a plugin that deletes and makes folders itself (API 5's `delete_files` and
//! `make_folder`) on a table without a library: what is handed over, and the answer read as every text of a
//! plugin is (lossy, a length past `MOST_TEXT` not read).

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use nc_plugin_api::{ERROR_CANCELLED, ERROR_UNSUPPORTED};

use super::*;

thread_local! {
    /// What the stubs were handed on this thread, one line a call.
    static HANDED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn handed() -> Vec<String> {
    HANDED.with(|handed| handed.borrow_mut().drain(..).collect())
}

fn hand(line: String) {
    HANDED.with(|handed| handed.borrow_mut().push(line));
}

static BROKEN: &[u8] = b"a\xffb";
static REASON: &str = "the plugin's reason";

unsafe extern "C" fn reason(_state: *mut c_void) -> NcStr {
    NcStr::new(REASON)
}

/// A folder made, its name answered as `answer` gives it; the call's arguments kept.
/// # Safety
/// `name` is the host's answer, valid through the call.
unsafe fn made_with(
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
    answer: Option<NcStr>,
) {
    // SAFETY: the host's answer lives through the call; it holds the name asked for.
    let asked = text(unsafe { *name });
    hand(format!(
        "{} {} {mode} [{asked}]",
        text(instance),
        text(path)
    ));
    if let Some(answer) = answer {
        // SAFETY: as above; the bytes are static.
        unsafe { *name = answer };
    }
}

unsafe extern "C" fn made_broken(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
) -> i32 {
    let answer = NcStr {
        ptr: BROKEN.as_ptr(),
        len: BROKEN.len(),
    };
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { made_with(instance, path, mode, name, Some(answer)) };
    0
}

unsafe extern "C" fn made_too_long(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
) -> i32 {
    // A length no text has: never read past the three bytes there are.
    let answer = NcStr {
        ptr: BROKEN.as_ptr(),
        len: MOST_TEXT + 1,
    };
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { made_with(instance, path, mode, name, Some(answer)) };
    0
}

unsafe extern "C" fn made_null(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
) -> i32 {
    let answer = NcStr {
        ptr: std::ptr::null(),
        len: 5,
    };
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { made_with(instance, path, mode, name, Some(answer)) };
    0
}

unsafe extern "C" fn made_untold(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
) -> i32 {
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { made_with(instance, path, mode, name, None) };
    0
}

unsafe extern "C" fn made_cancelled(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
) -> i32 {
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { made_with(instance, path, mode, name, Some(NcStr::new("never"))) };
    ERROR_CANCELLED
}

unsafe extern "C" fn made_failed(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
) -> i32 {
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { made_with(instance, path, mode, name, Some(NcStr::new("never"))) };
    ERROR_UNSUPPORTED
}

/// The names handed to a delete kept, its answer `code`.
/// # Safety
/// `names` is the host's list and its count, valid through the call.
unsafe fn deleted_with(instance: NcStr, path: NcStr, names: (*const NcStr, usize), mode: u32) {
    // SAFETY: the host's list of `count` texts lives through the call.
    let list: &[NcStr] = unsafe { super::super::calls::slice(names.0, names.1) };
    let names: Vec<String> = list.iter().map(|name| text(*name)).collect();
    hand(format!(
        "{} {} {mode} {}",
        text(instance),
        text(path),
        names.join("|")
    ));
}

unsafe extern "C" fn deleted(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
) -> i32 {
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { deleted_with(instance, path, (names, count), mode) };
    0
}

unsafe extern "C" fn delete_cancelled(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
) -> i32 {
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { deleted_with(instance, path, (names, count), mode) };
    ERROR_CANCELLED
}

unsafe extern "C" fn delete_failed(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
) -> i32 {
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { deleted_with(instance, path, (names, count), mode) };
    -7
}

type MakeFolder = unsafe extern "C" fn(*mut c_void, NcStr, NcStr, u32, *mut NcStr) -> i32;
type DeleteFiles = unsafe extern "C" fn(*mut c_void, NcStr, NcStr, *const NcStr, usize, u32) -> i32;

/// A plugin of this table "started" without a library of its own (kernel32's stands in). Never dropped:
/// it has no host's table or context to free, and its `release` is not to be called.
fn loaded(make_folder: MakeFolder, delete_files: DeleteFiles) -> ManuallyDrop<Loaded> {
    let mut plugin = table(API_VERSION, size_of::<NcPlugin>());
    plugin.make_folder = make_folder;
    plugin.delete_files = delete_files;
    plugin.last_error = reason;
    let plugin: &'static NcPlugin = Box::leak(Box::new(plugin));
    let system = crate::platform::process::system_folder();
    let library = Library::open(&system.join("System32").join("kernel32.dll")).unwrap();
    ManuallyDrop::new(Loaded {
        table: plugin,
        host: std::ptr::null_mut(),
        context: std::ptr::null_mut(),
        _library: library,
    })
}

#[test]
fn a_folder_made_is_asked_with_no_name_and_its_answer_read_lossily() {
    let plugin = loaded(made_broken, deleted);
    assert_eq!(
        plugin.make_folder("arc", "/docs", ""),
        Ok(Some("a\u{FFFD}b".to_string())),
        "broken UTF-8 replaced"
    );
    assert_eq!(handed(), ["arc /docs 0 []"], "OPM 0, no name asked");
    let plugin = loaded(made_untold, deleted);
    assert_eq!(
        plugin.make_folder("arc", "/", "asked"),
        Ok(Some("asked".to_string())),
        "an answer not written is the name asked"
    );
    assert_eq!(handed(), ["arc / 0 [asked]"]);
}

#[test]
fn a_folders_name_past_the_longest_text_or_without_a_pointer_is_none() {
    let plugin = loaded(made_too_long, deleted);
    assert_eq!(plugin.make_folder("arc", "/", ""), Ok(Some(String::new())));
    let plugin = loaded(made_null, deleted);
    assert_eq!(plugin.make_folder("arc", "/", ""), Ok(Some(String::new())));
    assert_eq!(handed().len(), 2);
}

#[test]
fn a_folder_not_made_is_none_when_stopped_and_the_reason_when_failed() {
    let plugin = loaded(made_cancelled, deleted);
    assert_eq!(plugin.make_folder("arc", "/", ""), Ok(None), "stopped");
    let plugin = loaded(made_failed, deleted);
    assert_eq!(
        plugin.make_folder("arc", "/", ""),
        Err(REASON.to_string()),
        "its last error"
    );
}

type OpenNew = unsafe extern "C" fn(*mut c_void, *mut NcStr) -> i32;

/// [`loaded`] with `open_new` for Shift+F1's new panel.
fn loaded_new(open_new: OpenNew) -> ManuallyDrop<Loaded> {
    let mut plugin = table(API_VERSION, size_of::<NcPlugin>());
    plugin.open_new = open_new;
    plugin.last_error = reason;
    let plugin: &'static NcPlugin = Box::leak(Box::new(plugin));
    let system = crate::platform::process::system_folder();
    let library = Library::open(&system.join("System32").join("kernel32.dll")).unwrap();
    ManuallyDrop::new(Loaded {
        table: plugin,
        host: std::ptr::null_mut(),
        context: std::ptr::null_mut(),
        _library: library,
    })
}

/// The host's place for the instance looked at (empty, as the host hands it), `answer` written there.
/// # Safety
/// `instance` is the host's place, valid through the call.
unsafe fn opened_with(instance: *mut NcStr, answer: Option<NcStr>) {
    // SAFETY: the host's place lives through the call.
    let given = unsafe { *instance };
    hand(format!("open_new [{}] {}", text(given), given.len));
    if let Some(answer) = answer {
        // SAFETY: as above; the bytes are static.
        unsafe { *instance = answer };
    }
}

unsafe extern "C" fn opened_one(_state: *mut c_void, instance: *mut NcStr) -> i32 {
    // SAFETY: the host's place of this call.
    unsafe { opened_with(instance, Some(NcStr::new("7"))) };
    0
}

unsafe extern "C" fn opened_broken(_state: *mut c_void, instance: *mut NcStr) -> i32 {
    let answer = NcStr {
        ptr: BROKEN.as_ptr(),
        len: BROKEN.len(),
    };
    // SAFETY: the host's place of this call.
    unsafe { opened_with(instance, Some(answer)) };
    0
}

unsafe extern "C" fn opened_too_long(_state: *mut c_void, instance: *mut NcStr) -> i32 {
    // A length no text has: never read past the three bytes there are.
    let answer = NcStr {
        ptr: BROKEN.as_ptr(),
        len: MOST_TEXT + 1,
    };
    // SAFETY: the host's place of this call.
    unsafe { opened_with(instance, Some(answer)) };
    0
}

unsafe extern "C" fn opened_null(_state: *mut c_void, instance: *mut NcStr) -> i32 {
    let answer = NcStr {
        ptr: std::ptr::null(),
        len: 5,
    };
    // SAFETY: the host's place of this call.
    unsafe { opened_with(instance, Some(answer)) };
    0
}

unsafe extern "C" fn opened_untold(_state: *mut c_void, instance: *mut NcStr) -> i32 {
    // SAFETY: the host's place of this call.
    unsafe { opened_with(instance, None) };
    0
}

unsafe extern "C" fn open_failed(_state: *mut c_void, instance: *mut NcStr) -> i32 {
    // SAFETY: the host's place of this call.
    unsafe { opened_with(instance, Some(NcStr::new("never"))) };
    ERROR_UNSUPPORTED
}

#[test]
fn a_new_panels_instance_is_read_lossily() {
    let plugin = loaded_new(opened_one);
    assert_eq!(plugin.open_new(), Ok(Some("7".to_string())));
    assert_eq!(handed(), ["open_new [] 0"], "an empty place handed over");
    let plugin = loaded_new(opened_broken);
    assert_eq!(
        plugin.open_new(),
        Ok(Some("a\u{FFFD}b".to_string())),
        "broken UTF-8 replaced"
    );
    assert_eq!(handed().len(), 1);
}

#[test]
fn a_new_panel_not_told_past_the_longest_text_or_without_a_pointer_is_none() {
    for open_new in [opened_untold as OpenNew, opened_too_long, opened_null] {
        let plugin = loaded_new(open_new);
        assert_eq!(plugin.open_new(), Ok(None), "none made: the next plugin's");
    }
    assert_eq!(handed().len(), 3, "each asked once");
}

#[test]
fn a_new_panel_that_failed_is_the_plugins_reason() {
    let plugin = loaded_new(open_failed);
    assert_eq!(plugin.open_new(), Err(REASON.to_string()), "its last error");
    assert_eq!(handed().len(), 1);
}

#[test]
fn entries_deleted_are_handed_in_order_with_the_mode_and_answered_by_the_code() {
    let names = ["b.txt".to_string(), "Привет".to_string(), "a b".to_string()];
    let plugin = loaded(made_untold, deleted);
    assert_eq!(plugin.delete_files("arc", "/docs", &names, 0), Ok(true));
    assert_eq!(handed(), ["arc /docs 0 b.txt|Привет|a b"]);
    assert_eq!(
        plugin.delete_files("arc", "/", &names[..1], GET_SILENT),
        Ok(true)
    );
    assert_eq!(handed(), [format!("arc / {GET_SILENT} b.txt")]);
    let plugin = loaded(made_untold, delete_cancelled);
    assert_eq!(plugin.delete_files("arc", "/", &names, 0), Ok(false));
    let plugin = loaded(made_untold, delete_failed);
    assert_eq!(
        plugin.delete_files("arc", "/", &[], 0),
        Err(REASON.to_string())
    );
    assert_eq!(handed().last().map(String::as_str), Some("arc / 0 "));
}

type HostFile =
    unsafe extern "C" fn(*mut c_void, NcStr, NcStr, *const NcStr, usize, u32, *mut u32) -> i32;

/// [`loaded`] with `process_host_file` for Shift+F3's commands on a panel's file.
fn loaded_host_file(process_host_file: HostFile) -> ManuallyDrop<Loaded> {
    let mut plugin = table(API_VERSION, size_of::<NcPlugin>());
    plugin.process_host_file = process_host_file;
    plugin.last_error = reason;
    let plugin: &'static NcPlugin = Box::leak(Box::new(plugin));
    let system = crate::platform::process::system_folder();
    let library = Library::open(&system.join("System32").join("kernel32.dll")).unwrap();
    ManuallyDrop::new(Loaded {
        table: plugin,
        host: std::ptr::null_mut(),
        context: std::ptr::null_mut(),
        _library: library,
    })
}

/// The call's arguments kept with the answer's place as the host hands it, `bits` written there.
/// # Safety
/// `names` is the host's list and its count, `done` the host's place, valid through the call.
unsafe fn processed_with(
    instance: NcStr,
    path: NcStr,
    names: (*const NcStr, usize),
    mode: u32,
    done: *mut u32,
    bits: Option<u32>,
) {
    // SAFETY: the host's list of `count` texts lives through the call.
    let list: &[NcStr] = unsafe { super::super::calls::slice(names.0, names.1) };
    let names: Vec<String> = list.iter().map(|name| text(*name)).collect();
    // SAFETY: the host's place lives through the call.
    let given = unsafe { *done };
    hand(format!(
        "{} {} {mode} {} [{given}]",
        text(instance),
        text(path),
        names.join("|")
    ));
    if let Some(bits) = bits {
        // SAFETY: as above.
        unsafe { *done = bits };
    }
}

unsafe extern "C" fn processed_both(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
    done: *mut u32,
) -> i32 {
    let bits = Some(HOST_FILE_CHANGED | HOST_FILE_UNSELECT);
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { processed_with(instance, path, (names, count), mode, done, bits) };
    0
}

unsafe extern "C" fn processed_unselect(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
    done: *mut u32,
) -> i32 {
    let bits = Some(HOST_FILE_UNSELECT);
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { processed_with(instance, path, (names, count), mode, done, bits) };
    0
}

unsafe extern "C" fn processed_untold(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
    done: *mut u32,
) -> i32 {
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { processed_with(instance, path, (names, count), mode, done, None) };
    0
}

unsafe extern "C" fn process_cancelled(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
    done: *mut u32,
) -> i32 {
    let bits = Some(HOST_FILE_CHANGED | HOST_FILE_UNSELECT);
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { processed_with(instance, path, (names, count), mode, done, bits) };
    ERROR_CANCELLED
}

unsafe extern "C" fn process_failed(
    _state: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
    done: *mut u32,
) -> i32 {
    let bits = Some(HOST_FILE_CHANGED);
    // SAFETY: the host's arguments of this call, valid through it.
    unsafe { processed_with(instance, path, (names, count), mode, done, bits) };
    -7
}

#[test]
fn a_host_files_entries_are_handed_in_order_with_the_mode_and_its_bits_read() {
    let names = ["b.txt".to_string(), "Привет".to_string(), "a b".to_string()];
    let plugin = loaded_host_file(processed_both);
    assert_eq!(
        plugin.process_host_file("arc", "/docs", &names, 0),
        Ok(HOST_FILE_CHANGED | HOST_FILE_UNSELECT),
        "both bits as the plugin wrote them"
    );
    assert_eq!(
        handed(),
        ["arc /docs 0 b.txt|Привет|a b [0]"],
        "the names in order, mode 0, the answer's place handed as 0"
    );
    let plugin = loaded_host_file(processed_unselect);
    assert_eq!(
        plugin.process_host_file("7", "/", &names[1..2], GET_SILENT),
        Ok(HOST_FILE_UNSELECT),
        "one bit alone"
    );
    assert_eq!(handed(), [format!("7 / {GET_SILENT} Привет [0]")]);
}

#[test]
fn a_host_files_answer_not_written_is_nothing_done() {
    let plugin = loaded_host_file(processed_untold);
    assert_eq!(plugin.process_host_file("arc", "/", &[], 0), Ok(0));
    assert_eq!(handed(), ["arc / 0  [0]"], "no names handed");
}

#[test]
fn a_host_files_commands_stopped_are_nothing_done_and_failed_the_reason() {
    let names = ["a".to_string()];
    let plugin = loaded_host_file(process_cancelled);
    assert_eq!(
        plugin.process_host_file("arc", "/", &names, 0),
        Ok(0),
        "stopped by the user: the bits it wrote are not taken"
    );
    let plugin = loaded_host_file(process_failed);
    assert_eq!(
        plugin.process_host_file("arc", "/", &names, 0),
        Err(REASON.to_string()),
        "its last error, not the bits"
    );
    assert_eq!(handed(), ["arc / 0 a [0]", "arc / 0 a [0]"]);
}
