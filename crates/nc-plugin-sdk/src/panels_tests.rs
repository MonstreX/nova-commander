//! What the SDK's panel calls hand the host's table and how they read its answers (a host faked in this
//! process).

use std::ffi::c_void;
use std::sync::Mutex;

use super::*;
use crate::api::{NcHost, NcPanelAction};
use crate::tests::host;

/// What the fake host answers, and what it heard.
#[derive(Default)]
struct Fake {
    names: Vec<String>,
    heard: Mutex<Vec<String>>,
}

/// # Safety
/// `context` is a live `Fake`.
unsafe fn fake<'a>(context: *mut c_void) -> &'a Fake {
    // SAFETY: as the caller promises.
    unsafe { &*context.cast::<Fake>() }
}

const FOLDER: &str = r"C:\work";

unsafe extern "C" fn state(context: *mut c_void, which: u32, out: *mut NcPanelState) -> i32 {
    // SAFETY: the test's own fake.
    let fake = unsafe { fake(context) };
    fake.heard.lock().unwrap().push(format!("state {which}"));
    let mut state = NcPanelState::EMPTY;
    state.flags = api::STATE_LEFT | api::STATE_CALL_TARGET;
    state.kind = api::KIND_INFO;
    state.folder = NcStr::new(FOLDER);
    state.items = fake.names.len();
    state.current = 1;
    // SAFETY: the SDK hands a writable state.
    unsafe { out.write(state) };
    0
}

unsafe extern "C" fn items(
    context: *mut c_void,
    which: u32,
    flags: u32,
    from: usize,
    out: *mut NcPanelItem,
    count: usize,
) -> isize {
    // SAFETY: the test's own fake.
    let fake = unsafe { fake(context) };
    fake.heard
        .lock()
        .unwrap()
        .push(format!("items {which} {flags} {from} {count}"));
    for (at, name) in fake.names.iter().enumerate().skip(from).take(count) {
        let mut item = NcPanelItem::EMPTY;
        item.name = NcStr::new(name);
        item.flags = if at == 0 {
            api::ITEM_PARENT
        } else {
            api::ITEM_SELECTED
        };
        item.modified = 2000;
        // SAFETY: room for `count` items.
        unsafe { out.add(at - from).write(item) };
    }
    isize::try_from(fake.names.len()).unwrap()
}

unsafe extern "C" fn act(
    context: *mut c_void,
    which: u32,
    action: *const NcPanelAction,
    answer: *mut NcStr,
) -> i32 {
    // SAFETY: the test's own fake; the SDK's action lives through the call.
    let (fake, action) = unsafe { (fake(context), &*action) };
    // SAFETY: the SDK's names live through the call.
    let names: Vec<String> = (0..action.name_count)
        .map(|at| unsafe { (*action.names.add(at)).as_str().to_string() })
        .collect();
    // SAFETY: as above.
    let text = unsafe { action.text.as_str() };
    fake.heard.lock().unwrap().push(format!(
        "do {which} {} {} {text} {} {} {names:?}",
        action.action, action.flags, action.number, action.number2
    ));
    if action.action == api::DO_GET_COMMAND_LINE {
        // SAFETY: the SDK hands a writable text.
        unsafe { answer.write(NcStr::new("dir /b")) };
    }
    0
}

fn faked(fake: &Fake) -> NcHost {
    NcHost {
        context: std::ptr::from_ref(fake).cast_mut().cast(),
        panel_state: state,
        panel_items: items,
        panel_do: act,
        ..host(api::API_VERSION)
    }
}

#[test]
fn a_panels_state_and_items_come_as_the_host_answers() {
    let fake = Fake {
        names: vec!["..".to_string(), "a.txt".to_string()],
        ..Fake::default()
    };
    let table = faked(&fake);
    let host = Host {
        table: &raw const table,
    };
    let state = host.panel(Which::Passive).unwrap();
    assert!(state.left && state.call_target && !state.active);
    assert_eq!(state.kind, PanelKind::Info);
    assert_eq!(
        (state.folder.as_str(), state.items, state.current),
        (FOLDER, 2, 1)
    );

    let items = host.selected_items(Which::Right);
    assert_eq!(items.len(), 2);
    assert!(items[0].parent && !items[0].selected);
    assert!(items[1].selected && items[1].name == "a.txt");
    let modified = std::time::UNIX_EPOCH + std::time::Duration::from_millis(2000);
    assert_eq!(items[1].modified, Some(modified));
    let heard = fake.heard.lock().unwrap().clone();
    let selected = api::ITEMS_SELECTED;
    assert_eq!(
        heard,
        [
            "state 1".to_string(),
            format!("items 3 {selected} 0 0"),
            format!("items 3 {selected} 0 2")
        ],
        "asked how many, then read them"
    );
}

