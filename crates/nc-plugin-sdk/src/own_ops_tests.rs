//! Deleting and making folders by the plugin through the table: the names and mode as the host made
//! them, the name made back, a plugin that does neither refusing, a stop and a failure by their codes.
use super::*;
use crate::api::{API_VERSION, ERROR_CANCELLED, ERROR_UNSUPPORTED, GET_SILENT, NcPlugin};
use crate::panel::{Error, ErrorKind};
use crate::tests::{host, text};
use crate::{Host, Info, entry};

/// Keeps what it was asked; `stop` answers as the user stopped it, `fail` as it failed.
#[derive(Default)]
struct Own {
    deleted: Vec<(String, String, Vec<String>, GetMode)>,
    made: Vec<(String, String)>,
}

impl Plugin for Own {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn delete_files(
        &mut self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: GetMode,
    ) -> Result<(), Error> {
        self.deleted
            .push((instance.to_string(), path.to_string(), names.to_vec(), mode));
        match names.first().map(String::as_str) {
            Some("stop") => Err(Error::new(ErrorKind::Cancelled, "stopped")),
            Some("fail") => Err(Error::new(ErrorKind::Other, "it broke")),
            _ => Ok(()),
        }
    }

    fn make_folder(
        &mut self,
        _instance: &str,
        path: &str,
        name: &str,
        _mode: GetMode,
    ) -> Result<String, Error> {
        self.made.push((path.to_string(), name.to_string()));
        match name {
            "stop" => Err(Error::new(ErrorKind::Cancelled, "stopped")),
            "" => Ok("asked name".to_string()),
            given => Ok(format!("{given} made")),
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

fn delete(table: &NcPlugin, names: &[&str], mode: u32) -> i32 {
    let names: Vec<NcStr> = names.iter().map(|name| NcStr::new(name)).collect();
    // SAFETY: the table's own state; the names live through the call.
    unsafe {
        (table.delete_files)(
            table.state,
            NcStr::new("2"),
            NcStr::new("/docs"),
            names.as_ptr(),
            names.len(),
            mode,
        )
    }
}

fn make(table: &NcPlugin, name: &str) -> (i32, String) {
    let mut answer = NcStr::new(name);
    // SAFETY: the table's own state; the name lives through the call.
    let code = unsafe {
        (table.make_folder)(
            table.state,
            NcStr::new("2"),
            NcStr::new("/docs"),
            0,
            &raw mut answer,
        )
    };
    (code, text(answer))
}

#[test]
fn the_names_come_as_the_host_gave_them() {
    let host = host(API_VERSION);
    let table = table::<Own>(&host);
    assert_eq!(delete(table, &["a.txt", "sub"], 0), 0);
    assert_eq!(delete(table, &["x"], GET_SILENT), 0);
    assert_eq!(delete(table, &["stop"], 0), ERROR_CANCELLED);
    assert_eq!(delete(table, &["fail"], 0), crate::api::ERROR_OTHER);
    // SAFETY: the table's own state.
    assert_eq!(text(unsafe { (table.last_error)(table.state) }), "it broke");
    // SAFETY: the table's own state, not released yet, no call running.
    let own = unsafe { &(*table.state.cast::<crate::State<Own>>()).plugin };
    let (instance, path, names, mode) = &own.deleted[0];
    assert_eq!((instance.as_str(), path.as_str()), ("2", "/docs"));
    assert_eq!(names, &["a.txt", "sub"]);
    assert!(mode.with_dialog(), "F8: its question");
    assert!(own.deleted[1].3.silent);
    // SAFETY: the table's own state; a null list is none.
    let none = unsafe {
        (table.delete_files)(
            table.state,
            NcStr::EMPTY,
            NcStr::EMPTY,
            std::ptr::null(),
            5,
            0,
        )
    };
    assert_eq!(none, 0);
    // SAFETY: the table's own state, not released yet, no call running.
    let own = unsafe { &(*table.state.cast::<crate::State<Own>>()).plugin };
    assert!(own.deleted[4].2.is_empty(), "no names at a null pointer");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn the_name_made_comes_back() {
    let host = host(API_VERSION);
    let table = table::<Own>(&host);
    assert_eq!(make(table, ""), (0, "asked name".to_string()), "F7: asked");
    assert_eq!(make(table, "new"), (0, "new made".to_string()));
    assert_eq!(make(table, "stop").0, ERROR_CANCELLED);
    // SAFETY: the table's own state; no place for the answer.
    let none = unsafe {
        (table.make_folder)(
            table.state,
            NcStr::EMPTY,
            NcStr::EMPTY,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_eq!(none, ERROR_UNSUPPORTED);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_plugin_that_does_neither_refuses() {
    let host = host(API_VERSION);
    let table = table::<Plain>(&host);
    assert_eq!(delete(table, &["a"], 0), ERROR_UNSUPPORTED);
    assert_eq!(make(table, "").0, ERROR_UNSUPPORTED);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

/// Shift+F1's new panel: each call answers the next of its answers, counting the calls.
#[derive(Default)]
struct Opens {
    calls: usize,
}

impl Plugin for Opens {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn open_new(&mut self) -> Result<Option<String>, Error> {
        self.calls += 1;
        match self.calls {
            1 => Ok(Some("new panel 1".to_string())),
            2 => Ok(None),
            3 => Err(Error::new(ErrorKind::Cancelled, "stopped")),
            4 => Err(Error::new(ErrorKind::Other, "no room for it")),
            5 => panic!("open_new broke"),
            _ => Ok(Some("Привет".to_string())),
        }
    }
}

/// The text the host's place holds before the call: what an answer not written leaves.
const UNTOUCHED: &str = "untouched";

/// One `open_new` call through the table: its code and the instance's place after it.
fn open_new(table: &NcPlugin) -> (i32, String) {
    let mut answer = NcStr::new(UNTOUCHED);
    // SAFETY: the table's own state; the answer's place lives through the call, its text read at once.
    let code = unsafe { (table.open_new)(table.state, &raw mut answer) };
    (code, text(answer))
}

fn last_error(table: &NcPlugin) -> String {
    // SAFETY: the table's own state.
    text(unsafe { (table.last_error)(table.state) })
}

#[test]
fn a_new_panels_instance_comes_back_and_none_is_an_empty_text() {
    let host = host(API_VERSION);
    let table = table::<Opens>(&host);
    assert_eq!(open_new(table), (0, "new panel 1".to_string()));
    assert_eq!(
        open_new(table),
        (0, String::new()),
        "none: the place emptied (the next plugin's)"
    );
    let (code, place) = open_new(table);
    assert_eq!(code, ERROR_CANCELLED, "the user stopped it");
    assert_eq!(place, UNTOUCHED, "a stop writes no instance");
    let (code, place) = open_new(table);
    assert_eq!(code, crate::api::ERROR_OTHER);
    assert_eq!(place, UNTOUCHED, "a failure writes no instance");
    assert_eq!(last_error(table), "no room for it", "its reason");
    let (code, place) = open_new(table);
    assert_eq!(code, crate::api::ERROR_OTHER, "a panic is a failure");
    assert_eq!(place, UNTOUCHED);
    let error = last_error(table);
    assert!(error.starts_with("The plugin failed"), "{error}");
    // The instance kept by the plugin until its next call: read whole after another call of the host's.
    let mut answer = NcStr::new(UNTOUCHED);
    // SAFETY: the table's own state; the answer's place lives through the call.
    let code = unsafe { (table.open_new)(table.state, &raw mut answer) };
    assert_eq!(code, 0);
    let _ = last_error(table);
    assert_eq!(text(answer), "Привет", "kept, not a text already dropped");
    // SAFETY: the table's own state, not released yet, no call running.
    let own = unsafe { &(*table.state.cast::<crate::State<Opens>>()).plugin };
    assert_eq!(own.calls, 6, "each call the plugin's once");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_new_panel_without_a_place_for_it_is_refused_before_the_plugin_is_asked() {
    let host = host(API_VERSION);
    let table = table::<Opens>(&host);
    // SAFETY: the table's own state; no place for the answer.
    let none = unsafe { (table.open_new)(table.state, std::ptr::null_mut()) };
    assert_eq!(none, ERROR_UNSUPPORTED);
    assert_eq!(last_error(table), "No place for the panel");
    // SAFETY: the table's own state, not released yet, no call running.
    let own = unsafe { &(*table.state.cast::<crate::State<Opens>>()).plugin };
    assert_eq!(own.calls, 0, "the plugin not asked");
    assert_eq!(
        open_new(table),
        (0, "new panel 1".to_string()),
        "its first answer still"
    );
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_plugin_that_makes_no_new_panels_answers_none() {
    let host = host(API_VERSION);
    let table = table::<Plain>(&host);
    assert_eq!(open_new(table), (0, String::new()), "the SDK's default");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

/// Shift+F3's commands: what it was handed, and its answer by the first name.
#[derive(Default)]
struct Commands {
    handed: Vec<(String, String, Vec<String>, GetMode)>,
}

impl Plugin for Commands {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn process_host_file(
        &mut self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: GetMode,
    ) -> Result<HostFileDone, Error> {
        self.handed
            .push((instance.to_string(), path.to_string(), names.to_vec(), mode));
        match names.first().map(String::as_str) {
            Some("changed") => Ok(HostFileDone {
                changed: true,
                unselect: false,
            }),
            Some("unselect") => Ok(HostFileDone {
                changed: false,
                unselect: true,
            }),
            Some("both") => Ok(HostFileDone {
                changed: true,
                unselect: true,
            }),
            Some("stop") => Err(Error::new(ErrorKind::Cancelled, "stopped")),
            Some("fail") => Err(Error::new(ErrorKind::Other, "it broke")),
            _ => Ok(HostFileDone::default()),
        }
    }
}

/// The bits a value no answer writes.
const NOT_WRITTEN: u32 = 0xDEAD;

/// One `process_host_file` call through the table: its code and the answer's place after it.
fn host_file(table: &NcPlugin, names: &[&str], mode: u32) -> (i32, u32) {
    let names: Vec<NcStr> = names.iter().map(|name| NcStr::new(name)).collect();
    let mut done = NOT_WRITTEN;
    // SAFETY: the table's own state; the names and the answer's place live through the call.
    let code = unsafe {
        (table.process_host_file)(
            table.state,
            NcStr::new("3"),
            NcStr::new("/docs"),
            names.as_ptr(),
            names.len(),
            mode,
            &raw mut done,
        )
    };
    (code, done)
}

#[test]
fn the_commands_on_a_panels_file_answer_what_they_did() {
    use crate::api::{HOST_FILE_CHANGED, HOST_FILE_UNSELECT};
    let host = host(API_VERSION);
    let table = table::<Commands>(&host);
    assert_eq!(
        host_file(table, &["changed", "b"], 0),
        (0, HOST_FILE_CHANGED)
    );
    assert_eq!(host_file(table, &["unselect"], 0), (0, HOST_FILE_UNSELECT));
    let both = HOST_FILE_CHANGED | HOST_FILE_UNSELECT;
    assert_eq!(host_file(table, &["both"], 0), (0, both));
    assert_eq!(host_file(table, &["nothing"], 0), (0, 0));
    assert_eq!(host_file(table, &[], 0), (0, 0), "no names");
    assert_eq!(host_file(table, &["stop"], 0), (ERROR_CANCELLED, 0));
    let (code, done) = host_file(table, &["fail"], GET_SILENT);
    assert_eq!((code, done), (crate::api::ERROR_OTHER, 0), "nothing done");
    assert_eq!(last_error(table), "it broke");
    // SAFETY: the table's own state, not released yet, no call running.
    let own = unsafe { &(*table.state.cast::<crate::State<Commands>>()).plugin };
    let first = &own.handed[0];
    assert_eq!(
        (first.0.as_str(), first.1.as_str()),
        ("3", "/docs"),
        "the panel and its folder"
    );
    assert_eq!(first.2, ["changed", "b"], "the names as the host gave them");
    assert_eq!(first.3, GetMode::default());
    assert!(own.handed[6].3.silent, "the mode as the host gave it");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn commands_without_a_place_for_the_answer_are_refused_and_the_default_does_nothing() {
    let host = host(API_VERSION);
    let plain = table::<Plain>(&host);
    let table = table::<Commands>(&host);
    let names = [NcStr::new("changed")];
    // SAFETY: the table's own state; no place for the answer.
    let code = unsafe {
        (table.process_host_file)(
            table.state,
            NcStr::new("3"),
            NcStr::new("/"),
            names.as_ptr(),
            1,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_eq!(code, ERROR_UNSUPPORTED);
    assert_eq!(last_error(table), "No place for the answer");
    // SAFETY: the table's own state, not released yet, no call running.
    let own = unsafe { &(*table.state.cast::<crate::State<Commands>>()).plugin };
    assert!(own.handed.is_empty(), "the plugin not asked");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
    assert_eq!(
        host_file(plain, &["changed"], 0),
        (0, 0),
        "the SDK's default"
    );
    // SAFETY: the table's own state, released once.
    unsafe { (plain.release)(plain.state) };
}
