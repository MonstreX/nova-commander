//! The SDK's table on a plugin in this process (no library: the entry is called directly, so the test
//! program's panic hook stays its own).

use std::path::PathBuf;

use super::*;
use crate::api::{PLACE_COMMANDS, PLACE_DISKS};

pub(crate) struct Counter {
    runs: usize,
    opened: usize,
    folder: PathBuf,
}

impl Plugin for Counter {
    fn new(host: Host) -> Self {
        Self {
            runs: 0,
            opened: 0,
            folder: host.folder(),
        }
    }

    fn info(&self) -> Info {
        Info {
            title: "Counter".to_string(),
            description: self.folder.display().to_string(),
            ..Info::default()
        }
    }

    fn commands(&self) -> Vec<String> {
        vec![format!("Ran {}", self.runs)]
    }

    fn run_command(&mut self, index: usize) -> Result<Option<String>, String> {
        match index {
            0 => {
                self.runs += 1;
                Ok(None)
            }
            1 => Err("no luck".to_string()),
            _ => panic!("no such command"),
        }
    }

    fn disks(&self) -> Vec<String> {
        vec!["Disk".to_string(), format!("Opened {}", self.opened)]
    }

    fn open_disk(&mut self, index: usize) -> Result<Option<String>, String> {
        match index {
            0 => {
                self.opened += 1;
                Ok(None)
            }
            1 => Err("no disk".to_string()),
            _ => panic!("no such disk"),
        }
    }
}

unsafe extern "C" fn no_message(
    _context: *mut c_void,
    _flags: u32,
    _title: NcStr,
    _text: NcStr,
    _buttons: NcStr,
) -> i32 {
    -1
}

unsafe extern "C" fn no_menu(
    _context: *mut c_void,
    _menu: *const api::NcMenu,
    _key: *mut i32,
) -> i32 {
    -1
}

unsafe extern "C" fn no_input(
    _context: *mut c_void,
    _title: NcStr,
    _prompt: NcStr,
    _initial: NcStr,
    _history: NcStr,
    _flags: u32,
    _text: *mut NcStr,
) -> i32 {
    -1
}

unsafe extern "C" fn no_log(_context: *mut c_void, _text: NcStr) {}

pub(crate) unsafe extern "C" fn no_dialog(
    _context: *mut c_void,
    _dialog: *const api::NcDialog,
    _values: *mut api::NcDialogValue,
    _count: usize,
) -> i32 {
    -1
}

pub(crate) unsafe extern "C" fn no_dialog_change(
    _context: *mut c_void,
    _handle: *mut c_void,
    _change: *const api::NcDialogChange,
) -> i32 {
    -1
}

pub(crate) unsafe extern "C" fn no_setting(
    _context: *mut c_void,
    _key: NcStr,
    _value: *mut NcStr,
) -> i32 {
    -1
}

pub(crate) unsafe extern "C" fn no_set_setting(_context: *mut c_void, _key: NcStr, _value: NcStr) {}

pub(crate) unsafe extern "C" fn no_decode(
    _context: *mut c_void,
    _code_page: u32,
    _bytes: *const u8,
    _len: usize,
    _text: *mut NcStr,
) -> i32 {
    -1
}

const FOLDER: &str = r"C:\plugins\counter";

pub(crate) fn host(version: u32) -> NcHost {
    NcHost {
        api_version: version,
        size: size_of::<NcHost>(),
        abi: crate::api::ABI,
        context: std::ptr::null_mut(),
        folder: NcStr::new(FOLDER),
        message: no_message,
        menu: no_menu,
        input: no_input,
        log: no_log,
        dialog: no_dialog,
        dialog_change: no_dialog_change,
        setting: no_setting,
        set_setting: no_set_setting,
        decode: no_decode,
        progress: no_progress,
        cancelled: no_cancelled,
        panel_state: no_panel_state,
        panel_items: no_panel_items,
        panel_do: no_panel_do,
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
        data_folder: NcStr::new(r"C:\settings\Plugins\counter"),
        transfer: crate::program::tests::no_transfer,
        editor: crate::editor::tests::fake_editor,
    }
}

pub(crate) unsafe extern "C" fn no_panel_state(
    _context: *mut c_void,
    _which: u32,
    _state: *mut api::NcPanelState,
) -> i32 {
    -1
}

pub(crate) unsafe extern "C" fn no_panel_items(
    _context: *mut c_void,
    _which: u32,
    _flags: u32,
    _from: usize,
    _items: *mut api::NcPanelItem,
    _count: usize,
) -> isize {
    -1
}

