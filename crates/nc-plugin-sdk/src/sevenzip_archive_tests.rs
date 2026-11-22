//! Through 7-Zip's installed library (the development machine has it) and small archives it made:
//! `a.7z` and `a.tar.gz` hold `docs\readme.txt` ("hello") and `top.txt` ("top"), `locked.7z` the same
//! with its headers encrypted by the password "secret", `nested.tar` holds `a.7z`.

use super::*;

const SEVEN: &[u8] = include_bytes!("../testdata/a.7z");
const TAR_GZ: &[u8] = include_bytes!("../testdata/a.tar.gz");
const LOCKED: &[u8] = include_bytes!("../testdata/locked.7z");
const NESTED: &[u8] = include_bytes!("../testdata/nested.tar");

fn library() -> Library {
    Library::find(tempfile::tempdir().unwrap().path())
}

fn format(library: &Library, name: &str) -> usize {
    (library.formats().iter())
        .position(|format| format.name == name)
        .unwrap()
}

/// The archive's bytes as a file of a folder of the test's.
fn file(bytes: &[u8], name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    (folder, path)
}

#[derive(Default)]
struct Asker {
    password: Option<String>,
    asked: usize,
}

impl Opening for Asker {
    fn password(&mut self) -> Option<String> {
        self.asked += 1;
        self.password.clone()
    }
}

fn paths(archive: &Archive) -> Vec<String> {
    let mut paths: Vec<String> = (0..archive.count())
        .map(|index| {
            archive
                .property(index, prop::PATH)
                .text()
                .unwrap_or("")
                .to_string()
        })
        .collect();
    paths.sort();
    paths
}

