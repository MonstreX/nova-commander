//! Through 7-Zip's installed library: archives made from nothing, then changed, written into memory and
//! read back; the callbacks' calls on the caller's thread.

use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};
use std::path::Path;

use super::super::{Archive, InStream, Library, Opening, UpdateItem, prop};
use super::*;

struct Quiet;
impl Opening for Quiet {
    fn password(&mut self) -> Option<String> {
        Some("secret".to_string())
    }
}

/// A new item: its path and data (none: a folder).
#[derive(Clone)]
struct New {
    path: &'static str,
    data: Option<&'static [u8]>,
}

/// The update's answers: the items of the new archive (an old one kept by its index, or a new one),
/// the output in memory, what was asked.
#[derive(Default)]
struct Sink {
    items: Vec<Result<u32, New>>,
    sources: Sources,
    out: std::io::Cursor<Vec<u8>>,
    password: Option<String>,
    total: Option<u64>,
    results: Vec<bool>,
    thread: Option<std::thread::ThreadId>,
    stop_at_stream: bool,
}

impl Sink {
    fn on_thread(&mut self) {
        let here = std::thread::current().id();
        assert_eq!(*self.thread.get_or_insert(here), here, "one thread");
    }

    fn new_item(&self, index: u32) -> &New {
        match &self.items[index as usize] {
            Err(new) => new,
            Ok(_) => panic!("item {index} is an old one"),
        }
    }
}

impl Updating for Sink {
    fn set_total(&mut self, total: u64) -> bool {
        self.on_thread();
        self.total = Some(total);
        true
    }

    fn item(&mut self, index: u32) -> UpdateItem {
        self.on_thread();
        match &self.items[index as usize] {
            Ok(old) => UpdateItem {
                new_data: false,
                new_properties: false,
                in_archive: Some(*old),
            },
            Err(_) => UpdateItem {
                new_data: true,
                new_properties: true,
                in_archive: None,
            },
        }
    }

    fn property(&mut self, index: u32, id: u32) -> Prop {
        self.on_thread();
        let new = self.new_item(index).clone();
        match id {
            prop::PATH => Prop::Text(new.path.to_string()),
            prop::IS_DIR => Prop::Bool(new.data.is_none()),
            prop::SIZE => Prop::U64(new.data.map_or(0, |data| data.len() as u64)),
            prop::ATTRIBUTES => Prop::U32(if new.data.is_none() { 0x10 } else { 0x20 }),
            prop::MODIFIED => Prop::Time(133_500_000_000_000_000),
            _ => Prop::Empty,
        }
    }

    fn stream(&mut self, index: u32) -> Feed {
        self.on_thread();
        if self.stop_at_stream {
            return Feed::Stop;
        }
        let new = self.new_item(index).clone();
        let Some(data) = new.data else {
            return Feed::Nothing;
        };
        // One name a source across the tests running at once.
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("nc-sdk-update-{}-{next}.bin", std::process::id()));
        std::fs::write(&path, data).unwrap();
        self.sources.0.push(path.clone());
        Feed::File(std::fs::File::open(path).unwrap())
    }

    fn done(&mut self, ok: bool) -> bool {
        self.on_thread();
        self.results.push(ok);
        true
    }

    fn password(&mut self) -> Option<String> {
        self.on_thread();
        self.password.clone()
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.on_thread();
        self.out.write_all(data)
    }

    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.on_thread();
        self.out.seek(to)
    }

    fn set_size(&mut self, size: u64) -> io::Result<()> {
        self.on_thread();
        self.out.get_mut().resize(usize::try_from(size).unwrap(), 0);
        Ok(())
    }
}

/// The source files made for the data, deleted with the sink.
#[derive(Default)]
struct Sources(Vec<std::path::PathBuf>);

impl Drop for Sources {
    fn drop(&mut self) {
        for source in &self.0 {
            let _ = std::fs::remove_file(source);
        }
    }
}

fn format_of(library: &Library, name: &str) -> usize {
    (library.formats().iter())
        .position(|format| format.name == name)
        .unwrap()
}

/// `bytes` opened as the format `name` (its file in `folder`).
fn opened(library: &Library, folder: &Path, name: &str, bytes: &[u8]) -> Archive {
    let path = folder.join(format!("out.{name}"));
    std::fs::write(&path, bytes).unwrap();
    let stream = InStream::file(&path).unwrap();
    (library.open(
        format_of(library, name),
        &stream,
        Some(&path),
        false,
        &mut Quiet,
    ))
    .unwrap()
    .unwrap()
}