pub(crate) unsafe extern "C" fn no_panel_do(
    _context: *mut c_void,
    _which: u32,
    _action: *const api::NcPanelAction,
    _answer: *mut NcStr,
) -> i32 {
    -1
}

pub(crate) unsafe extern "C" fn no_progress(
    _context: *mut c_void,
    _done: u64,
    _total: u64,
    _text: NcStr,
) {
}

pub(crate) unsafe extern "C" fn no_cancelled(_context: *mut c_void) -> i32 {
    0
}

pub(crate) fn text(raw: NcStr) -> String {
    // SAFETY: the plugin's strings live until its next call; this copies them at once.
    unsafe { raw.as_str() }.to_string()
}

#[test]
fn a_host_of_another_version_or_none_gets_no_plugin() {
    let other = host(API_VERSION + 1);
    let older = NcHost {
        size: size_of::<NcHost>() - 1,
        ..host(API_VERSION)
    };
    // SAFETY: null is allowed; the other tables live through the call.
    unsafe {
        assert!(entry::<Counter>(std::ptr::null()).is_null());
        assert!(entry::<Counter>(&raw const other).is_null());
        assert!(
            entry::<Counter>(&raw const older).is_null(),
            "a smaller table lacks functions"
        );
    }
    // R7 review A3: the same version and size, other structs.
    let structs = NcHost {
        abi: crate::api::ABI ^ 1,
        ..host(API_VERSION)
    };
    // SAFETY: as above.
    assert!(unsafe { entry::<Counter>(&raw const structs) }.is_null());
}

#[test]
fn the_table_calls_the_plugin_and_reports_its_failures() {
    let host = host(API_VERSION);
    // SAFETY: the host's table lives until the plugin is released at the end; each call below passes
    // the table's own state, one at a time, none after `release`.
    unsafe {
        let table = entry::<Counter>(&raw const host);
        let call = |table: *const NcPlugin| &*table;
        let state = call(table).state;
        assert_eq!(call(table).api_version, API_VERSION);
        let info = (call(table).info)(state);
        assert_eq!(text(info.title), "Counter");
        assert_eq!(text(info.description), FOLDER, "the plugin's folder");
        assert_eq!((call(table).item_count)(state, PLACE_COMMANDS), 1);
        assert_eq!(text((call(table).item)(state, PLACE_COMMANDS, 0)), "Ran 0");
        assert_eq!(
            (call(table).run_item)(state, PLACE_COMMANDS, 0, std::ptr::null_mut()),
            0
        );
        assert_eq!((call(table).item_count)(state, PLACE_COMMANDS), 1);
        assert_eq!(
            text((call(table).item)(state, PLACE_COMMANDS, 0)),
            "Ran 1",
            "told again"
        );
        assert_eq!(
            (call(table).run_item)(state, PLACE_COMMANDS, 1, std::ptr::null_mut()),
            1
        );
        assert_eq!(text((call(table).last_error)(state)), "no luck");
        assert_eq!(
            (call(table).run_item)(state, PLACE_COMMANDS, 5, std::ptr::null_mut()),
            1
        );
        assert!(text((call(table).last_error)(state)).starts_with("The plugin failed: "));
        assert_eq!(
            text((call(table).item)(state, PLACE_COMMANDS, 9)),
            "",
            "no such item"
        );
        (call(table).release)(state);
    }
}

#[test]
fn the_drive_menus_items_are_their_own() {
    let host = host(API_VERSION);
    // SAFETY: the host's table lives until the plugin is released at the end; each call below passes
    // the table's own state, one at a time, none after `release`.
    unsafe {
        let table = &*entry::<Counter>(&raw const host);
        let state = table.state;
        let label = |place, index| text((table.item)(state, place, index));
        assert_eq!((table.item_count)(state, PLACE_COMMANDS), 1);
        assert_eq!((table.item_count)(state, PLACE_DISKS), 2);
        assert_eq!(label(PLACE_DISKS, 1), "Opened 0");
        assert_eq!(label(PLACE_COMMANDS, 0), "Ran 0", "each place its own");
        assert_eq!(
            (table.run_item)(state, PLACE_DISKS, 0, std::ptr::null_mut()),
            0
        );
        assert_eq!((table.item_count)(state, PLACE_DISKS), 2);
        assert_eq!(label(PLACE_DISKS, 1), "Opened 1", "told again");
        assert_eq!(label(PLACE_COMMANDS, 0), "Ran 0", "no command ran");
        assert_eq!(
            (table.run_item)(state, PLACE_DISKS, 1, std::ptr::null_mut()),
            1
        );
        assert_eq!(text((table.last_error)(state)), "no disk");
        assert_eq!(
            (table.run_item)(state, PLACE_DISKS, 7, std::ptr::null_mut()),
            1
        );
        assert!(text((table.last_error)(state)).starts_with("The plugin failed: "));
        (table.release)(state);
    }
}

