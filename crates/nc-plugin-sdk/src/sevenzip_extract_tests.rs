//! Through 7-Zip's installed library and the archives of `testdata`: `a.7z` (`docs\readme.txt` "hello",
//! `top.txt` "top"), `data_locked.7z` (the same, its data encrypted by "secret", its headers not).

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::super::{Archive, InStream, Library, Opening, prop};
use super::*;

const SEVEN: &[u8] = include_bytes!("../testdata/a.7z");
const DATA_LOCKED: &[u8] = include_bytes!("../testdata/data_locked.7z");

struct Quiet;
impl Opening for Quiet {}

fn opened(bytes: &[u8]) -> (tempfile::TempDir, Archive) {
    let folder = tempfile::tempdir().unwrap();
    let path: PathBuf = folder.path().join("a.7z");
    std::fs::write(&path, bytes).unwrap();
    let library = Library::find(folder.path());
    let seven = (library.formats().iter())
        .position(|format| format.name == "7z")
        .unwrap();
    let stream = InStream::file(&path).unwrap();
    let archive = (library.open(seven, &stream, Some(&path), false, &mut Quiet))
        .unwrap()
        .unwrap();
    (folder, archive)
}

/// What the library told, item by item.
#[derive(Default)]
struct Sink {
    total: Option<u64>,
    items: Vec<(u32, AskMode)>,
    data: BTreeMap<u32, Vec<u8>>,
    outcomes: Vec<Outcome>,
    current: Option<u32>,
    password: Option<String>,
    asked: usize,
    /// Stop at this item's start.
    stop_at: Option<u32>,
    thread: Option<std::thread::ThreadId>,
}

impl Sink {
    fn on_thread(&mut self) {
        let here = std::thread::current().id();
        assert_eq!(*self.thread.get_or_insert(here), here, "one thread");
    }
}

impl Extracting for Sink {
    fn set_total(&mut self, total: u64) -> bool {
        self.total = Some(total);
        true
    }

    fn item(&mut self, index: u32, mode: AskMode) -> Option<bool> {
        self.on_thread();
        if self.stop_at == Some(index) {
            return None;
        }
        self.items.push((index, mode));
        self.current = Some(index);
        Some(mode == AskMode::Extract)
    }

    fn write(&mut self, data: &[u8]) -> bool {
        self.on_thread();
        let index = self.current.unwrap();
        self.data.entry(index).or_default().extend_from_slice(data);
        true
    }

    fn done(&mut self, outcome: Outcome) -> bool {
        self.outcomes.push(outcome);
        true
    }

    fn password(&mut self) -> Option<String> {
        self.asked += 1;
        self.password.clone()
    }
}

fn file_indices(archive: &Archive) -> BTreeMap<String, u32> {
    (0..archive.count())
        .filter(|&index| archive.property(index, prop::IS_DIR) != super::super::Prop::Bool(true))
        .map(|index| {
            let path = archive.property(index, prop::PATH);
            (path.text().unwrap().to_string(), index)
        })
        .collect()
}

/// The files extracted in one pass: each comes to be written, its data whole, its outcome good; the
/// total told first; every call on the caller's thread.
#[test]
fn a_7zs_files_are_extracted() {
    let (_folder, archive) = opened(SEVEN);
    let files = file_indices(&archive);
    let indices: Vec<u32> = files.values().copied().collect();
    let mut sink = Sink::default();
    archive.extract(&indices, false, &mut sink).unwrap();
    assert_eq!(sink.total, Some(8));
    assert_eq!(sink.data[&files[r"docs\readme.txt"]], b"hello");
    assert_eq!(sink.data[&files["top.txt"]], b"top");
    assert_eq!(sink.outcomes, [Outcome::Ok, Outcome::Ok]);
    assert!(sink.items.iter().all(|(_, mode)| *mode == AskMode::Extract));
    assert_eq!(sink.thread, Some(std::thread::current().id()));
}

