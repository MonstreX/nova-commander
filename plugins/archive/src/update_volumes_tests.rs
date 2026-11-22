//! Volumes in a temporary folder.

use std::io::{Seek, SeekFrom};

use super::*;

/// arclite's names: `.001` on, a path ending in `.001` taken as the first's.
#[test]
fn volumes_are_named_as_arclites() {
    let path = Path::new(r"C:\a\b.7z");
    assert_eq!(volume_path(path, 0), Path::new(r"C:\a\b.7z.001"));
    assert_eq!(volume_path(path, 11), Path::new(r"C:\a\b.7z.012"));
    let first = Path::new(r"C:\a\b.7z.001");
    assert_eq!(volume_path(first, 1), Path::new(r"C:\a\b.7z.002"));
    assert_eq!(
        volume_path(Path::new("x.ZIP.001"), 999),
        Path::new("x.ZIP.1000")
    );
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

/// Bytes cut into volumes of the size; a place moved back and written over within them; the size set
/// smaller drops the volumes past it; a cleaning deletes them all.
#[test]
fn volumes_are_written_across() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.7z");
    let mut out = Output::Volumes(Volumes::new(&path, 4).unwrap());
    out.write(b"0123456789").unwrap();
    assert_eq!(read(&volume_path(&path, 0)), b"0123");
    assert_eq!(read(&volume_path(&path, 1)), b"4567");
    assert_eq!(read(&volume_path(&path, 2)), b"89");
    assert_eq!(out.seek(SeekFrom::Start(2)).unwrap(), 2);
    out.write(b"ab").unwrap();
    out.write(b"cd").unwrap();
    assert_eq!(read(&volume_path(&path, 0)), b"01ab");
    assert_eq!(read(&volume_path(&path, 1)), b"cd67");
    assert_eq!(out.seek(SeekFrom::End(0)).unwrap(), 10);
    assert_eq!(out.seek(SeekFrom::Current(-3)).unwrap(), 7);
    out.set_size(5).unwrap();
    assert_eq!(read(&volume_path(&path, 1)), b"c");
    assert!(!volume_path(&path, 2).exists());
    let Output::Volumes(volumes) = out else {
        unreachable!()
    };
    volumes.clean();
    assert!(!volume_path(&path, 0).exists() && !volume_path(&path, 1).exists());
}

/// A volume made whose writing failed before the length grew (a full disk) is cleaned with the others.
#[test]
fn a_volume_made_but_not_written_is_cleaned() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.7z");
    let mut volumes = Volumes::new(&path, 4).unwrap();
    volumes.write(b"0123").unwrap();
    volumes.open(1).unwrap();
    assert!(volume_path(&path, 1).exists());
    volumes.clean();
    assert!(!volume_path(&path, 0).exists());
    assert!(!volume_path(&path, 1).exists(), "the one open too");
}

/// A place sought past the end: the volumes before it made full, the bytes written there.
#[test]
fn a_place_past_the_end_fills_the_volumes_before() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.tar");
    let mut out = Output::Volumes(Volumes::new(&path, 3).unwrap());
    out.seek(SeekFrom::Start(7)).unwrap();
    out.write(b"z").unwrap();
    assert_eq!(std::fs::metadata(volume_path(&path, 1)).unwrap().len(), 3);
    let last = read(&volume_path(&path, 2));
    assert_eq!(last.len(), 2);
    assert_eq!(last[1], b'z');
}

/// One file: written, the place moved, the size set.
#[test]
fn one_file_is_written() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("one");
    let mut out = Output::File(std::fs::File::create(&path).unwrap());
    out.write(b"hello").unwrap();
    out.seek(SeekFrom::Start(0)).unwrap();
    out.write(b"J").unwrap();
    out.set_size(3).unwrap();
    drop(out);
    assert_eq!(read(&path), b"Jel");
    let _ = std::fs::File::open(&path).unwrap().seek(SeekFrom::End(0));
}

/// After a module: the places counted from its end (arclite's `SfxUpdateStream`), a place inside it
/// refused, the size set past it; the module kept.
#[test]
fn a_file_after_a_module_counts_from_its_end() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("sfx");
    std::fs::write(&path, b"MODULE").unwrap();
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.seek(SeekFrom::End(0)).unwrap();
    let mut out = Output::Shifted(file, 6);
    out.write(b"archive").unwrap();
    assert_eq!(out.seek(SeekFrom::Current(0)).unwrap(), 7);
    assert_eq!(out.seek(SeekFrom::Start(0)).unwrap(), 0);
    out.write(b"A").unwrap();
    assert_eq!(out.seek(SeekFrom::End(-2)).unwrap(), 5);
    assert!(
        out.seek(SeekFrom::Current(-6)).is_err(),
        "inside the module"
    );
    out.set_size(4).unwrap();
    drop(out);
    assert_eq!(read(&path), b"MODULEArch");
}

/// A name that is only `.001` keeps it as its base (the dot at the start is no extension's).
#[test]
fn a_name_of_only_the_volume_extension_is_its_base() {
    assert_eq!(volume_path(Path::new(".001"), 0), Path::new(".001.001"));
    assert_eq!(volume_path(Path::new("a.002"), 0), Path::new("a.002.001"));
}

/// The size set to the length there is changes nothing: no volume made past the last full one.
#[test]
fn the_size_set_as_it_is_makes_no_volume() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.7z");
    let mut out = Output::Volumes(Volumes::new(&path, 4).unwrap());
    out.write(b"01234567").unwrap();
    out.set_size(8).unwrap();
    assert!(!volume_path(&path, 2).exists(), "no third volume");
    assert_eq!(read(&volume_path(&path, 1)), b"4567");
}

/// A place before the start, by the current place or the end, is refused; the place stays as it was.
#[test]
fn a_place_before_the_start_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.7z");
    let mut out = Output::Volumes(Volumes::new(&path, 4).unwrap());
    out.write(b"012345").unwrap();
    let before = out.seek(SeekFrom::Current(-7)).unwrap_err();
    assert_eq!(before.kind(), io::ErrorKind::InvalidInput);
    let past = out.seek(SeekFrom::End(-7)).unwrap_err();
    assert_eq!(past.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(out.seek(SeekFrom::Current(0)).unwrap(), 6);
    assert_eq!(out.seek(SeekFrom::End(-6)).unwrap(), 0);
}

/// Volumes of no size are a byte each (the size taken as at least 1).
#[test]
fn volumes_of_no_size_are_a_byte_each() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.7z");
    let mut out = Output::Volumes(Volumes::new(&path, 0).unwrap());
    out.write(b"xyz").unwrap();
    assert_eq!(read(&volume_path(&path, 0)), b"x");
    assert_eq!(read(&volume_path(&path, 2)), b"z");
}

/// The size set past the volume open: the volume it ends in made, its length set within it.
#[test]
fn the_size_set_into_a_later_volume_makes_it() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.7z");
    let mut out = Output::Volumes(Volumes::new(&path, 4).unwrap());
    out.write(b"0123").unwrap();
    out.set_size(6).unwrap();
    assert_eq!(read(&volume_path(&path, 1)), [0, 0]);
    assert_eq!(out.seek(SeekFrom::End(0)).unwrap(), 6);
}
