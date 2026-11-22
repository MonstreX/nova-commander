//! What a plugin asks of the program beyond its panels, through the table: the host's answers as the
//! plugin reads them; the plugin's synchro called back. The host's functions here answer as a fixed one
//! does (the other tests' tables take them too).

use std::sync::Mutex;

use super::*;
use crate::api::{self, API_VERSION, NcEntry};
use crate::tests::host;
use crate::{Info, entry};

/// What the stubs below heard, for the tests of this file (one at a time: `HEARD` is shared).
static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn heard(line: String) {
    HEARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(line);
}

pub(crate) unsafe extern "C" fn no_synchro(_context: *mut c_void, param: u64) {
    heard(format!("synchro {param}"));
}

pub(crate) unsafe extern "C" fn no_post_keys(_context: *mut c_void, keys: NcStr) -> i32 {
    // SAFETY: the SDK's own text, alive through the call.
    let keys = unsafe { keys.as_str() };
    if keys == "Nonsense" { -1 } else { 0 }
}

pub(crate) unsafe extern "C" fn no_screen(_context: *mut c_void, screen: *mut NcScreen) -> i32 {
    let answer = NcScreen {
        columns: 120,
        rows: 40,
        window: 7,
    };
    // SAFETY: the SDK hands a writable place.
    unsafe { screen.write(answer) };
    0
}

/// Takes the entries whose names start with `a`.
pub(crate) unsafe extern "C" fn no_filter(
    _context: *mut c_void,
    which: u32,
    entries: *const NcEntry,
    count: usize,
    results: *mut u8,
) -> i32 {
    if which != api::WHICH_PASSIVE {
        return -1;
    }
    for at in 0..count {
        // SAFETY: the SDK hands `count` entries and results, their texts alive through the call.
        unsafe {
            let name = (*entries.add(at)).name.as_str();
            results.add(at).write(u8::from(name.starts_with('a')));
        }
    }
    0
}

pub(crate) unsafe extern "C" fn no_clipboard(_context: *mut c_void, text: NcStr) -> i32 {
    // SAFETY: the SDK's own text, alive through the call.
    heard(format!("clipboard {}", unsafe { text.as_str() }));
    0
}

pub(crate) unsafe extern "C" fn no_show_help(_context: *mut c_void, _topic: NcStr) -> i32 {
    -1
}

/// Knows the plugin "tmppanel" only.
pub(crate) unsafe extern "C" fn no_plugins_control(
    _context: *mut c_void,
    action: u32,
    plugin: NcStr,
) -> i32 {
    // SAFETY: the SDK's own text, alive through the call.
    let plugin = unsafe { plugin.as_str() };
    heard(format!("plugins {action} {plugin}"));
    if plugin == "tmppanel" { 0 } else { -1 }
}

/// Asks before "Delete", not before "Exit", knows no other.
pub(crate) unsafe extern "C" fn no_confirmation(_context: *mut c_void, name: NcStr) -> i32 {
    // SAFETY: the SDK's own text, alive through the call.
    match unsafe { name.as_str() } {
        "Delete" => 1,
        "Exit" => 0,
        _ => -1,
    }
}

struct Waker(Vec<u64>);