/// Each item's path and its data (empty for a folder).
fn contents(archive: &Archive) -> BTreeMap<String, Vec<u8>> {
    struct Gather {
        current: Option<u32>,
        data: BTreeMap<u32, Vec<u8>>,
    }
    impl super::super::Extracting for Gather {
        fn item(&mut self, index: u32, mode: super::super::AskMode) -> Option<bool> {
            self.current = Some(index);
            self.data.entry(index).or_default();
            Some(mode == super::super::AskMode::Extract)
        }
        fn write(&mut self, data: &[u8]) -> bool {
            let index = self.current.unwrap();
            self.data.get_mut(&index).unwrap().extend_from_slice(data);
            true
        }
        fn done(&mut self, outcome: super::super::Outcome) -> bool {
            assert_eq!(outcome, super::super::Outcome::Ok);
            true
        }
        fn password(&mut self) -> Option<String> {
            Some("secret".to_string())
        }
    }
    let indices: Vec<u32> = (0..archive.count()).collect();
    let mut gather = Gather {
        current: None,
        data: BTreeMap::new(),
    };
    archive.extract(&indices, false, &mut gather).unwrap();
    (0..archive.count())
        .map(|index| {
            let path = archive
                .property(index, prop::PATH)
                .text()
                .unwrap()
                .to_string();
            (path, gather.data.remove(&index).unwrap_or_default())
        })
        .collect()
}

fn new(path: &'static str, data: Option<&'static [u8]>) -> Result<u32, New> {
    Err(New { path, data })
}

/// A new 7z of a folder and two files, its level set: read back whole; every call on the caller's
/// thread; each item's result good.
#[test]
fn a_new_7z_is_made() {
    let folder = tempfile::tempdir().unwrap();
    let library = Library::find(folder.path());
    let mut sink = Sink {
        items: vec![
            new("docs", None),
            new(r"docs\readme.txt", Some(b"hello")),
            new("top.txt", Some(b"top")),
        ],
        ..Sink::default()
    };
    let properties = [("x".to_string(), Prop::U32(5))];
    let seven = format_of(&library, "7z");
    library
        .create_archive(seven, 3, &properties, &mut sink)
        .unwrap();
    assert_eq!(sink.thread, Some(std::thread::current().id()));
    assert_eq!(sink.total, Some(8));
    assert!(sink.results.iter().all(|ok| *ok), "{:?}", sink.results);
    let bytes = sink.out.get_ref().clone();
    let archive = opened(&library, folder.path(), "7z", &bytes);
    let read = contents(&archive);
    assert_eq!(read["top.txt"], b"top");
    assert_eq!(read[r"docs\readme.txt"], b"hello");
    assert!(read.contains_key("docs"));
}

/// An archive written anew: an old item kept, one left out, a new one added; the old data copied by
/// the library.
#[test]
fn an_archive_is_changed() {
    let folder = tempfile::tempdir().unwrap();
    let library = Library::find(folder.path());
    let seven = format_of(&library, "7z");
    let mut first = Sink {
        items: vec![new("a.txt", Some(b"aaa")), new("b.txt", Some(b"bbb"))],
        ..Sink::default()
    };
    library.create_archive(seven, 2, &[], &mut first).unwrap();
    let archive = opened(&library, folder.path(), "7z", first.out.get_ref());
    let kept = (0..2)
        .find(|&index| archive.property(index, prop::PATH) == Prop::Text("a.txt".into()))
        .unwrap();
    let mut second = Sink {
        items: vec![Ok(kept), new("c.txt", Some(b"ccc"))],
        ..Sink::default()
    };
    archive.update(2, &[], &mut second).unwrap();
    drop(archive);
    let bytes = second.out.get_ref().clone();
    let changed = opened(&library, folder.path(), "7z", &bytes);
    let read = contents(&changed);
    assert_eq!(read.keys().collect::<Vec<_>>(), ["a.txt", "c.txt"]);
    assert_eq!(read["a.txt"], b"aaa");
    assert_eq!(read["c.txt"], b"ccc");
}