#[test]
fn a_place_this_build_does_not_know_has_no_items_and_runs_none() {
    let host = host(API_VERSION);
    // SAFETY: as above.
    unsafe {
        let table = &*entry::<Counter>(&raw const host);
        let state = table.state;
        // 2 became `PLACE_CONFIG` (5d-3a): the first place this build does not know is 3.
        assert_eq!((table.item_count)(state, 3), 0);
        assert_eq!((table.item_count)(state, u32::MAX), 0);
        assert_eq!(text((table.item)(state, 3, 0)), "");
        assert_eq!((table.run_item)(state, 3, 0, std::ptr::null_mut()), 1);
        assert_eq!(text((table.last_error)(state)), "The plugin has no place 3");
        assert_eq!((table.item_count)(state, PLACE_COMMANDS), 1);
        assert_eq!(
            text((table.item)(state, PLACE_COMMANDS, 0)),
            "Ran 0",
            "nothing ran"
        );
        (table.release)(state);
    }
}

#[test]
fn a_host_of_another_build_gets_no_plugin() {
    // Changed (review R3): a newer host's larger table was taken before, but the ABI's other structs carry
    // no size of their own, so the plugin would write past the host's answers; now the size must match.
    let newer = NcHost {
        size: size_of::<NcHost>() + 16,
        ..host(API_VERSION)
    };
    // SAFETY: the table lives through the call; `size` is all the entry reads past the fields it knows.
    let table = unsafe { entry::<Counter>(&raw const newer) };
    assert!(table.is_null(), "another build's host table: no plugin");
}

#[test]
fn the_table_tells_its_own_size() {
    let host = host(API_VERSION);
    // SAFETY: the host's table lives until the plugin is released; one call at a time, none after it.
    unsafe {
        let table = &*entry::<Counter>(&raw const host);
        assert_eq!(table.size, size_of::<NcPlugin>(), "the host checks it");
        assert_eq!(table.abi, crate::api::ABI, "and its structs' fingerprint");
        assert!(!table.state.is_null());
        assert_eq!(
            text((table.last_error)(table.state)),
            "",
            "nothing failed yet"
        );
        (table.release)(table.state);
    }
}

/// What the host's message function heard, and what it answers.
struct Heard {
    answer: i32,
    calls: std::sync::Mutex<Vec<(String, String)>>,
}

unsafe extern "C" fn heard_message(
    context: *mut c_void,
    flags: u32,
    title: NcStr,
    text: NcStr,
    buttons: NcStr,
) -> i32 {
    // SAFETY: the tests below make the context a `Heard` that outlives the plugin.
    let heard = unsafe { &*context.cast::<Heard>() };
    assert_eq!(flags, 0, "a plain message");
    // SAFETY: the plugin's strings live through this call.
    let (title, text, buttons) = unsafe { (title.as_str(), text.as_str(), buttons.as_str()) };
    assert_eq!(buttons, "OK", "Host::message has OK");
    heard
        .calls
        .lock()
        .unwrap()
        .push((title.to_string(), text.to_string()));
    heard.answer
}

/// Its only command asks the host's message window and fails on Esc.
struct Talker {
    host: Host,
}

impl Plugin for Talker {
    fn new(host: Host) -> Self {
        Self { host }
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        if self.host.message("Talk", &["one", "", "three"]) {
            Ok(None)
        } else {
            Err("Esc".to_string())
        }
    }
}

/// Talker's command run with a host that answers `answer`: its result and what the host heard.
fn talk(answer: i32) -> (i32, String, Vec<(String, String)>) {
    let heard = Heard {
        answer,
        calls: std::sync::Mutex::new(Vec::new()),
    };
    let host = NcHost {
        context: (&raw const heard).cast_mut().cast(),
        message: heard_message,
        ..host(API_VERSION)
    };
    // SAFETY: `heard` and `host` live until the plugin is released at the end; one call at a time.
    let (result, error) = unsafe {
        let table = &*entry::<Talker>(&raw const host);
        let result = (table.run_item)(table.state, PLACE_COMMANDS, 0, std::ptr::null_mut());
        let error = text((table.last_error)(table.state));
        (table.release)(table.state);
        (result, error)
    };
    (result, error, heard.calls.into_inner().unwrap())
}

