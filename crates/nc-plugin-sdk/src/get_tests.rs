//! Files written out by the plugin through the table: the request as the host made it, the modes by their
//! bits, a plugin that writes none refusing, a failure and a stop by their codes.
use std::path::Path;

use super::*;
use crate::api::{
    API_VERSION, ERROR_CANCELLED, ERROR_UNSUPPORTED, GET_EDIT, GET_SILENT, GET_VIEW, NcPlugin,
};
use crate::panel::{Error, ErrorKind};
use crate::tests::{host, text};
use crate::{Host, Info, entry};

/// Keeps what it was asked; `stop` answers as the user stopped it, `fail` as it failed.
#[derive(Default)]
struct Giver {
    asked: Vec<(String, String, GetRequest)>,
}

impl Plugin for Giver {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn get(&mut self, instance: &str, path: &str, request: &GetRequest) -> Result<(), Error> {
        self.asked
            .push((instance.to_string(), path.to_string(), request.clone()));
        match request.names.first().map(String::as_str) {
            Some("stop") => Err(Error::new(ErrorKind::Cancelled, "stopped")),
            Some("fail") => Err(Error::new(ErrorKind::Other, "it broke")),
            _ => Ok(()),
        }
    }
}

/// Answers as the SDK does for any plugin.
struct Plain;

impl Plugin for Plain {
    fn new(_host: Host) -> Self {
        Self
    }

    fn info(&self) -> Info {
        Info::default()
    }
}

fn table<P: Plugin>(host: &crate::api::NcHost) -> &'static NcPlugin {
    // SAFETY: the host's table outlives the plugin in each test, released at its end.
    unsafe { &*entry::<P>(host) }
}

fn call(table: &NcPlugin, names: &[&str], mode: u32, folder: &str) -> i32 {
    let names: Vec<NcStr> = names.iter().map(|name| NcStr::new(name)).collect();
    let raw = NcGet {
        names: names.as_ptr(),
        count: names.len(),
        destination: NcStr::new(r"C:\out"),
        moving: 1,
        mode,
        folder: NcStr::new(folder),
    };
    // SAFETY: the table's own state; the request lives through the call.
    unsafe {
        (table.get)(
            table.state,
            NcStr::new("7"),
            NcStr::new("/docs"),
            &raw const raw,
        )
    }
}