/// A 7z opened: its items' paths, folders, sizes and times; the properties it has; the archive's own.
#[test]
fn a_7z_is_opened_and_its_items_read() {
    let library = library();
    let (_folder, path) = file(SEVEN, "a.7z");
    let stream = InStream::file(&path).unwrap();
    let archive = (library.open(
        format(&library, "7z"),
        &stream,
        Some(&path),
        false,
        &mut Asker::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(paths(&archive), ["docs", r"docs\readme.txt", "top.txt"]);
    let index = (0..archive.count())
        .find(|&index| archive.property(index, prop::PATH).text() == Some("top.txt"))
        .unwrap();
    assert_eq!(archive.property(index, prop::IS_DIR), Prop::Bool(false));
    assert_eq!(archive.property(index, prop::SIZE).number(), Some(3));
    assert!(matches!(archive.property(index, prop::MODIFIED), Prop::Time(time) if time > 0));
    assert_eq!(archive.property(index, 0xFFFF), Prop::Empty);
    assert!(
        archive
            .properties()
            .iter()
            .any(|info| info.id == prop::PATH)
    );
    assert!(!archive.archive_properties().is_empty());
    assert_eq!(
        archive.archive_property(prop::PHYSICAL_SIZE).number(),
        Some(SEVEN.len() as u64)
    );
    assert!(archive.volumes().is_empty());
    assert_eq!(archive.main_file(), None);
}

/// A file of another format is not one: `None`, not an error.
#[test]
fn another_format_is_not_opened() {
    let library = library();
    let (_folder, path) = file(TAR_GZ, "a.tar.gz");
    let stream = InStream::file(&path).unwrap();
    let opened = library.open(
        format(&library, "7z"),
        &stream,
        Some(&path),
        false,
        &mut Asker::default(),
    );
    assert!(opened.unwrap().is_none());
}

/// A tar.gz: 7-Zip's gzip names no main file and gives no stream that seeks (arclite shows its one item,
/// the tar, and opens no chain through it).
#[test]
fn a_tar_gz_shows_its_tar() {
    let library = library();
    let (_folder, path) = file(TAR_GZ, "a.tar.gz");
    let stream = InStream::file(&path).unwrap();
    let gzip = (library.open(
        format(&library, "gzip"),
        &stream,
        Some(&path),
        false,
        &mut Asker::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(gzip.main_file(), None);
    assert_eq!(paths(&gzip), ["a.tar"]);
    assert!(gzip.stream(0).is_none());
}

/// An item's stream opens as an archive of its own (arclite's `get_stream`): the 7z a tar holds.
#[test]
fn an_items_stream_opens_as_an_archive() {
    let library = library();
    let (_folder, path) = file(NESTED, "nested.tar");
    let stream = InStream::file(&path).unwrap();
    let tar = (library.open(
        format(&library, "tar"),
        &stream,
        Some(&path),
        false,
        &mut Asker::default(),
    ))
    .unwrap()
    .unwrap();
    let inner = tar.stream(0).unwrap();
    let seven = (library.open(
        format(&library, "7z"),
        &inner,
        None,
        false,
        &mut Asker::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(paths(&seven), ["docs", r"docs\readme.txt", "top.txt"]);
    assert!(tar.stream(tar.count()).is_none());
}

/// Encrypted headers ask the password once; the right one opens; none given stops the opening.
#[test]
fn encrypted_headers_ask_the_password() {
    let library = library();
    let (_folder, path) = file(LOCKED, "locked.7z");
    let seven = format(&library, "7z");
    let mut asker = Asker {
        password: Some("secret".to_string()),
        asked: 0,
    };
    let stream = InStream::file(&path).unwrap();
    let archive = library
        .open(seven, &stream, Some(&path), false, &mut asker)
        .unwrap()
        .unwrap();
    assert_eq!(asker.asked, 1);
    assert_eq!(paths(&archive).len(), 3);
    let stream = InStream::file(&path).unwrap();
    let refused = library.open(seven, &stream, Some(&path), false, &mut Asker::default());
    assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::Interrupted);
}

/// The quick look: gzip's header is one, a method gzip has not is not, its signature alone asks for
/// more; a format with no look (7z) or none there says yes.
#[test]
fn signatures_are_looked_at() {
    let library = library();
    let gzip = format(&library, "gzip");
    assert_eq!(library.is_arc(gzip, TAR_GZ), IsArc::Yes);
    assert_eq!(
        library.is_arc(gzip, b"\x1F\x8B\x09\x00xxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
        IsArc::No
    );
    assert_eq!(library.is_arc(gzip, b"\x1F\x8B"), IsArc::NeedMore);
    assert_eq!(library.is_arc(format(&library, "7z"), b"PK"), IsArc::Yes);
    assert_eq!(library.is_arc(usize::MAX, b""), IsArc::Yes);
}

/// `get_skip_header`: an archive after a stub is found and its size told.
#[test]
fn an_archive_after_a_stub_is_measured() {
    let library = library();
    let mut bytes = b"stub of an SFX".to_vec();
    bytes.extend_from_slice(SEVEN);
    let (_folder, path) = file(&bytes, "sfx.exe");
    let stream = InStream::file(&path).unwrap();
    let seven = format(&library, "7z");
    assert_eq!(
        library.physical_size(seven, &stream, 1 << 20),
        Some(SEVEN.len() as u64)
    );
    let sub = stream.from_offset(14).unwrap();
    let archive = library
        .open(seven, &sub, None, false, &mut Asker::default())
        .unwrap();
    assert_eq!(paths(&archive.unwrap()).len(), 3);
}

/// A class id of the wrong length is no `GUID`; a format not there cannot be made.
#[test]
fn class_ids_are_checked() {
    assert!(guid(&[1, 2, 3]).is_err());
    let id = guid(&[1, 0, 0, 0, 2, 0, 3, 0, 4, 5, 6, 7, 8, 9, 10, 11]).unwrap();
    assert_eq!((id.data1, id.data2, id.data3), (1, 2, 3));
    assert_eq!(id.data4, [4, 5, 6, 7, 8, 9, 10, 11]);
    assert!(library().create(usize::MAX).is_err());
}