#[test]
fn a_message_is_its_lines_joined_and_ok_is_zero() {
    let (result, _, calls) = talk(0);
    assert_eq!(result, 0, "OK: the command succeeded");
    assert_eq!(
        calls,
        [("Talk".to_string(), "one\n\nthree".to_string())],
        "one call, the lines joined by line feeds"
    );
    let (result, error, _) = talk(-1);
    assert_eq!((result, error.as_str()), (1, "Esc"), "Esc is false");
}

struct PanicsStarting;

impl Plugin for PanicsStarting {
    fn new(_host: Host) -> Self {
        panic!("no start")
    }

    fn info(&self) -> Info {
        Info::default()
    }
}

#[test]
fn a_plugin_panicking_as_it_starts_gives_no_table() {
    let host = host(API_VERSION);
    // SAFETY: the host's table lives through the call.
    assert!(unsafe { entry::<PanicsStarting>(&raw const host) }.is_null());
}

/// Tells and lists once, then panics telling and listing.
struct Fickle {
    calls: std::cell::Cell<usize>,
}

impl Fickle {
    fn once(&self) -> bool {
        let calls = self.calls.get();
        self.calls.set(calls + 1);
        calls < 2
    }
}

impl Plugin for Fickle {
    fn new(_host: Host) -> Self {
        Self {
            calls: std::cell::Cell::new(0),
        }
    }

    fn info(&self) -> Info {
        assert!(self.once(), "info panics");
        Info {
            title: "Fickle".to_string(),
            description: "d".to_string(),
            author: "a".to_string(),
            version: "v".to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        assert!(self.once(), "commands panic");
        vec!["Item".to_string()]
    }
}

#[test]
fn a_panic_telling_or_listing_tells_nothing_and_forgets_the_items() {
    let host = host(API_VERSION);
    // SAFETY: the host's table lives until the plugin is released at the end; one call at a time.
    unsafe {
        let table = &*entry::<Fickle>(&raw const host);
        let state = table.state;
        let info = (table.info)(state);
        let told = [info.title, info.description, info.author, info.version].map(text);
        assert_eq!(told, ["Fickle", "d", "a", "v"], "each field its own");
        assert_eq!((table.item_count)(state, PLACE_COMMANDS), 1);
        assert_eq!(text((table.item)(state, PLACE_COMMANDS, 0)), "Item");
        let info = (table.info)(state);
        let told = [info.title, info.description, info.author, info.version].map(text);
        assert_eq!(told, ["", "", "", ""], "a panic tells nothing");
        assert_eq!(
            (table.item_count)(state, PLACE_COMMANDS),
            0,
            "a panic lists nothing"
        );
        assert_eq!(
            text((table.item)(state, PLACE_COMMANDS, 0)),
            "",
            "the old items are gone"
        );
        (table.release)(state);
    }
}

static DROPPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Counts its drops in [`DROPPED`]; the one in a folder named `panic` panics dropping.
struct Dropped {
    panics: bool,
}

impl Plugin for Dropped {
    fn new(host: Host) -> Self {
        Self {
            panics: host.folder().ends_with("panic"),
        }
    }

    fn info(&self) -> Info {
        Info::default()
    }
}

impl Drop for Dropped {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(!self.panics, "the plugin panics dropping");
    }
}

#[test]
fn release_drops_the_plugin_and_keeps_its_panic() {
    for (folder, dropped) in [(r"C:\plugins\calm", 1), (r"C:\plugins\panic", 2)] {
        let host = NcHost {
            folder: NcStr::new(folder),
            ..host(API_VERSION)
        };
        // SAFETY: the host's table lives until the plugin is released; nothing is called after it.
        unsafe {
            let table = &*entry::<Dropped>(&raw const host);
            assert_eq!(
                DROPPED.load(std::sync::atomic::Ordering::SeqCst),
                dropped - 1
            );
            // A panic escaping an `extern "C"` call would end the test program here.
            (table.release)(table.state);
        }
        assert_eq!(
            DROPPED.load(std::sync::atomic::Ordering::SeqCst),
            dropped,
            "{folder}: dropped once"
        );
    }
}