#[test]
fn the_request_comes_as_the_host_made_it() {
    let host = host(API_VERSION);
    let table = table::<Giver>(&host);
    assert_eq!(call(table, &["a.txt", "sub"], GET_SILENT | GET_VIEW, ""), 0);
    assert_eq!(call(table, &["x"], 0, r"D:\here"), 0);
    // SAFETY: the table's own state, not released yet, no call running.
    let giver = unsafe { &(*table.state.cast::<crate::State<Giver>>()).plugin };
    let (instance, path, request) = &giver.asked[0];
    assert_eq!((instance.as_str(), path.as_str()), ("7", "/docs"));
    assert_eq!(request.names, ["a.txt", "sub"]);
    assert_eq!(request.destination, Path::new(r"C:\out"));
    assert!(request.moving);
    assert!(request.mode.silent && request.mode.view && !request.mode.edit);
    assert!(!request.mode.with_dialog());
    assert_eq!(request.folder, None, "no folder given");
    let second = &giver.asked[1].2;
    assert!(second.mode.with_dialog(), "F5: the dialog");
    assert_eq!(second.folder.as_deref(), Some(Path::new(r"D:\here")));
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_stop_and_a_failure_have_their_codes() {
    let host = host(API_VERSION);
    let table = table::<Giver>(&host);
    assert_eq!(call(table, &["stop"], 0, ""), ERROR_CANCELLED);
    assert_eq!(call(table, &["fail"], 0, ""), crate::api::ERROR_OTHER);
    // SAFETY: the table's own state.
    assert_eq!(text(unsafe { (table.last_error)(table.state) }), "it broke");
    // SAFETY: the table's own state; no request at all.
    let none = unsafe { (table.get)(table.state, NcStr::EMPTY, NcStr::EMPTY, std::ptr::null()) };
    assert_eq!(none, ERROR_UNSUPPORTED);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_plugin_that_writes_none_refuses_and_the_modes_read_their_bits() {
    let host = host(API_VERSION);
    let table = table::<Plain>(&host);
    assert_eq!(call(table, &["a"], 0, ""), ERROR_UNSUPPORTED);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
    let edit = GetMode::of(GET_EDIT | crate::api::GET_QUICK_VIEW | crate::api::GET_FIND);
    assert!(edit.edit && edit.quick_view && edit.find && !edit.top_level);
    assert!(GetMode::of(crate::api::GET_TOP_LEVEL).with_dialog());
}

/// Each bit sets its own field and no other; a bit the ABI does not name (FAR's `OPM_DESCR`, 0x20) none.
#[test]
fn each_bit_is_its_own_field() {
    use crate::api::{GET_FIND, GET_QUICK_VIEW, GET_TOP_LEVEL};
    let only = |field: &str| GetMode {
        silent: field == "silent",
        find: field == "find",
        view: field == "view",
        edit: field == "edit",
        top_level: field == "top_level",
        quick_view: field == "quick_view",
    };
    assert_eq!(GetMode::of(GET_SILENT), only("silent"));
    assert_eq!(GetMode::of(GET_FIND), only("find"));
    assert_eq!(GetMode::of(GET_VIEW), only("view"));
    assert_eq!(GetMode::of(GET_EDIT), only("edit"));
    assert_eq!(GetMode::of(GET_TOP_LEVEL), only("top_level"));
    assert_eq!(GetMode::of(GET_QUICK_VIEW), only("quick_view"));
    assert_eq!(GetMode::of(0), GetMode::default());
    assert_eq!(GetMode::of(0x20), GetMode::default(), "not a mode");
    let all = GET_SILENT | GET_FIND | GET_VIEW | GET_EDIT | GET_TOP_LEVEL | GET_QUICK_VIEW;
    let every = GetMode::of(all);
    assert!(every.silent && every.find && every.view && every.edit);
    assert!(every.top_level && every.quick_view);
}

/// The dialog is shown unless any one of silent, Find file, the viewer, the editor or the quick view
/// asks; archives of a disk's panel at once still get it.
#[test]
fn any_one_quiet_mode_takes_the_dialog_away() {
    use crate::api::{GET_FIND, GET_QUICK_VIEW, GET_TOP_LEVEL};
    assert!(GetMode::default().with_dialog(), "F5");
    for bit in [GET_SILENT, GET_FIND, GET_VIEW, GET_EDIT, GET_QUICK_VIEW] {
        assert!(!GetMode::of(bit).with_dialog(), "bit {bit:#x}");
        assert!(
            !GetMode::of(bit | GET_TOP_LEVEL).with_dialog(),
            "bit {bit:#x}"
        );
    }
    assert!(GetMode::of(GET_TOP_LEVEL | 0x20).with_dialog());
}

/// The request as the raw one says: no names when the pointer is null or the count 0 (whatever the
/// other says), `moving` 0 as not moving, the destination as given.
#[test]
fn a_raw_request_without_names_or_moving_reads_so() {
    let names = [NcStr::new("a"), NcStr::new("b")];
    let raw = |names: *const NcStr, count: usize| NcGet {
        names,
        count,
        destination: NcStr::new(r"E:\to"),
        moving: 0,
        mode: GET_EDIT,
        folder: NcStr::new(r"F:\cur"),
    };
    // SAFETY: the texts live through each call; a null pointer is never read.
    let (none, empty, two) = unsafe {
        (
            GetRequest::of(&raw(std::ptr::null(), 2)),
            GetRequest::of(&raw(names.as_ptr(), 0)),
            GetRequest::of(&raw(names.as_ptr(), 2)),
        )
    };
    assert!(none.names.is_empty(), "null names");
    assert!(empty.names.is_empty(), "count 0");
    assert_eq!(two.names, ["a", "b"]);
    assert!(!two.moving, "copied (F5)");
    assert_eq!(two.destination, Path::new(r"E:\to"));
    assert_eq!(two.folder.as_deref(), Some(Path::new(r"F:\cur")));
    assert!(two.mode.edit && !two.mode.view && !two.mode.with_dialog());
}