/// A folder of more than one coder (a BCJ or BCJ2 filter before LZMA, 7-Zip's for programs; its mixer
/// runs the coders on threads of its own): every call still on the caller's thread, the data whole.
#[test]
fn a_filtered_folders_calls_come_on_the_callers_thread() {
    for bytes in [
        &include_bytes!("../testdata/bcj.7z")[..],
        include_bytes!("../testdata/bcj2.7z"),
    ] {
        let (_folder, archive) = opened(bytes);
        let mut sink = Sink::default();
        archive.extract(&[0], false, &mut sink).unwrap();
        assert_eq!(sink.thread, Some(std::thread::current().id()));
        assert_eq!(sink.outcomes, [Outcome::Ok]);
        assert_eq!(sink.data[&0].len(), 64 * 16_384);
    }
}

/// Testing: each item comes to be tested, no data asked for, the outcomes good.
#[test]
fn a_7z_is_tested() {
    let (_folder, archive) = opened(SEVEN);
    let indices: Vec<u32> = file_indices(&archive).values().copied().collect();
    let mut sink = Sink::default();
    archive.extract(&indices, true, &mut sink).unwrap();
    assert!(sink.items.iter().all(|(_, mode)| *mode == AskMode::Test));
    assert!(sink.data.is_empty());
    assert_eq!(sink.outcomes, [Outcome::Ok, Outcome::Ok]);
}

