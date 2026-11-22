//! Through 7-Zip's installed library: `testdata/a.7z` (`top.txt` "top", `docs\readme.txt` "hello"),
//! `testdata/data_locked.7z` (the same, its data encrypted by "secret"), `testdata/big.7z` (`big.bin`,
//! 20 MiB and a little more, byte `i` is `i % 251`), `testdata/a.txt.bz2` ("bzip2 text " 1000 times,
//! no size told).

use std::path::Path;

/// `big.bin`'s size: 251's pattern whole, past 20 MiB.
const BIG: u64 = 20_971_552;

use nc_plugin_sdk::sevenzip::{InStream, Library, Opening, Prop, prop};

use super::*;

struct NoAsk;
impl Opening for NoAsk {}

fn opened(folder: &Path, (name, format): (&str, &str), bytes: &[u8]) -> Archive {
    let path = folder.join(name);
    std::fs::write(&path, bytes).unwrap();
    let library = Library::find(folder);
    let found = (library.formats().iter())
        .position(|known| known.name == format)
        .unwrap();
    let stream = InStream::file(&path).unwrap();
    library
        .open(found, &stream, Some(&path), false, &mut NoAsk)
        .unwrap()
        .unwrap()
}

fn seven(folder: &Path, bytes: &[u8]) -> Archive {
    opened(folder, ("a.7z", "7z"), bytes)
}

fn index_of(archive: &Archive, name: &str) -> u32 {
    (0..archive.count())
        .find(|&index| archive.property(index, prop::PATH) == Prop::Text(name.to_string()))
        .unwrap()
}

/// Up to `limit` bytes from `offset` through `spool`.
fn piece(spool: &mut Spool, archive: &Archive, (offset, limit): (u64, usize)) -> Vec<u8> {
    let mut buffer = vec![0; limit];
    let read = spool
        .read(archive, (offset, &mut buffer), (0, None))
        .unwrap();
    buffer.truncate(read);
    buffer
}

/// From the start, from inside, at most the limit; past the end nothing; another item its own.
#[test]
fn a_file_is_read_from_an_offset() {
    let temp = tempfile::tempdir().unwrap();
    let archive = seven(temp.path(), include_bytes!("../testdata/a.7z"));
    let readme = index_of(&archive, r"docs\readme.txt");
    let mut spool = Spool::new(readme);
    assert_eq!(spool.item(), readme);
    assert_eq!(piece(&mut spool, &archive, (0, 100)), b"hello");
    assert_eq!(piece(&mut spool, &archive, (1, 3)), b"ell");
    assert_eq!(piece(&mut spool, &archive, (4, 10)), b"o");
    assert!(piece(&mut spool, &archive, (9, 10)).is_empty());
    let mut top = Spool::new(index_of(&archive, "top.txt"));
    assert_eq!(piece(&mut top, &archive, (0, 2)), b"to");
}

/// A file the archive tells no size of is read to its end (bzip2's), as is one told too small.
#[test]
fn a_file_of_no_size_told_is_read_whole() {
    let temp = tempfile::tempdir().unwrap();
    let bytes = include_bytes!("../testdata/a.txt.bz2");
    let archive = opened(temp.path(), ("a.txt.bz2", "bzip2"), bytes);
    let text = b"bzip2 text ".repeat(1000);
    let mut spool = Spool::new(0);
    assert_eq!(piece(&mut spool, &archive, (0, 20_000)), text);
    let mut told = Spool::new(0);
    let mut buffer = vec![0; 20_000];
    let read = told
        .read(&archive, (10_990, &mut buffer), (5, None))
        .unwrap();
    assert_eq!(&buffer[..read], &text[10_990..]);
}