/// Encrypted with its headers (`he`): the names need the password to be read; the data with it.
#[test]
fn an_encrypted_7z_is_made() {
    let folder = tempfile::tempdir().unwrap();
    let library = Library::find(folder.path());
    let seven = format_of(&library, "7z");
    let mut sink = Sink {
        items: vec![new("secret.txt", Some(b"hidden"))],
        password: Some("secret".to_string()),
        ..Sink::default()
    };
    let properties = [("he".to_string(), Prop::Bool(true))];
    library
        .create_archive(seven, 1, &properties, &mut sink)
        .unwrap();
    let bytes = sink.out.get_ref().clone();
    assert!(
        !bytes.windows(6).any(|window| window == b"hidden"),
        "the data is encrypted"
    );
    let archive = opened(&library, folder.path(), "7z", &bytes);
    assert_eq!(contents(&archive)["secret.txt"], b"hidden");
}

/// A gzip of one file and a tar of two: other formats through the same calls.
#[test]
fn other_formats_are_made() {
    let folder = tempfile::tempdir().unwrap();
    let library = Library::find(folder.path());
    let mut tar = Sink {
        items: vec![new("one.txt", Some(b"1")), new("two.txt", Some(b"22"))],
        ..Sink::default()
    };
    let tar_format = format_of(&library, "tar");
    library
        .create_archive(tar_format, 2, &[], &mut tar)
        .unwrap();
    let archive = opened(&library, folder.path(), "tar", tar.out.get_ref());
    assert_eq!(contents(&archive)["two.txt"], b"22");
    let mut gzip = Sink {
        items: vec![new("one.txt", Some(b"gzipped"))],
        ..Sink::default()
    };
    let gzip_format = format_of(&library, "gzip");
    let level = [("x".to_string(), Prop::U32(9))];
    library
        .create_archive(gzip_format, 1, &level, &mut gzip)
        .unwrap();
    let mut data = Vec::new();
    let archive = opened(&library, folder.path(), "gzip", gzip.out.get_ref());
    for (_, bytes) in contents(&archive) {
        data = bytes;
    }
    assert_eq!(data, b"gzipped");
}

/// The plugin stopping at an item's data stops the call; a property the format does not know is a
/// failure the library tells.
#[test]
fn a_stop_and_a_wrong_property() {
    let folder = tempfile::tempdir().unwrap();
    let library = Library::find(folder.path());
    let seven = format_of(&library, "7z");
    let mut sink = Sink {
        items: vec![new("a.txt", Some(b"aaa"))],
        stop_at_stream: true,
        ..Sink::default()
    };
    let stopped = library.create_archive(seven, 1, &[], &mut sink);
    assert_eq!(stopped.unwrap_err().kind(), io::ErrorKind::Interrupted);
    let mut sink = Sink {
        items: vec![new("a.txt", Some(b"aaa"))],
        ..Sink::default()
    };
    let wrong = [("no-such-thing".to_string(), Prop::U32(1))];
    assert!(library.create_archive(seven, 1, &wrong, &mut sink).is_err());
}

/// The object's interfaces: the callback (`IProgress`, `IArchiveUpdateCallback`), the password's
/// (`ICryptoGetTextPassword2`), the output stream (`ISequentialOutStream`, `IOutStream`); a call
/// after the update refused.
#[test]
fn the_updater_has_its_interfaces() {
    let updater = Updater::new();
    let (callback, out) = updater.pointers();
    for iid in [IID_UNKNOWN, IID_PROGRESS, IID_UPDATE_CALLBACK] {
        assert_eq!(updater.com.query(&iid).unwrap().raw(), callback);
    }
    for iid in [IID_SEQUENTIAL_OUT_STREAM, IID_OUT_STREAM] {
        assert_eq!(updater.com.query(&iid).unwrap().raw(), out);
    }
    let password = updater.com.query(&IID_CRYPTO_GET_TEXT_PASSWORD2).unwrap();
    assert_ne!(password.raw(), callback);
    assert!(updater.com.query(&IID_SET_PROPERTIES).is_none());
    // SAFETY: the callback's own table, as the library calls it, after no update call.
    let after = unsafe { (updater.com.table::<CallbackTable>().set_total)(callback, 1) };
    assert_eq!(after, E_ABORT);
    let mut read = [0u8; 1];
    assert!(std::io::empty().read(&mut read).is_ok());
}