/// Encrypted data: the password asked, the right one gives the data; a wrong one a data or CRC error;
/// none stops the call.
#[test]
fn encrypted_data_asks_the_password() {
    let (_folder, archive) = opened(DATA_LOCKED);
    let files = file_indices(&archive);
    let indices: Vec<u32> = files.values().copied().collect();
    let mut right = Sink {
        password: Some("secret".to_string()),
        ..Sink::default()
    };
    archive.extract(&indices, false, &mut right).unwrap();
    assert!(right.asked >= 1);
    assert_eq!(right.data[&files["top.txt"]], b"top");
    let mut wrong = Sink {
        password: Some("nope".to_string()),
        ..Sink::default()
    };
    archive.extract(&indices, false, &mut wrong).unwrap();
    assert!(
        (wrong.outcomes.iter()).all(|outcome| matches!(
            outcome,
            Outcome::DataError | Outcome::CrcError | Outcome::WrongPassword
        )),
        "{:?}",
        wrong.outcomes
    );
    let refused = archive.extract(&indices, false, &mut Sink::default());
    assert_eq!(refused.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
}

/// The plugin stopping at an item stops the call.
#[test]
fn a_stop_ends_the_extraction() {
    let (_folder, archive) = opened(SEVEN);
    let files = file_indices(&archive);
    let indices: Vec<u32> = files.values().copied().collect();
    let mut sink = Sink {
        stop_at: Some(indices[1]),
        ..Sink::default()
    };
    let stopped = archive.extract(&indices, false, &mut sink);
    assert_eq!(stopped.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
    assert_eq!(sink.items.len(), 1);
}

/// The callback's interfaces: `IProgress`, `IArchiveExtractCallback` and `ICryptoGetTextPassword`, no
/// other; a call after the extract call is refused.
#[test]
fn the_callback_has_its_interfaces() {
    let extractor = Extractor::new();
    for iid in [IID_UNKNOWN, IID_PROGRESS, IID_EXTRACT_CALLBACK] {
        assert_eq!(extractor.com.query(&iid).unwrap().raw(), extractor.raw());
    }
    let password = extractor.com.query(&IID_CRYPTO_GET_TEXT_PASSWORD).unwrap();
    assert_ne!(password.raw(), extractor.raw());
    assert_eq!(password.query(&IID_UNKNOWN).unwrap().raw(), extractor.raw());
    assert!(extractor.com.query(&IID_SEQUENTIAL_OUT_STREAM).is_none());
    // SAFETY: the callback's own table, as the library calls it, after no extract call.
    let after = unsafe { (extractor.com.table::<CallbackTable>().set_total)(extractor.raw(), 1) };
    assert_eq!(after, E_ABORT);
}

/// Damaged data: the item's CRC error told by its outcome (the call itself goes on: 7-Zip's), the
/// other item good; a damaged first byte a data error for both.
#[test]
fn damage_is_told_by_the_outcomes() {
    let outcomes = |at: usize| {
        let mut bytes = SEVEN.to_vec();
        bytes[at] ^= 0x55;
        let (_folder, archive) = opened(&bytes);
        let indices: Vec<u32> = file_indices(&archive).values().copied().collect();
        let mut sink = Sink::default();
        archive.extract(&indices, true, &mut sink).unwrap();
        sink.outcomes
    };
    assert_eq!(outcomes(36), [Outcome::CrcError, Outcome::Ok]);
    assert_eq!(outcomes(41), [Outcome::Ok, Outcome::CrcError]);
    assert_eq!(outcomes(32), [Outcome::DataError, Outcome::DataError]);
}

/// Extracting only the second item of a solid block: the first comes too, to be passed over.
#[test]
fn an_item_before_the_one_asked_is_passed_over() {
    let (_folder, archive) = opened(SEVEN);
    let files = file_indices(&archive);
    let top = files["top.txt"];
    let readme = files[r"docs\readme.txt"];
    let mut sink = Sink::default();
    archive.extract(&[top], false, &mut sink).unwrap();
    assert_eq!(
        sink.items,
        [(readme, AskMode::Skip), (top, AskMode::Extract)],
        "{:?}",
        sink.items
    );
    assert_eq!(sink.data.keys().copied().collect::<Vec<_>>(), [top]);
    assert_eq!(sink.data[&top], b"top");
}

/// The plugin's answers to the callbacks called directly, as the library calls them.
#[derive(Default)]
struct Answers {
    /// False from every call that can stop.
    stop: bool,
    /// The answer to `item`.
    want: Option<bool>,
    password: Option<String>,
    calls: Vec<String>,
    outcomes: Vec<Outcome>,
    written: Vec<u8>,
}

impl Extracting for Answers {
    fn set_total(&mut self, total: u64) -> bool {
        self.calls.push(format!("total {total}"));
        !self.stop
    }

    fn set_completed(&mut self, done: u64) -> bool {
        self.calls.push(format!("completed {done}"));
        !self.stop
    }

    fn item(&mut self, index: u32, mode: AskMode) -> Option<bool> {
        self.calls.push(format!("item {index} {mode:?}"));
        self.want
    }

    fn write(&mut self, data: &[u8]) -> bool {
        self.written.extend_from_slice(data);
        !self.stop
    }

    fn done(&mut self, outcome: Outcome) -> bool {
        self.outcomes.push(outcome);
        !self.stop
    }

    fn password(&mut self) -> Option<String> {
        self.calls.push("password".to_string());
        self.password.clone()
    }
}

/// The callback's table from its pointer.
fn callbacks(raw: Raw) -> &'static CallbackTable {
    // SAFETY: the callback object of `Extractor::new` starts with its static table's pointer.
    unsafe { &**raw.cast::<*const CallbackTable>() }
}

/// `calls` given the callback's pointer on a thread of the extract call's, `answers` answering.
fn within<T: Send>(answers: &mut Answers, calls: impl FnOnce(Raw) -> T + Send) -> T {
    let extractor = Extractor::new();
    let raw = extractor.raw() as usize;
    extractor.with(answers, move || calls(raw as Raw))
}

/// Each of 7-Zip's operation results is the outcome of its name; any other code its own.
#[test]
fn the_results_are_7zips_codes() {
    let mut answers = Answers::default();
    let results = within(&mut answers, |raw| {
        let table = callbacks(raw);
        (-1..=10)
            // SAFETY: the callback's own function on its live object.
            .map(|code| unsafe { (table.result)(raw, code) })
            .collect::<Vec<_>>()
    });
    assert!(results.iter().all(|result| *result == S_OK), "{results:?}");
    assert_eq!(
        answers.outcomes,
        [
            Outcome::Other(-1),
            Outcome::Ok,
            Outcome::UnsupportedMethod,
            Outcome::DataError,
            Outcome::CrcError,
            Outcome::Unavailable,
            Outcome::UnexpectedEnd,
            Outcome::DataAfterEnd,
            Outcome::IsNotArc,
            Outcome::HeadersError,
            Outcome::WrongPassword,
            Outcome::Other(10),
        ]
    );
}

/// The progress and the outcome go to the plugin, its answer back: true goes on, false aborts; a
/// progress told by no value asks nothing.
#[test]
fn the_plugins_answers_go_back() {
    for stop in [false, true] {
        let mut answers = Answers {
            stop,
            ..Answers::default()
        };
        let results = within(&mut answers, |raw| {
            let table = callbacks(raw);
            let done = 3u64;
            // SAFETY: the callback's own functions on its live object, a value of the test's own.
            unsafe {
                [
                    (table.set_total)(raw, 5),
                    (table.set_completed)(raw, &raw const done),
                    (table.set_completed)(raw, std::ptr::null()),
                    (table.prepare)(raw, 0),
                    (table.result)(raw, 0),
                ]
            }
        });
        let refused = if stop { E_ABORT } else { S_OK };
        assert_eq!(results, [refused, refused, S_OK, S_OK, refused], "{stop}");
        assert_eq!(answers.calls, ["total 5", "completed 3"], "{stop}");
        assert_eq!(answers.outcomes, [Outcome::Ok]);
    }
}

/// An item comes with 7-Zip's ask mode (extract, test, any other skip); no stream for an item not
/// wanted, none and an abort when the plugin stops, a failure with no place for the stream.
#[test]
fn an_item_is_asked_by_its_mode() {
    let mut answers = Answers {
        want: Some(false),
        ..Answers::default()
    };
    let results = within(&mut answers, |raw| {
        let table = callbacks(raw);
        let mut results = Vec::new();
        for mode in [0, 1, 2, 7] {
            let mut out: Raw = std::ptr::dangling_mut();
            // SAFETY: the callback's own function on its live object, the answer's place the test's.
            let result = unsafe { (table.stream)(raw, 4, &raw mut out, mode) };
            results.push((result, out.is_null()));
        }
        // SAFETY: as above, with no place for the answer.
        let no_place = unsafe { (table.stream)(raw, 5, std::ptr::null_mut(), 0) };
        (results, no_place)
    });
    assert_eq!(results.0, [(S_OK, true); 4]);
    assert_eq!(results.1, E_FAIL);
    assert_eq!(
        answers.calls,
        [
            "item 4 Extract",
            "item 4 Test",
            "item 4 Skip",
            "item 4 Skip"
        ]
    );
    let mut stopping = Answers::default();
    let stopped = within(&mut stopping, |raw| {
        let mut out: Raw = std::ptr::dangling_mut();
        // SAFETY: as above.
        let result = unsafe { (callbacks(raw).stream)(raw, 1, &raw mut out, 0) };
        (result, out.is_null())
    });
    assert_eq!(stopped, (E_ABORT, true));
}

/// A wanted item's stream: `ISequentialOutStream` only; its writes go to the plugin, the count written
/// told only when the plugin took them (none for nothing, a failure for no data); refused after the
/// call.
#[test]
fn a_wanted_items_stream_writes_to_the_plugin() {
    let mut answers = Answers {
        want: Some(true),
        ..Answers::default()
    };
    let (stream, results) = within(&mut answers, |raw| {
        let mut out: Raw = std::ptr::null_mut();
        // SAFETY: the callback's own function on its live object, the answer's place the test's.
        let given = unsafe { (callbacks(raw).stream)(raw, 0, &raw mut out, 0) };
        assert_eq!(given, S_OK);
        let stream = Com::take(out).unwrap();
        assert_eq!(
            stream.query(&IID_SEQUENTIAL_OUT_STREAM).unwrap().raw(),
            stream.raw()
        );
        assert_eq!(stream.query(&IID_UNKNOWN).unwrap().raw(), stream.raw());
        assert!(stream.query(&IID_PROGRESS).is_none());
        // SAFETY: the stream's own table.
        let write = unsafe { stream.table::<OutTable>() }.write;
        let mut results = Vec::new();
        let mut count = |data: *const u8, size: u32| {
            let mut done = 99u32;
            // SAFETY: the stream's own function, `size` bytes of the test's at `data`.
            let result = unsafe { write(stream.raw(), data.cast(), size, &raw mut done) };
            results.push((result, done));
        };
        count(b"abc".as_ptr(), 3);
        count(std::ptr::null(), 0);
        count(std::ptr::null(), 2);
        // SAFETY: as above, with no place for the count.
        let uncounted =
            unsafe { write(stream.raw(), b"de".as_ptr().cast(), 2, std::ptr::null_mut()) };
        results.push((uncounted, 0));
        let raw = stream.raw() as usize;
        std::mem::forget(stream);
        (raw, results)
    });
    assert_eq!(results, [(S_OK, 3), (S_OK, 0), (E_FAIL, 0), (S_OK, 0)]);
    assert_eq!(answers.written, b"abcde");
    let stream = Com::take(stream as Raw).unwrap();
    let mut done = 99u32;
    // SAFETY: the stream's own function and table, the stream still held.
    let after = unsafe {
        (stream.table::<OutTable>().write)(stream.raw(), b"f".as_ptr().cast(), 1, &raw mut done)
    };
    assert_eq!((after, done), (E_ABORT, 0));
    let mut refusing = Answers {
        want: Some(true),
        stop: true,
        ..Answers::default()
    };
    let refused = within(&mut refusing, |raw| {
        let mut out: Raw = std::ptr::null_mut();
        // SAFETY: as above.
        unsafe { (callbacks(raw).stream)(raw, 0, &raw mut out, 0) };
        let stream = Com::take(out).unwrap();
        let mut done = 99u32;
        // SAFETY: as above.
        let result = unsafe {
            (stream.table::<OutTable>().write)(stream.raw(), b"g".as_ptr().cast(), 1, &raw mut done)
        };
        (result, done)
    });
    assert_eq!(refused, (E_ABORT, 0));
    assert_eq!(refusing.written, b"g");
}

/// The password interface gives the plugin's password as a `BSTR`; none aborts; no place fails.
#[test]
fn the_password_is_the_plugins() {
    let ask = |password: Option<&str>| {
        let mut answers = Answers {
            password: password.map(str::to_string),
            ..Answers::default()
        };
        let answered = within(&mut answers, |raw| {
            // SAFETY: the password interface's place inside the live callback object.
            let this: Raw =
                unsafe { raw.cast::<u8>().add(offset_of!(CallbackObject, password)) }.cast();
            // SAFETY: that interface starts with its static table's pointer.
            let table = unsafe { &**this.cast::<*const PasswordTable>() };
            let mut text: BSTR = std::ptr::null_mut();
            // SAFETY: the interface's own function, the answer's place the test's.
            let result = unsafe { (table.password)(this, &raw mut text) };
            // SAFETY: as above, no place.
            let no_place = unsafe { (table.password)(this, std::ptr::null_mut()) };
            (result, super::super::com::take_bstr(text), no_place)
        });
        (answered, answers.calls.len())
    };
    assert_eq!(
        ask(Some("pässwörd")),
        ((S_OK, Some("pässwörd".to_string()), E_FAIL), 1)
    );
    assert_eq!(ask(None), ((E_ABORT, None, E_FAIL), 1));
}