/// A file over 16 MiB read through: each piece right, the part past the memory's kept too.
#[test]
fn a_large_file_is_read_through() {
    let temp = tempfile::tempdir().unwrap();
    let archive = seven(temp.path(), include_bytes!("../testdata/big.7z"));
    let mut spool = Spool::new(0);
    let expected = |offset: u64, count: usize| -> Vec<u8> {
        (0..count as u64)
            .map(|at| ((offset + at) % 251) as u8)
            .collect()
    };
    for offset in [0, 1 << 20, (16 << 20) - 3, 18 << 20, BIG - 10] {
        assert_eq!(
            piece(&mut spool, &archive, (offset, 1000)),
            expected(offset, 1000.min((BIG - offset) as usize)),
            "at {offset}"
        );
    }
    assert_eq!(piece(&mut spool, &archive, (5, 10)), expected(5, 10));
    assert!(piece(&mut spool, &archive, (21 << 20, 10)).is_empty());
}

/// Encrypted data: read with the archive's password; without one (or an empty one), told that a
/// password is needed; a wrong one: its error.
#[test]
fn encrypted_data_needs_the_password() {
    let temp = tempfile::tempdir().unwrap();
    let archive = seven(temp.path(), include_bytes!("../testdata/data_locked.7z"));
    let top = index_of(&archive, "top.txt");
    let attempt = |password: Option<&str>| {
        let mut buffer = vec![0; 10];
        let read = Spool::new(top).read(
            &archive,
            (0, &mut buffer),
            (3, password.map(str::to_string)),
        );
        read.map(|read| buffer[..read].to_vec())
    };
    assert_eq!(attempt(Some("secret")).unwrap(), b"top");
    for none in [None, Some("")] {
        let refused = attempt(none).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{none:?}");
    }
    let wrong = attempt(Some("nope")).unwrap_err();
    assert_eq!(wrong.kind(), io::ErrorKind::InvalidData);
    // A failed extraction keeps nothing it gave: the same spool reads right with the right password.
    let mut spool = Spool::new(top);
    let mut buffer = vec![0; 10];
    let failed = spool.read(&archive, (0, &mut buffer), (3, Some("nope".to_string())));
    assert!(failed.is_err());
    let read = spool
        .read(&archive, (0, &mut buffer), (3, Some("secret".to_string())))
        .unwrap();
    assert_eq!(&buffer[..read], b"top");
}

/// What `store` holds from `offset`, up to `limit` bytes.
fn stored(store: &mut Store, offset: u64, limit: usize) -> Vec<u8> {
    let mut buffer = vec![0; limit];
    let read = store.read_at(offset, &mut buffer).unwrap();
    buffer.truncate(read);
    buffer
}

/// Memory keeps up to 16 MiB exactly; a byte more and all goes into a temporary file, read the same.
#[test]
fn the_memory_keeps_up_to_its_limit() {
    let mut store = Store::Memory(Vec::new());
    let kept = usize::try_from(KEPT).unwrap();
    store.append(&vec![1; kept - 1]).unwrap();
    store.append(&[2]).unwrap();
    assert!(matches!(&store, Store::Memory(held) if held.len() == kept));
    assert_eq!(stored(&mut store, KEPT - 2, 10), [1, 2]);
    store.append(&[3]).unwrap();
    assert!(matches!(store, Store::File(_)), "past the limit");
    assert_eq!(stored(&mut store, KEPT - 2, 10), [1, 2, 3]);
    assert_eq!(stored(&mut store, 0, 3), [1, 1, 1]);
    assert!(stored(&mut store, KEPT + 1, 10).is_empty());
    assert!(stored(&mut store, KEPT + 100, 10).is_empty());
}

/// Cut back, the store goes on from its new end, in memory and in a file; a read in between does not
/// move where the next piece goes.
#[test]
fn a_store_cut_back_goes_on_from_there() {
    let mut memory = Store::Memory(Vec::new());
    memory.append(b"abcdef").unwrap();
    memory.truncate(2).unwrap();
    memory.append(b"xy").unwrap();
    assert_eq!(stored(&mut memory, 0, 10), b"abxy");
    assert!(stored(&mut memory, 9, 10).is_empty());
    let mut file = Store::File(temp_file().unwrap());
    file.append(b"abcdef").unwrap();
    assert_eq!(stored(&mut file, 1, 2), b"bc");
    file.append(b"gh").unwrap();
    assert_eq!(stored(&mut file, 0, 20), b"abcdefgh");
    file.truncate(3).unwrap();
    file.append(b"z").unwrap();
    assert_eq!(stored(&mut file, 0, 20), b"abcz");
}

