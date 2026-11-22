//! A plugin's files through the table: one entry (its own or found in the listing), reading from an
//! offset, writing new and appended, a folder made, renames with and without replacing, removals of a
//! file and a folder, failures by kind and a panic.

use std::collections::BTreeMap;

use super::*;
use crate::api::{
    API_VERSION, ENTRY_DIR, ERROR_EXISTS, ERROR_NOT_FOUND, ERROR_UNSUPPORTED, NcPlugin,
    REMOVE_FILE, WRITE_ABORT, WRITE_END, WRITE_NEW,
};
use crate::panel::ErrorKind;
use crate::tests::{host, text};
use crate::{FileEntry, Host, Info, entry};

/// Files by path, `dir` its folders; what it heard last.
#[derive(Default)]
struct Store {
    files: BTreeMap<String, Vec<u8>>,
    heard: Vec<String>,
}

impl Plugin for Store {
    fn new(_host: Host) -> Self {
        let mut store = Self::default();
        store
            .files
            .insert("/a.txt".to_string(), b"hello world".to_vec());
        store
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn list(&mut self, _instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        assert_eq!(path, "/", "only the root is listed");
        Ok((self.files.iter())
            .map(|(name, data)| FileEntry::file(&name[1..], data.len() as u64))
            .chain([FileEntry::folder("dir")])
            .collect())
    }

    fn read(
        &mut self,
        instance: &str,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        assert_ne!(path, "/panic", "the store panics reading");
        self.heard.push(format!("read {instance} {path} {offset}"));
        let data =
            (self.files.get(path)).ok_or_else(|| Error::new(ErrorKind::NotFound, "no file"))?;
        let from = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(data.len());
        let count = buffer.len().min(data.len() - from);
        buffer[..count].copy_from_slice(&data[from..from + count]);
        Ok(count)
    }

    fn write(
        &mut self,
        _instance: &str,
        path: &str,
        mode: WriteMode,
        data: &[u8],
    ) -> Result<(), Error> {
        match mode {
            WriteMode::New if self.files.contains_key(path) => {
                Err(Error::new(ErrorKind::Exists, "there"))
            }
            WriteMode::New => {
                self.files.insert(path.to_string(), data.to_vec());
                Ok(())
            }
            WriteMode::Append => {
                (self.files.get_mut(path))
                    .ok_or_else(|| Error::new(ErrorKind::NotFound, "no file"))?
                    .extend_from_slice(data);
                Ok(())
            }
        }
    }

    fn make_dir(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.heard.push(format!("make {instance} {path}"));
        Ok(())
    }

    fn write_end(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.heard.push(format!("end {instance} {path}"));
        match path {
            "/refused" => Err(Error::new(ErrorKind::Other, "not finished")),
            _ => Ok(()),
        }
    }

    fn write_abort(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.heard.push(format!("abort {instance} {path}"));
        Ok(())
    }

    fn rename(
        &mut self,
        _instance: &str,
        from: &str,
        to: &str,
        replace: bool,
    ) -> Result<(), Error> {
        if self.files.contains_key(to) && !replace {
            return Err(Error::new(ErrorKind::Exists, "taken"));
        }
        let data =
            (self.files.remove(from)).ok_or_else(|| Error::new(ErrorKind::NotFound, "no file"))?;
        self.files.insert(to.to_string(), data);
        Ok(())
    }

    fn remove(&mut self, _instance: &str, path: &str, folder: bool) -> Result<(), Error> {
        self.heard.push(format!("remove {path} {folder}"));
        Ok(())
    }

    fn set_times(&mut self, instance: &str, path: &str, times: &crate::Times) -> Result<(), Error> {
        self.heard
            .push(format!("times {instance} {path} {times:?}"));
        Ok(())
    }
}

/// Lists only: the other calls answer as the SDK does for any plugin.
struct Lister;

impl Plugin for Lister {
    fn new(_host: Host) -> Self {
        Self
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn list(&mut self, _instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        match path {
            "/" => Ok(vec![FileEntry::folder("docs")]),
            "/docs" => Ok(vec![FileEntry::file("a.txt", 3)]),
            _ => Err(Error::new(ErrorKind::NotFound, "nothing")),
        }
    }
}

fn table<P: Plugin>(host: &crate::api::NcHost) -> &'static NcPlugin {
    // SAFETY: the host's table outlives the plugin in each test, released at its end.
    unsafe { &*entry::<P>(host) }
}

fn store(table: &NcPlugin) -> &Store {
    // SAFETY: the table's own state, not released yet, no call running.
    unsafe { &(*table.state.cast::<crate::State<Store>>()).plugin }
}

fn failure(table: &NcPlugin) -> String {
    // SAFETY: the table's own state.
    text(unsafe { (table.last_error)(table.state) })
}

fn stat(table: &NcPlugin, path: &str) -> (i32, Option<(String, u32, u64)>) {
    let mut found = NcEntry {
        name: NcStr::EMPTY,
        flags: 0,
        size: 0,
        modified: 0,
        created: 0,
        accessed: 0,
        ..NcEntry::EMPTY
    };
    // SAFETY: the table's own state; the answer's place lives through the call.
    let code = unsafe {
        (table.stat)(
            table.state,
            NcStr::new("i"),
            NcStr::new(path),
            &raw mut found,
        )
    };
    let told = (code == 0).then(|| (text(found.name), found.flags, found.size));
    (code, told)
}

fn read(table: &NcPlugin, path: &str, offset: u64, size: usize) -> (i32, Vec<u8>) {
    let mut buffer = vec![0; size];
    let mut done = usize::MAX;
    // SAFETY: the table's own state; the buffer and the answer's place live through the call.
    let code = unsafe {
        (table.read)(
            table.state,
            NcStr::new("i"),
            NcStr::new(path),
            offset,
            buffer.as_mut_ptr(),
            size,
            &raw mut done,
        )
    };
    buffer.truncate(if code == 0 { done } else { 0 });
    (code, buffer)
}

fn write(table: &NcPlugin, path: &str, mode: u32, data: &[u8]) -> i32 {
    // SAFETY: the table's own state; the data lives through the call.
    unsafe {
        (table.write)(
            table.state,
            NcStr::new("i"),
            NcStr::new(path),
            mode,
            data.as_ptr(),
            data.len(),
        )
    }
}

#[test]
fn a_file_is_read_from_an_offset_and_short_only_at_its_end() {
    let host = host(API_VERSION);
    let table = table::<Store>(&host);
    assert_eq!(read(table, "/a.txt", 0, 5), (0, b"hello".to_vec()));
    assert_eq!(read(table, "/a.txt", 6, 100), (0, b"world".to_vec()));
    assert_eq!(
        read(table, "/a.txt", 99, 4),
        (0, Vec::new()),
        "past the end"
    );
    assert_eq!(
        read(table, "/a.txt", 0, 0),
        (0, Vec::new()),
        "nothing asked"
    );
    assert_eq!(
        store(table).heard[0],
        "read i /a.txt 0",
        "the instance and path"
    );
    assert_eq!(read(table, "/b", 0, 4).0, ERROR_NOT_FOUND);
    assert_eq!(failure(table), "no file");
    assert_eq!(read(table, "/panic", 0, 4).0, ERROR_OTHER);
    assert!(failure(table).starts_with("The plugin failed: "));
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_file_is_written_new_then_appended_and_never_new_over_another() {
    let host = host(API_VERSION);
    let table = table::<Store>(&host);
    assert_eq!(write(table, "/b.txt", WRITE_NEW, b""), 0, "no bytes yet");
    assert_eq!(write(table, "/b.txt", WRITE_APPEND, b"one "), 0);
    assert_eq!(write(table, "/b.txt", WRITE_APPEND, b"two"), 0);
    assert_eq!(read(table, "/b.txt", 0, 64), (0, b"one two".to_vec()));
    assert_eq!(write(table, "/a.txt", WRITE_NEW, b"x"), ERROR_EXISTS);
    assert_eq!(failure(table), "there");
    assert_eq!(read(table, "/a.txt", 0, 64).1, b"hello world", "untouched");
    // SAFETY: the table's own state; a null pointer with no bytes is no data.
    let code = unsafe {
        (table.write)(
            table.state,
            NcStr::new("i"),
            NcStr::new("/c"),
            WRITE_NEW,
            std::ptr::null(),
            0,
        )
    };
    assert_eq!(code, 0);
    assert_eq!(read(table, "/c", 0, 8), (0, Vec::new()));
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

/// API 9's end of a written file reaches `write_end` (its failure the code's), its abandonment
/// `write_abort`, never a write; a mode a later host may add is refused.
#[test]
fn a_written_files_end_is_heard() {
    let host = host(API_VERSION);
    let table = table::<Store>(&host);
    assert_eq!(write(table, "/b.txt", WRITE_NEW, b"x"), 0);
    let before = store(table).heard.len();
    assert_eq!(write(table, "/b.txt", WRITE_END, b""), 0);
    assert_eq!(write(table, "/b.txt", WRITE_ABORT, b""), 0);
    assert_eq!(
        store(table).heard[before..],
        ["end i /b.txt", "abort i /b.txt"]
    );
    assert_eq!(read(table, "/b.txt", 0, 8).1, b"x", "nothing written");
    assert_ne!(write(table, "/refused", WRITE_END, b""), 0);
    assert_eq!(failure(table), "not finished");
    assert_ne!(write(table, "/b.txt", 7, b""), 0, "a mode not known");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn folders_renames_and_removals_reach_the_plugin_as_asked() {
    let host = host(API_VERSION);
    let table = table::<Store>(&host);
    let call = |name: &str| NcStr::new(name);
    // SAFETY: the table's own state, one call at a time; released once at the end.
    unsafe {
        assert_eq!((table.make_dir)(table.state, call("i"), call("/new")), 0);
        assert_eq!(
            (table.remove)(table.state, call("i"), call("/f"), REMOVE_FILE),
            0
        );
        assert_eq!(
            (table.remove)(table.state, call("i"), call("/d"), REMOVE_FOLDER),
            0
        );
        assert_eq!(write(table, "/b", WRITE_NEW, b"b"), 0);
        let rename = |from: &str, to: &str, replace: u32| {
            (table.rename)(table.state, call("i"), call(from), call(to), replace)
        };
        assert_eq!(rename("/a.txt", "/b", 0), ERROR_EXISTS, "not over another");
        assert_eq!(rename("/a.txt", "/b", 1), 0, "replacing");
        assert_eq!(read(table, "/b", 0, 64).1, b"hello world");
        assert_eq!(rename("/gone", "/x", 0), ERROR_NOT_FOUND);
        assert_eq!(
            store(table).heard,
            [
                "make i /new",
                "remove /f false",
                "remove /d true",
                "read i /b 0"
            ]
        );
        (table.release)(table.state);
    }
}

#[test]
fn one_entry_is_found_in_its_folders_listing_unless_the_plugin_tells_it() {
    let host = host(API_VERSION);
    let table = table::<Lister>(&host);
    assert_eq!(
        stat(table, "/docs"),
        (0, Some(("docs".to_string(), ENTRY_DIR, 0)))
    );
    assert_eq!(
        stat(table, "/docs/a.txt"),
        (0, Some(("a.txt".to_string(), 0, 3)))
    );
    assert_eq!(
        stat(table, "/"),
        (0, Some((String::new(), ENTRY_DIR, 0))),
        "the root"
    );
    assert_eq!(stat(table, "/docs/b").0, ERROR_NOT_FOUND);
    assert_eq!(failure(table), "/docs/b is not there");
    assert_eq!(
        stat(table, "/none/b").0,
        ERROR_NOT_FOUND,
        "its folder's failure"
    );
    assert_eq!(failure(table), "nothing");
    let unsupported = [
        read(table, "/docs/a.txt", 0, 4).0,
        write(table, "/x", WRITE_NEW, b""),
    ];
    assert_eq!(unsupported, [ERROR_UNSUPPORTED; 2]);
    assert_eq!(failure(table), "/x cannot be written");
    // SAFETY: the table's own state, one call at a time; released once at the end.
    unsafe {
        let call = |name: &str| NcStr::new(name);
        assert_eq!(
            (table.make_dir)(table.state, call("i"), call("/d")),
            ERROR_UNSUPPORTED
        );
        let renamed = (table.rename)(table.state, call("i"), call("/a"), call("/b"), 0);
        assert_eq!(renamed, ERROR_UNSUPPORTED);
        let removed = (table.remove)(table.state, call("i"), call("/a"), REMOVE_FILE);
        assert_eq!(removed, ERROR_UNSUPPORTED);
        assert_eq!(failure(table), "/a cannot be deleted");
        (table.release)(table.state);
    }
}

#[test]
fn a_mode_or_a_removal_this_build_does_not_know_is_refused_never_taken_for_another() {
    let host = host(API_VERSION);
    let table = table::<Store>(&host);
    assert_eq!(write(table, "/n", 7, b"x"), ERROR_UNSUPPORTED);
    assert_eq!(failure(table), "No write mode 7");
    assert_eq!(read(table, "/n", 0, 4).0, ERROR_NOT_FOUND, "nothing made");
    // SAFETY: the table's own state, one call at a time; released once at the end.
    unsafe {
        let removed = (table.remove)(table.state, NcStr::new("i"), NcStr::new("/a.txt"), 2);
        assert_eq!(removed, ERROR_UNSUPPORTED);
        assert_eq!(failure(table), "No removal 2");
        assert!(
            store(table)
                .heard
                .iter()
                .all(|heard| !heard.starts_with("remove"))
        );
        // A null buffer of some size: nothing read, nothing told read.
        let mut done = usize::MAX;
        let code = (table.read)(
            table.state,
            NcStr::new("i"),
            NcStr::new("/a.txt"),
            0,
            std::ptr::null_mut(),
            8,
            &raw mut done,
        );
        assert_eq!((code, done), (0, 0));
        (table.release)(table.state);
    }
}

#[test]
fn what_a_file_put_in_keeps_reaches_the_plugin_as_given() {
    use std::time::{Duration, UNIX_EPOCH};

    use crate::api::{ENTRY_HIDDEN, ENTRY_READ_ONLY, NcTimes, TIME_NONE};
    use crate::{Attributes, Times};

    let host = host(API_VERSION);
    let lister = table::<Lister>(&host);
    let table = table::<Store>(&host);
    let given = NcTimes {
        modified: 2_000,
        created: -1_000,
        flags: ENTRY_HIDDEN | ENTRY_READ_ONLY,
        flags_given: 1,
        ..NcTimes::NONE
    };
    let call = |times: *const NcTimes| {
        // SAFETY: the table's own state, one call at a time; the times live through the call.
        unsafe { (table.set_times)(table.state, NcStr::new("i"), NcStr::new("/a.txt"), times) }
    };
    assert_eq!(call(&raw const given), 0);
    let kept = Times {
        modified: Some(UNIX_EPOCH + Duration::from_secs(2)),
        created: UNIX_EPOCH.checked_sub(Duration::from_secs(1)),
        attributes: Some(Attributes {
            hidden: true,
            system: false,
            read_only: true,
            bits: 0,
        }),
        ..Times::default()
    };
    let heard = format!("times i /a.txt {kept:?}");
    assert_eq!(store(table).heard.last(), Some(&heard));
    let no_marks = NcTimes {
        flags: ENTRY_HIDDEN,
        ..NcTimes::NONE
    };
    assert_eq!(call(&raw const no_marks), 0);
    let heard = format!("times i /a.txt {:?}", Times::default());
    assert_eq!(store(table).heard.last(), Some(&heard), "marks not given");
    assert_eq!(NcTimes::NONE.changed, TIME_NONE);
    assert_eq!(
        call(std::ptr::null()),
        ERROR_UNSUPPORTED,
        "no times: refused"
    );
    assert_eq!(failure(table), "No times for /a.txt");
    // SAFETY: the table's own state, one call at a time; released once at the end.
    unsafe {
        let none = NcTimes::NONE;
        let code = (lister.set_times)(
            lister.state,
            NcStr::new("i"),
            NcStr::new("/x"),
            &raw const none,
        );
        assert_eq!(code, ERROR_UNSUPPORTED, "keeps none by default");
        assert_eq!(failure(lister), "/x keeps no times");
        (lister.release)(lister.state);
        (table.release)(table.state);
    }
}