impl Plugin for Waker {
    fn new(host: Host) -> Self {
        // A worker of its own asks, as a plugin's thread that saw a change does.
        let synchro = host.synchro();
        let _ = std::thread::spawn(move || synchro.ask(5)).join();
        Self(Vec::new())
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn synchro(&mut self, param: u64) {
        self.0.push(param);
    }
}

/// A confirmation by its name: asked, not asked, or a name the program does not know.
#[test]
fn a_confirmation_comes_back_by_its_name() {
    let table = host(API_VERSION);
    let fake = Host {
        table: &raw const table,
    };
    assert_eq!(fake.confirms("Delete"), Some(true));
    assert_eq!(fake.confirms("Exit"), Some(false));
    assert_eq!(fake.confirms("Unknown"), None);
}

/// Matches `*.zip` only; "[" is masks with errors.
pub(crate) unsafe extern "C" fn no_match_masks(
    _context: *mut c_void,
    masks: NcStr,
    name: NcStr,
) -> i32 {
    // SAFETY: the SDK's own texts, alive through the call.
    let (masks, name) = unsafe { (masks.as_str(), name.as_str()) };
    match masks {
        "[" => -1,
        _ => i32::from(masks == "*.zip" && name.ends_with(".zip")),
    }
}

pub(crate) unsafe extern "C" fn no_resume(_context: *mut c_void) {
    heard("resume".to_string());
}

/// The transfer heard as `transfer <instance> <folder> <names,> <disk> <flags>`; refused for a folder
/// "/refused".
pub(crate) unsafe extern "C" fn no_transfer(
    _context: *mut c_void,
    instance: NcStr,
    folder: NcStr,
    names: *const NcStr,
    count: usize,
    disk: NcStr,
    flags: u32,
) -> i32 {
    // SAFETY: the SDK's texts and array, alive through the call.
    let (instance, folder, disk, names) = unsafe {
        let names = std::slice::from_raw_parts(names, count);
        let names: Vec<&str> = names.iter().map(|name| name.as_str()).collect();
        (
            instance.as_str(),
            folder.as_str(),
            disk.as_str(),
            names.join(","),
        )
    };
    heard(format!(
        "transfer {instance} {folder} {names} {disk} {flags}"
    ));
    if folder == "/refused" { -1 } else { 0 }
}

#[test]
fn masks_are_matched_by_the_program() {
    let table = host(API_VERSION);
    let fake = Host {
        table: &raw const table,
    };
    assert_eq!(fake.match_masks("*.zip", "a.zip"), Some(true));
    assert_eq!(fake.match_masks("*.zip", "a.rar"), Some(false));
    assert_eq!(fake.match_masks("[", "a.zip"), None);
}

#[test]
fn the_program_answers_as_the_plugin_reads_it_and_a_synchro_comes_back() {
    let table = host(API_VERSION);
    let fake = Host {
        table: &raw const table,
    };
    assert!(fake.post_keys("Shift+F6 Ctrl+R"));
    assert!(!fake.post_keys("Nonsense"));
    let screen = Screen {
        columns: 120,
        rows: 40,
        window: 7,
    };
    assert_eq!(fake.screen(), Some(screen));
    let entries = [FileEntry::file("apple", 1), FileEntry::file("pear", 2)];
    assert_eq!(
        fake.filter(Which::Passive, &entries),
        Some(vec![true, false])
    );
    assert_eq!(fake.filter(Which::Active, &entries), None, "refused");
    assert!(fake.set_clipboard("text"));
    assert!(!fake.show_help("Contents"), "no help yet");
    assert!(fake.find_plugin("tmppanel") && !fake.find_plugin("none"));
    assert!(fake.load_plugin("tmppanel") && fake.unload_plugin("tmppanel"));
    fake.resume();
    // SAFETY: the host's table outlives the plugin; released once at the end.
    unsafe {
        let plugin = &*entry::<Waker>(&table);
        (plugin.synchro)(plugin.state, 9);
        let state = &*plugin.state.cast::<crate::State<Waker>>();
        assert_eq!(state.plugin.0, [9]);
        (plugin.release)(plugin.state);
    }
    let heard = HEARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(heard.contains(&"synchro 5".to_string()), "{heard:?}");
    assert!(heard.contains(&"clipboard text".to_string()), "{heard:?}");
    assert!(heard.contains(&"resume".to_string()), "{heard:?}");
    assert!(
        heard.contains(&"plugins 2 tmppanel".to_string()),
        "{heard:?}"
    );
}

/// API 9's copy handed to the program: its instance, folder, names, disk folder and flags as sent; a
/// refusal is false.
#[test]
fn a_copy_is_handed_to_the_program_as_asked() {
    let table = host(API_VERSION);
    let fake = Host {
        table: &raw const table,
    };
    let down = Transfer {
        instance: "sftp-1".to_string(),
        folder: "/home".to_string(),
        names: vec!["a".to_string(), "b".to_string()],
        disk: r"C:\w".to_string(),
        upload: false,
        moving: true,
    };
    assert!(fake.background_transfer(&down));
    let up = Transfer {
        folder: "/refused".to_string(),
        upload: true,
        moving: false,
        ..down
    };
    assert!(!fake.background_transfer(&up), "refused");
    let heard = HEARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let down = r"transfer sftp-1 /home a,b C:\w 2".to_string();
    let up = r"transfer sftp-1 /refused a,b C:\w 1".to_string();
    assert!(heard.contains(&down) && heard.contains(&up), "{heard:?}");
}

/// A plugin that hands its [`Synchro`] out, as one whose thread keeps it past `release` would.
struct Keeper;

static KEPT: Mutex<Option<Synchro>> = Mutex::new(None);

impl Plugin for Keeper {
    fn new(host: Host) -> Self {
        *KEPT.lock().unwrap() = Some(host.synchro());
        Self
    }

    fn info(&self) -> Info {
        Info::default()
    }
}

/// R7 review A7: a [`Synchro`] asks while its plugin is loaded, nothing once it is released (the
/// host's table goes with it); one of a host never loaded asks nothing.
#[test]
fn a_synchro_kept_past_release_asks_nothing() {
    let table = host(API_VERSION);
    let heard_of = |param: u64| {
        let heard = HEARD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        heard.contains(&format!("synchro {param}"))
    };
    // SAFETY: the host's table outlives the plugin; released once.
    let plugin = unsafe { &*entry::<Keeper>(&table) };
    let synchro = KEPT.lock().unwrap().take().unwrap();
    synchro.ask(70_001);
    assert!(heard_of(70_001), "loaded: asked");
    // SAFETY: as above.
    unsafe { (plugin.release)(plugin.state) };
    synchro.ask(70_002);
    assert!(!heard_of(70_002), "released: nothing");
    let never = Host {
        table: &raw const table,
    };
    never.synchro().ask(70_003);
    assert!(!heard_of(70_003), "never loaded: nothing");
}
