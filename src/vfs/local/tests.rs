//! What the local provider does for copy and move, on names Windows would take for others.

use std::fs;
use std::io::{ErrorKind, Read, Write};

use super::*;
use crate::vfs::provider_for;

fn in_dir(temp: &tempfile::TempDir, name: &str) -> VfsPath {
    VfsPath::local(temp.path()).join(name)
}

fn on_disk(temp: &tempfile::TempDir, name: &str) -> PathBuf {
    platform::files::exact_path(&temp.path().join(name))
}

fn read(path: &VfsPath) -> Vec<u8> {
    let mut bytes = Vec::new();
    LocalProvider
        .open_read(path)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

fn write(path: &VfsPath, mode: WriteMode, bytes: &[u8]) -> io::Result<()> {
    let mut output = LocalProvider.open_write(path, mode)?;
    output.write_all(bytes)?;
    output.flush()
}

#[test]
fn the_local_provider_is_what_a_local_path_finds() {
    assert_eq!(
        provider_for(&VfsPath::local("C:\\x")).unwrap().id(),
        LocalProvider.id()
    );
}

#[test]
fn reading_takes_the_exact_name() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(on_disk(&temp, "a "), "spaced").unwrap();
    fs::write(on_disk(&temp, "a"), "plain").unwrap();

    assert_eq!(read(&in_dir(&temp, "a ")), b"spaced");
    assert_eq!(read(&in_dir(&temp, "a")), b"plain");
}

#[test]
fn a_new_file_never_goes_over_an_existing_one() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(on_disk(&temp, "a"), "old").unwrap();

    let error = write(&in_dir(&temp, "a"), WriteMode::New, b"new").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(on_disk(&temp, "a")).unwrap(), b"old");

    write(&in_dir(&temp, "a."), WriteMode::New, b"dotted").unwrap();
    assert_eq!(fs::read(on_disk(&temp, "a.")).unwrap(), b"dotted");
    assert_eq!(fs::read(on_disk(&temp, "a")).unwrap(), b"old");
}

#[test]
fn appending_writes_after_what_the_file_holds() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(on_disk(&temp, "a"), "old").unwrap();
    write(&in_dir(&temp, "a"), WriteMode::Append, b"+new").unwrap();
    assert_eq!(fs::read(on_disk(&temp, "a")).unwrap(), b"old+new");

    let error = write(&in_dir(&temp, "none"), WriteMode::Append, b"x").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::NotFound);
}

#[test]
fn a_folder_is_made_under_its_exact_name_and_never_again() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(on_disk(&temp, "d")).unwrap();

    LocalProvider.make_dir(&in_dir(&temp, "d ")).unwrap();
    assert!(on_disk(&temp, "d ").is_dir());

    let error = LocalProvider.make_dir(&in_dir(&temp, "d")).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
}

#[test]
fn a_rename_replaces_only_when_told_and_only_the_exact_name() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(on_disk(&temp, "from "), "moved").unwrap();
    fs::write(on_disk(&temp, "from"), "stays").unwrap();
    fs::write(on_disk(&temp, "to"), "old").unwrap();
    let (from, to) = (in_dir(&temp, "from "), in_dir(&temp, "to"));

    let error = LocalProvider.rename(&from, &to, false).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(on_disk(&temp, "to")).unwrap(), b"old");

    LocalProvider.rename(&from, &to, true).unwrap();
    assert_eq!(fs::read(on_disk(&temp, "to")).unwrap(), b"moved");
    assert!(fs::symlink_metadata(on_disk(&temp, "from ")).is_err());
    assert_eq!(fs::read(on_disk(&temp, "from")).unwrap(), b"stays");
}

#[test]
fn a_file_is_read_anywhere_and_its_writer_goes_on_writing() {
    let temp = tempfile::tempdir().unwrap();
    let path = in_dir(&temp, "log.txt");
    write(&path, WriteMode::New, b"0123456789").unwrap();
    let mut file = LocalProvider.open_random(&path).unwrap();
    let mut buf = [0u8; 4];
    assert_eq!(file.read_at(3, &mut buf).unwrap(), 4);
    assert_eq!(&buf, b"3456");
    assert_eq!(file.read_at(8, &mut buf).unwrap(), 2, "short at the end");
    assert_eq!(file.read_at(20, &mut buf).unwrap(), 0, "nothing past it");
    // Open for reading, the file still takes more from its writer.
    write(&path, WriteMode::Append, b"ab").unwrap();
    assert_eq!(file.len().unwrap(), 12);
    assert_eq!(file.read_at(10, &mut buf).unwrap(), 2);
    assert_eq!(&buf[..2], b"ab");
}
