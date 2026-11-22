//! Files taken in by the plugin through the table: the request as the host made it, a plugin that takes
//! none refusing, a failure and a stop by their codes, a raw request without sources.
use std::path::{Path, PathBuf};

use super::*;
use crate::api::{API_VERSION, ERROR_CANCELLED, ERROR_UNSUPPORTED, GET_EDIT, GET_SILENT, NcPlugin};
use crate::panel::{Error, ErrorKind};
use crate::tests::{host, text};
use crate::{Host, Info, entry};

/// Keeps what it was asked; a source named `stop` answers as the user stopped it, `fail` as it failed.
#[derive(Default)]
struct Taker {
    asked: Vec<(String, String, PutRequest)>,
}

impl Plugin for Taker {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn put(&mut self, instance: &str, path: &str, request: &PutRequest) -> Result<(), Error> {
        self.asked
            .push((instance.to_string(), path.to_string(), request.clone()));
        let first = request
            .sources
            .first()
            .and_then(|source| source.file_name());
        match first.and_then(|name| name.to_str()) {
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

fn call(table: &NcPlugin, sources: &[&str], (moving, mode): (u32, u32), folder: &str) -> i32 {
    let sources: Vec<NcStr> = sources.iter().map(|source| NcStr::new(source)).collect();
    let raw = NcPut {
        sources: sources.as_ptr(),
        count: sources.len(),
        moving,
        mode,
        folder: NcStr::new(folder),
    };
    // SAFETY: the table's own state; the request lives through the call.
    unsafe {
        (table.put)(
            table.state,
            NcStr::new("3"),
            NcStr::new("/docs"),
            &raw const raw,
        )
    }
}

#[test]
fn the_request_comes_as_the_host_made_it() {
    let host = host(API_VERSION);
    let table = table::<Taker>(&host);
    assert_eq!(call(table, &[r"C:\a.txt", r"C:\sub"], (1, 0), r"C:\"), 0);
    assert_eq!(
        call(table, &[r"D:\t\x.txt"], (0, GET_SILENT | GET_EDIT), ""),
        0
    );
    // SAFETY: the table's own state, not released yet, no call running.
    let taker = unsafe { &(*table.state.cast::<crate::State<Taker>>()).plugin };
    let (instance, path, request) = &taker.asked[0];
    assert_eq!((instance.as_str(), path.as_str()), ("3", "/docs"));
    assert_eq!(
        request.sources,
        [PathBuf::from(r"C:\a.txt"), PathBuf::from(r"C:\sub")]
    );
    assert!(request.moving, "F6");
    assert!(request.mode.with_dialog(), "F5 / F6: the dialog");
    assert_eq!(request.folder.as_deref(), Some(Path::new(r"C:\")));
    let edited = &taker.asked[1].2;
    assert!(!edited.moving && edited.mode.edit && edited.mode.silent);
    assert_eq!(edited.folder, None, "no folder given");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_stop_a_failure_and_no_request_have_their_codes() {
    let host = host(API_VERSION);
    let table = table::<Taker>(&host);
    assert_eq!(call(table, &[r"C:\stop"], (0, 0), ""), ERROR_CANCELLED);
    assert_eq!(
        call(table, &[r"C:\fail"], (0, 0), ""),
        crate::api::ERROR_OTHER
    );
    // SAFETY: the table's own state.
    assert_eq!(text(unsafe { (table.last_error)(table.state) }), "it broke");
    // SAFETY: the table's own state; no request at all.
    let none = unsafe { (table.put)(table.state, NcStr::EMPTY, NcStr::EMPTY, std::ptr::null()) };
    assert_eq!(none, ERROR_UNSUPPORTED);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_plugin_that_takes_none_refuses() {
    let host = host(API_VERSION);
    let table = table::<Plain>(&host);
    assert_eq!(call(table, &[r"C:\a"], (0, 0), ""), ERROR_UNSUPPORTED);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

/// No sources when the pointer is null or the count 0 (whatever the other says).
#[test]
fn a_raw_request_without_sources_reads_so() {
    let sources = [NcStr::new(r"C:\a"), NcStr::new(r"C:\b")];
    let raw = |sources: *const NcStr, count: usize| NcPut {
        sources,
        count,
        moving: 0,
        mode: 0,
        folder: NcStr::EMPTY,
    };
    // SAFETY: the texts live through each call; a null pointer is never read.
    let (none, empty, two) = unsafe {
        (
            PutRequest::of(&raw(std::ptr::null(), 2)),
            PutRequest::of(&raw(sources.as_ptr(), 0)),
            PutRequest::of(&raw(sources.as_ptr(), 2)),
        )
    };
    assert!(none.sources.is_empty(), "null sources");
    assert!(empty.sources.is_empty(), "count 0");
    assert_eq!(two.sources.len(), 2);
    assert!(!two.moving && two.folder.is_none());
}

/// Any non-zero `moving` is a move; the mode's bits each as they are (the editor's copy alone: no
/// silence); only `count` texts are read.
#[test]
fn a_raw_requests_flags_read_as_given() {
    let sources = [NcStr::new(r"C:\a"), NcStr::new(r"C:\b")];
    let raw = NcPut {
        sources: sources.as_ptr(),
        count: 1,
        moving: 2,
        mode: GET_EDIT,
        folder: NcStr::new(r"C:\from"),
    };
    // SAFETY: the texts live through the call; `count` is within them.
    let request = unsafe { PutRequest::of(&raw) };
    assert_eq!(request.sources, [PathBuf::from(r"C:\a")]);
    assert!(request.moving, "any non-zero");
    assert!(request.mode.edit && !request.mode.silent && !request.mode.view);
    assert_eq!(request.folder.as_deref(), Some(Path::new(r"C:\from")));
}