/// The first read of a file whose size is not told extracts to 1 MiB at least, the next past it to
/// twice as far; one told its size up to 16 MiB of it at once; a small file is whole after one.
#[test]
fn each_extraction_goes_far_enough() {
    let temp = tempfile::tempdir().unwrap();
    let archive = seven(temp.path(), include_bytes!("../testdata/big.7z"));
    let mut spool = Spool::new(0);
    let mut buffer = vec![0; 10];
    spool.read(&archive, (0, &mut buffer), (0, None)).unwrap();
    let first = spool.have;
    assert!((FIRST..2 * FIRST).contains(&first), "{first}");
    assert!(!spool.whole);
    spool
        .read(&archive, (first, &mut buffer), (0, None))
        .unwrap();
    assert!(spool.have >= 2 * first, "{} after {first}", spool.have);
    assert_eq!(
        buffer,
        (first..first + 10)
            .map(|at| (at % 251) as u8)
            .collect::<Vec<_>>()
    );
    let mut told = Spool::new(0);
    told.read(&archive, (0, &mut buffer), (BIG, None)).unwrap();
    assert!((KEPT..BIG).contains(&told.have), "{}", told.have);
    assert_eq!(matches!(told.store, Store::File(_)), told.have > KEPT);
    let other = tempfile::tempdir().unwrap();
    let small = seven(other.path(), include_bytes!("../testdata/a.7z"));
    let mut readme = Spool::new(index_of(&small, r"docs\readme.txt"));
    readme.read(&small, (0, &mut buffer), (0, None)).unwrap();
    assert!(readme.whole);
    assert_eq!(readme.have, 5);
    assert!(matches!(&readme.store, Store::Memory(held) if held == b"hello"));
}

/// `testdata/big.bin.gz`'s file: 3 MiB and 7 bytes, byte `i` is `i % 251` (7-Zip's gzip decoder gives
/// it 32 KiB at a time).
const GZ_SIZE: u64 = 3 * 1024 * 1024 + 7;

/// The pattern's bytes from `offset`.
fn pattern(offset: u64, count: usize) -> Vec<u8> {
    (offset..offset + count as u64)
        .map(|at| (at % 251) as u8)
        .collect()
}

/// A decoder giving small pieces: the first read gathers 1 MiB of them, each read past what is kept
/// twice as far, the last to the end; the data right all the way.
#[test]
fn small_pieces_are_gathered_to_twice_as_far() {
    let temp = tempfile::tempdir().unwrap();
    let bytes = include_bytes!("../testdata/big.bin.gz");
    let archive = opened(temp.path(), ("big.bin.gz", "gzip"), bytes);
    let mut spool = Spool::new(0);
    let mut buffer = vec![0; 10];
    let mut kept = Vec::new();
    let mut offset = 0;
    while !spool.whole {
        let read = spool
            .read(&archive, (offset, &mut buffer), (0, None))
            .unwrap();
        assert_eq!(&buffer[..read], pattern(offset, read), "at {offset}");
        kept.push(spool.have);
        offset = spool.have;
    }
    assert_eq!(kept, [FIRST, 2 * FIRST, GZ_SIZE]);
    let read = spool
        .read(&archive, (FIRST - 5, &mut buffer), (0, None))
        .unwrap();
    assert_eq!(&buffer[..read], pattern(FIRST - 5, 10));
}