#[test]
fn every_change_is_handed_with_its_arguments() {
    let fake = Fake::default();
    let table = faked(&fake);
    let host = Host {
        table: &raw const table,
    };
    assert!(host.set_folder(Which::Passive, FOLDER, Some("a.txt")));
    assert!(host.set_cursor(Which::Active, 3, None));
    assert!(host.set_cursor_on(Which::Left, "b"));
    assert!(host.reread(Which::Active, true));
    assert!(host.select(Which::Active, &["x".to_string()], false));
    assert!(host.set_sort(Which::Active, api::SORT_SIZE, true));
    assert!(host.close_panel(Which::Active, None));
    assert_eq!(host.command_line(), "dir /b");
    assert!(host.set_command_line("ver"));
    assert!(host.insert_command_line("x"));
    assert!(host.view(std::path::Path::new(r"C:\t.txt"), true));
    assert!(host.edit(std::path::Path::new(r"C:\t.txt"), 3, 4, false));
    let heard = fake.heard.lock().unwrap().clone();
    let max = usize::MAX;
    let expected = [
        format!("do 1 {} 0 {FOLDER} 0 0 [\"a.txt\"]", api::DO_SET_FOLDER),
        format!("do 0 {} 0  3 {max} []", api::DO_SET_CURSOR),
        format!("do 2 {} 0 b 0 {max} []", api::DO_SET_CURSOR),
        format!("do 0 {} {}  0 0 []", api::DO_REREAD, api::DO_KEEP_SELECTION),
        format!("do 0 {} {}  0 0 [\"x\"]", api::DO_SELECT, api::DO_OFF),
        format!(
            "do 0 {} {}  {} 0 []",
            api::DO_SET_SORT,
            api::DO_REVERSE,
            api::SORT_SIZE
        ),
        format!("do 0 {} 0  0 0 []", api::DO_CLOSE),
        format!("do 0 {} 0  0 0 []", api::DO_GET_COMMAND_LINE),
        format!("do 0 {} 0 ver 0 0 []", api::DO_SET_COMMAND_LINE),
        format!("do 0 {} 0 x 0 0 []", api::DO_INSERT_COMMAND_LINE),
        format!(
            "do 0 {} {} C:\\t.txt 0 0 []",
            api::DO_VIEW,
            api::DO_DELETE_ON_CLOSE
        ),
        format!("do 0 {} 0 C:\\t.txt 3 4 []", api::DO_EDIT),
    ];
    assert_eq!(heard, expected);
}

#[test]
fn a_host_that_does_not_listen_answers_nothing() {
    let table = host(api::API_VERSION);
    let host = Host {
        table: &raw const table,
    };
    assert_eq!(host.panel(Which::Active), None);
    assert!(host.panel_items(Which::Active).is_empty());
    assert!(!host.set_folder(Which::Active, FOLDER, None));
    assert_eq!(host.command_line(), "");
}

/// R7 review A8: a path that is no exact text (an unpaired surrogate) is not viewed or edited - it was
/// sent with U+FFFD, a sibling of that name opened; a real path or a key's place of it is none.
#[cfg(windows)]
#[test]
fn a_path_that_is_no_exact_text_is_not_sent() {
    use std::os::windows::ffi::OsStringExt;
    let lone = std::path::PathBuf::from(std::ffi::OsString::from_wide(&[0x43, 0x3A, 0x5C, 0xD800]));
    let fake = Fake::default();
    let table = faked(&fake);
    let host = Host {
        table: &raw const table,
    };
    assert!(!host.view(&lone, false));
    assert!(!host.edit(&lone, 1, 1, false));
    assert!(fake.heard.lock().unwrap().is_empty(), "nothing asked");
    let mut kept = crate::Kept::default();
    let entry = crate::FileEntry {
        real_path: Some(lone.clone()),
        ..crate::FileEntry::file("a", 1)
    };
    let real_path = crate::panel::raw_entry(entry, &mut kept).real_path;
    assert_eq!(real_path.len, 0, "no real path");
    let answer = crate::KeyAnswer::go(crate::KeyGo::ActiveFile(lone));
    let raw = answer.raw(&mut kept);
    assert_eq!((raw.flags & api::KEY_ACTIVE_FILE, raw.path.len), (0, 0));
}
