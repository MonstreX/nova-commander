//! A panel of references through the table: its flags, format and start mode in its look, paths put
//! reaching it with the move, a failure telling its kind.

use super::*;
use crate::api::{API_VERSION, ERROR_DENIED};
use crate::tests::{host, text};
use crate::{Host, Info, entry};

/// A panel of references: it takes paths, opens at Ctrl+4 (`far` past the keys), keeps what it is put.
#[derive(Default)]
struct Refs {
    put: Vec<(String, Vec<std::path::PathBuf>, bool)>,
}

impl Plugin for Refs {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn panel_start_mode(&self, instance: &str, _path: &str) -> Option<usize> {
        assert_ne!(instance, "panic", "the plugin panics telling its mode");
        Some(if instance == "far" { 10 } else { 4 })
    }

    fn panel_flags(&self, instance: &str) -> PanelFlags {
        PanelFlags {
            takes_paths: instance != "data",
            names_only: instance == "far",
            right_align_names: instance == "far",
            preserve_case: instance == "far",
            own_get: instance == "own",
            own_delete: instance == "own",
            gives_files: instance == "own",
            deletes: instance == "own",
            makes_folders: instance == "own",
            puts_on_save: instance == "own",
        }
    }

    fn panel_format(&self, instance: &str, _path: &str) -> String {
        if instance == "main" {
            "References".to_string()
        } else {
            String::new()
        }
    }

    fn put(
        &mut self,
        _instance: &str,
        path: &str,
        request: &crate::PutRequest,
    ) -> Result<(), Error> {
        if path == "/full" {
            return Err(Error::new(ErrorKind::Denied, "full"));
        }
        let crate::PutRequest {
            sources, moving, ..
        } = request;
        self.put.push((path.to_string(), sources.clone(), *moving));
        Ok(())
    }
}

#[test]
fn a_panel_of_references_tells_its_flag_and_start_mode() {
    let host = host(API_VERSION);
    // SAFETY: the host's table outlives the plugin; released at the end.
    let table = unsafe { &*entry::<Refs>(&raw const host) };
    let info = |instance: &str| {
        // SAFETY: the table's own state; the texts live through the call.
        unsafe { (table.panel_info)(table.state, NcStr::new(instance), NcStr::new("/")) }
    };
    let main = info("main");
    assert_eq!((main.flags, main.start_mode), (PANEL_TAKES_PATHS, 4));
    assert_eq!(text(main.format), "References");
    assert_eq!(info("data").flags, 0, "its data, not paths");
    assert_eq!(text(info("data").format), "", "none given");
    let far = info("far");
    assert_eq!(far.start_mode, START_MODE_NONE, "past Ctrl+9: none");
    assert_eq!(
        far.flags,
        PANEL_TAKES_PATHS | PANEL_NAMES_ONLY | PANEL_RIGHT_ALIGN_NAMES | PANEL_PRESERVE_CASE
    );
    assert_eq!(
        info("own").flags,
        PANEL_TAKES_PATHS
            | crate::api::PANEL_OWN_GET
            | crate::api::PANEL_OWN_DELETE
            | crate::api::PANEL_GIVES_FILES
            | crate::api::PANEL_DELETES
            | crate::api::PANEL_MAKES_FOLDERS
            | crate::api::PANEL_PUTS_ON_SAVE,
        "its real names its own, its files given, deleted and made by it, put back at each save"
    );
    assert_eq!(info("panic").start_mode, START_MODE_NONE, "a panic: none");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn paths_put_reach_the_plugin_with_the_move_and_a_failure_tells_its_kind() {
    let host = host(API_VERSION);
    // SAFETY: the host's table outlives the plugin; released at the end.
    let table = unsafe { &*entry::<Refs>(&raw const host) };
    let sources = [NcStr::new(r"C:\a.txt"), NcStr::new(r"D:\b")];
    let put = |path: &str, sources: *const NcStr, count: usize, moving: u32| {
        let request = crate::api::NcPut {
            sources,
            count,
            moving,
            mode: 0,
            folder: NcStr::EMPTY,
        };
        // SAFETY: the table's own state; the texts, the list and the request live through the call.
        unsafe {
            (table.put)(
                table.state,
                NcStr::new("main"),
                NcStr::new(path),
                &raw const request,
            )
        }
    };
    assert_eq!(put("/", sources.as_ptr(), 2, 0), 0);
    assert_eq!(put("/", sources.as_ptr(), 1, 1), 0);
    assert_eq!(put("/", std::ptr::null(), 3, 0), 0, "no list: none");
    assert_eq!(put("/full", sources.as_ptr(), 2, 0), ERROR_DENIED);
    // SAFETY: the table's own state.
    assert_eq!(text(unsafe { (table.last_error)(table.state) }), "full");
    // SAFETY: the table's own state, no call running.
    let heard = unsafe { &(*table.state.cast::<crate::State<Refs>>()).plugin.put };
    let paths = |texts: &[&str]| {
        texts
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        *heard,
        [
            ("/".to_string(), paths(&[r"C:\a.txt", r"D:\b"]), false),
            ("/".to_string(), paths(&[r"C:\a.txt"]), true),
            ("/".to_string(), Vec::new(), false),
        ]
    );
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}