/// The window keeps only what is past where the store ends (the piece across it cut there) and asks
/// for more until its target; only the wanted item's data is wanted.
#[test]
fn the_window_keeps_from_where_the_store_ends() {
    let mut store = Store::Memory(b"01234".to_vec());
    let mut window = Window {
        wanted: 3,
        at: 0,
        from: 5,
        target: 12,
        store: &mut store,
        outcome: None,
        password: None,
        failed: None,
    };
    assert_eq!(window.item(3, AskMode::Extract), Some(true));
    assert_eq!(window.item(3, AskMode::Skip), Some(false));
    assert_eq!(window.item(3, AskMode::Test), Some(false));
    assert_eq!(window.item(2, AskMode::Extract), Some(false));
    assert!(window.write(b"012"), "before where the store ends");
    assert!(window.write(b"3456"), "across it");
    assert!(!window.write(b"789ab"), "the target reached");
    assert!(window.failed.is_none());
    assert_eq!(stored(&mut store, 0, 20), b"0123456789ab");
}

/// Stored encrypted data (`testdata/copy_locked.7z`: `top.txt` "top", not compressed, encrypted by
/// "secret"): a wrong password has its garbage written and fails; what it wrote is not kept, so the
/// same file read with the right one is right.
#[test]
fn what_a_failed_extraction_wrote_goes() {
    let temp = tempfile::tempdir().unwrap();
    let archive = seven(temp.path(), include_bytes!("../testdata/copy_locked.7z"));
    let top = index_of(&archive, "top.txt");
    let mut spool = Spool::new(top);
    let mut buffer = vec![0; 10];
    let failed = spool.read(&archive, (0, &mut buffer), (3, Some("nope".to_string())));
    assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::InvalidData);
    let read = spool
        .read(&archive, (0, &mut buffer), (3, Some("secret".to_string())))
        .unwrap();
    assert_eq!(&buffer[..read], b"top");
    assert!(spool.whole);
}

/// The panel's reads through the library ([`crate::opened::Opened::read`]): each file its own data,
/// whichever was read before; the archive's password used.
#[test]
fn the_panels_reads_are_each_files_own() {
    use crate::library_open::{Request, open};
    use crate::opened::Opened;
    let opened_at = |folder: &Path, bytes: &[u8]| {
        let path = folder.join("a.7z");
        std::fs::write(&path, bytes).unwrap();
        let library = Library::find(folder);
        let formats = library.formats();
        let enabled: Vec<usize> = (0..formats.len()).collect();
        let request = Request {
            library: &library,
            formats: &formats,
            enabled: &enabled,
            every: false,
        };
        let chain = open(&request, &path, &mut NoAsk).unwrap().remove(0);
        Opened::from_library(&path, chain, ("7z".to_string(), "a"), None).unwrap()
    };
    let item = |opened: &Opened, name: &str| {
        (opened.items.iter())
            .position(|item| item.path.join("\\") == name)
            .unwrap()
    };
    let read = |opened: &mut Opened, item: usize, offset: u64| {
        let mut buffer = vec![0; 10];
        opened.read(item, offset, &mut buffer).map(|read| {
            buffer.truncate(read);
            buffer
        })
    };
    let temp = tempfile::tempdir().unwrap();
    let mut opened = opened_at(temp.path(), include_bytes!("../testdata/a.7z"));
    let (readme, top) = (item(&opened, r"docs\readme.txt"), item(&opened, "top.txt"));
    assert_eq!(read(&mut opened, readme, 0).unwrap(), b"hello");
    assert_eq!(read(&mut opened, top, 0).unwrap(), b"top");
    assert_eq!(read(&mut opened, readme, 1).unwrap(), b"ello");
    assert_eq!(read(&mut opened, top, 2).unwrap(), b"p");
    let locked = tempfile::tempdir().unwrap();
    let mut opened = opened_at(locked.path(), include_bytes!("../testdata/data_locked.7z"));
    let top = item(&opened, "top.txt");
    let refused = read(&mut opened, top, 0).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
    opened.password = Some("secret".to_string());
    assert_eq!(read(&mut opened, top, 0).unwrap(), b"top");
}
