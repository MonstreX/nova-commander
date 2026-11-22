//! A plugin's files through the real table, on the probe: one entry, reading anywhere (no more than a
//! chunk at once), writing new and appended, folders made, renames, removals, each failure by its kind,
//! and a window the plugin asks for in a panel's call answered Esc at once. Each test names its probe
//! alone: the providers' list is the program's.
#![cfg(windows)]

use std::io::{Read, Write};

use super::ffi::MOST_READ;
use super::*;
use crate::vfs::{Removal, WriteMode};

fn probe(name: &str) -> (tempfile::TempDir, PluginHost) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    install_probe(temp.path(), name, &manifest);
    let host = load(temp.path(), &|_| true);
    assert!(host.failures.is_empty(), "{:?}", host.failures);
    (temp, host)
}

fn kind<T>(result: io::Result<T>) -> io::ErrorKind {
    result.err().map(|error| error.kind()).expect("a failure")
}

#[test]
fn one_entry_and_reading_anywhere_come_through_the_table() {
    let (_temp, _host) = probe("filesone");
    let at = |path: &str| VfsPath::in_session("filesone", "one", path);
    let provider = vfs::provider_for(&at("/")).unwrap();
    let root = provider.stat(&at("/")).unwrap();
    assert!(root.is_dir && root.name.is_empty(), "the root");
    let readme = provider.stat(&at("/readme.txt")).unwrap();
    assert_eq!(
        (readme.name.as_str(), readme.size),
        ("readme.txt", 12),
        "its name copied"
    );
    assert_eq!(kind(provider.stat(&at("/none"))), io::ErrorKind::NotFound);
    assert_eq!(
        kind(provider.stat(&at("/locked"))),
        io::ErrorKind::PermissionDenied
    );
    let mut whole = String::new();
    provider
        .open_read(&at("/readme.txt"))
        .unwrap()
        .read_to_string(&mut whole)
        .unwrap();
    assert_eq!(whole, "Hello probe!");
    let mut file = provider.open_random(&at("/readme.txt")).unwrap();
    let mut buffer = [0; 5];
    assert_eq!(file.read_at(6, &mut buffer).unwrap(), 5);
    assert_eq!(&buffer, b"probe");
    assert_eq!(
        file.read_at(11, &mut buffer).unwrap(),
        1,
        "short at its end"
    );
    assert_eq!(file.read_at(40, &mut buffer).unwrap(), 0, "past it");
    assert_eq!(file.len().unwrap(), 12);
    assert_eq!(
        kind(provider.open_random(&at("/none"))),
        io::ErrorKind::NotFound
    );
}

#[test]
fn a_read_asks_for_no_more_than_a_chunk_however_large_the_buffer() {
    let (_temp, _host) = probe("filestwo");
    let big = VfsPath::in_session("filestwo", "one", "/big");
    let provider = vfs::provider_for(&big).unwrap();
    let data: Vec<u8> = (0..3 * MOST_READ).map(|at| (at % 253) as u8).collect();
    provider
        .open_write(&big, WriteMode::New)
        .unwrap()
        .write_all(&data)
        .unwrap();
    let mut file = provider.open_random(&big).unwrap();
    let mut buffer = vec![0; 3 * MOST_READ];
    let read = file.read_at(1, &mut buffer).unwrap();
    assert_eq!(read, MOST_READ, "one chunk");
    assert_eq!(&buffer[..read], &data[1..=MOST_READ]);
    assert_eq!(file.len().unwrap(), data.len() as u64);
}

#[test]
fn files_are_written_new_and_appended_and_folders_made_renamed_and_removed() {
    let (temp, _host) = probe("filesthree");
    let at = |path: &str| VfsPath::in_session("filesthree", "one", path);
    let provider = vfs::provider_for(&at("/")).unwrap();
    let mut written = provider
        .open_write(&at("/new.txt"), WriteMode::New)
        .unwrap();
    written.write_all(b"one").unwrap();
    let mut more = provider
        .open_write(&at("/new.txt"), WriteMode::Append)
        .unwrap();
    more.write_all(b" two").unwrap();
    // A writer flushed tells the plugin the file is whole, one let go unflushed that it was abandoned
    // (API 9), once each.
    written.flush().unwrap();
    written.flush().unwrap();
    drop((written, more));
    assert_eq!(provider.stat(&at("/new.txt")).unwrap().size, 7);
    let made_over = provider.open_write(&at("/readme.txt"), WriteMode::New);
    assert_eq!(kind(made_over), io::ErrorKind::AlreadyExists);
    provider.make_dir(&at("/made")).unwrap();
    assert_eq!(
        kind(provider.make_dir(&at("/made"))),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        kind(provider.make_dir(&at("/none/x"))),
        io::ErrorKind::NotFound
    );
    let renamed = provider.rename(&at("/new.txt"), &at("/readme.txt"), false);
    assert_eq!(kind(renamed), io::ErrorKind::AlreadyExists);
    provider
        .rename(&at("/new.txt"), &at("/readme.txt"), true)
        .unwrap();
    assert_eq!(
        provider.stat(&at("/readme.txt")).unwrap().size,
        7,
        "replaced"
    );
    let into_itself = provider.rename(&at("/docs"), &at("/docs/deep/docs"), false);
    assert_eq!(kind(into_itself), io::ErrorKind::Other);
    let over_a_folder = provider.rename(&at("/readme.txt"), &at("/made"), true);
    assert_eq!(kind(over_a_folder), io::ErrorKind::AlreadyExists);
    assert_eq!(
        kind(provider.remove(&at("/docs"), Removal::EmptyDir)),
        io::ErrorKind::Other
    );
    provider.remove(&at("/made"), Removal::EmptyDir).unwrap();
    provider.remove(&at("/readme.txt"), Removal::File).unwrap();
    assert_eq!(
        kind(provider.stat(&at("/readme.txt"))),
        io::ErrorKind::NotFound
    );
    let told = std::fs::read_to_string(temp.path().join("Logs").join("filesthree.log")).unwrap();
    assert_eq!(told.matches("ended file one:/new.txt").count(), 1, "{told}");
    assert_eq!(
        told.matches("aborted file one:/new.txt").count(),
        1,
        "{told}"
    );
    for line in [
        "made file one:/new.txt",
        "made folder one:/made",
        "renamed one:/new.txt to /readme.txt",
        "removed one:/made",
        "removed one:/readme.txt",
    ] {
        assert!(told.contains(line), "{line}: {told}");
    }
}

#[test]
fn a_window_asked_for_in_a_panels_call_is_answered_esc_at_once() {
    let (temp, host) = probe("filesfour");
    let folder = VfsPath::in_session("filesfour", "asking", "/asked");
    let provider = vfs::provider_for(&folder).unwrap();
    provider.make_dir(&folder).unwrap();
    assert!(host.take_event().is_none(), "no window for the program");
    let told = std::fs::read_to_string(temp.path().join("Logs").join("filesfour.log")).unwrap();
    assert!(told.contains("asked making /asked: false"), "{told}");
    assert!(
        told.contains("A window asked for in a panel's call is not shown"),
        "{told}"
    );
    // Its commands still show their windows.
    host.run_command(0, 0, 0, 0);
    assert!(
        matches!(next_event(&host), Event::Ask { .. }),
        "a window again"
    );
}
