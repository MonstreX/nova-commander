use super::*;

use std::time::{Duration, UNIX_EPOCH};

use nc_plugin_sdk::sevenzip::{InStream, Library, Opening};

fn gzip() -> Format {
    Format {
        name: "gzip".to_string(),
        extensions: ["gz", "gzip", "tgz", "tpz", "apk"]
            .map(String::from)
            .to_vec(),
        add_extensions: ["*", "*", ".tar", ".tar", "*"].map(String::from).to_vec(),
        ..Format::default()
    }
}

/// arclite's default name: the archive's less its extension, the inner extension put on when the
/// format names one and the name has it not.
#[test]
fn the_default_name_is_arclites() {
    assert_eq!(default_name("a.tar.gz", Some(&gzip())), "a.tar");
    assert_eq!(default_name("a.TGZ", Some(&gzip())), "a.tar");
    assert_eq!(default_name("a.tar.tgz", Some(&gzip())), "a.tar");
    assert_eq!(default_name("b.gz", Some(&gzip())), "b");
    assert_eq!(default_name("noext", Some(&gzip())), "noext");
    assert_eq!(default_name("c.bz2", None), "c");
}

/// The Windows attributes of 7-Zip's: a Unix mode's half and its own bits dropped; a folder always one.
#[test]
fn attributes_are_the_windows_half() {
    assert_eq!(attributes(&Prop::U32(0x20), false), 0x20);
    assert_eq!(attributes(&Prop::U32(0x81A4_8020), false), 0x20);
    assert_eq!(attributes(&Prop::Empty, true), DIRECTORY);
    assert_eq!(attributes(&Prop::U32(1), true), 1 | DIRECTORY);
}

/// Any of the four high bits marks a Unix mode (p7zip's 0x8000, macOS's 0x4000, Info-ZIP's none): its
/// low 14 bits kept; without them every bit is Windows'; a value of another type is none.
#[test]
fn a_unix_mode_is_told_by_any_high_bit() {
    assert_eq!(attributes(&Prop::U32(0x41ED_4010), false), 0x10);
    assert_eq!(attributes(&Prop::U32(0x1000_0021), false), 0x21);
    assert_eq!(attributes(&Prop::U32(0x0FFF_C021), false), 0x0FFF_C021);
    assert_eq!(attributes(&Prop::U32(0x8000_7FFF), false), 0x3FFF);
    assert_eq!(attributes(&Prop::U64(0x20), false), 0);
}

/// The inner extension's case is the name's when it has it already; it replaces the name's own
/// extension otherwise; an extension is after the last dot of the last part.
#[test]
fn the_default_names_inner_extension_replaces_its_own() {
    assert_eq!(default_name("a.TAR.tgz", Some(&gzip())), "a.TAR");
    assert_eq!(default_name("a.x.tgz", Some(&gzip())), "a.tar");
    assert_eq!(default_name("a.x.apk", Some(&gzip())), "a.x");
    assert_eq!(split_extension("a.b/c"), ("a.b/c", ""));
    assert_eq!(split_extension(r"a.b\c.d"), (r"a.b\c", ".d"));
}

struct Quiet;
impl Opening for Quiet {}

fn opened(bytes: &[u8], name: &str, format: &str) -> (tempfile::TempDir, Archive) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    let library = Library::find(folder.path());
    let index = (library.formats().iter())
        .position(|it| it.name == format)
        .unwrap();
    let stream = InStream::file(&path).unwrap();
    let archive = (library.open(index, &stream, Some(&path), false, &mut Quiet))
        .unwrap()
        .unwrap();
    (folder, archive)
}

/// A 7z's items: their paths split, the folder a folder without sizes, the files' sizes and times.
#[test]
fn a_7zs_items_are_read() {
    let (_folder, archive) = opened(include_bytes!("../testdata/a.7z"), "a.7z", "7z");
    let file = FileInfo::default();
    let mut items = items(&archive, "a", &file);
    items.sort_by(|one, other| one.path.cmp(&other.path));
    let paths: Vec<String> = items.iter().map(|item| item.path.join("/")).collect();
    assert_eq!(paths, ["docs", "docs/readme.txt", "top.txt"]);
    assert!(items[0].is_dir && items[0].size == 0);
    assert_eq!(items[0].attributes & DIRECTORY, DIRECTORY);
    assert_eq!((items[1].size, items[2].size), (5, 3));
    // A solid block's packed size on its first file only.
    assert_eq!((items[1].packed, items[2].packed), (12, 0));
    assert_eq!(items[2].attributes, 0x20);
    assert!(!items[1].is_dir && !items[1].encrypted);
    let year_2024 = UNIX_EPOCH + Duration::from_secs(1_704_067_200);
    assert!(items[2].modified.is_some_and(|time| time >= year_2024));
}

/// gzip's one item has no path: the default name; its times the archive file's when it tells none.
#[test]
fn a_nameless_item_takes_the_default_name() {
    let (_folder, archive) = opened(include_bytes!("../testdata/a.tar.gz"), "a.tgz", "gzip");
    let file = FileInfo {
        created: Some(UNIX_EPOCH + Duration::from_secs(1)),
        accessed: Some(UNIX_EPOCH + Duration::from_secs(2)),
        ..FileInfo::default()
    };
    let items = items(&archive, "a.tar", &file);
    assert_eq!(items.len(), 1);
    // 7-Zip names gzip's item by its stored name when it has one.
    assert_eq!(items[0].path, ["a.tar"]);
    assert_eq!(items[0].created, file.created);
    assert_eq!(items[0].accessed, file.accessed);
    assert!(items[0].modified.is_some() && items[0].modified != file.modified);
}

/// The password given.
struct Secret;
impl Opening for Secret {
    fn password(&mut self) -> Option<String> {
        Some("secret".to_string())
    }
}

/// The files of an archive encrypted are; its folder is not.
#[test]
fn encrypted_files_are_told() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("locked.7z");
    std::fs::write(&path, include_bytes!("../testdata/locked.7z")).unwrap();
    let library = Library::find(folder.path());
    let index = (library.formats().iter())
        .position(|it| it.name == "7z")
        .unwrap();
    let stream = InStream::file(&path).unwrap();
    let archive = (library.open(index, &stream, Some(&path), false, &mut Secret))
        .unwrap()
        .unwrap();
    let items = items(&archive, "locked", &FileInfo::default());
    let encrypted: Vec<(String, bool)> = (items.iter())
        .map(|item| (item.path.join("/"), item.encrypted))
        .collect();
    assert!(
        encrypted.contains(&("top.txt".to_string(), true)),
        "{encrypted:?}"
    );
    assert!(
        encrypted.contains(&("docs".to_string(), false)),
        "{encrypted:?}"
    );
}
