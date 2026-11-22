//! A directory read in one pass: every entry with its sizes, times and link tag.
//! (Before the one pass these were the tests of `directory_details`, the extra pass beside `std::fs`; the
//! same checks now hold of the records the one pass gives, plus what `std::fs` gave before.)

use std::collections::HashMap;

use super::*;

/// The records of `dir` by name.
fn records(dir: &Path, cancel: bool) -> io::Result<HashMap<OsString, DirectoryRecord>> {
    let mut found = HashMap::new();
    read_directory(dir, &AtomicBool::new(cancel), &mut |record| {
        found.insert(record.name.clone(), record);
    })?;
    Ok(found)
}

#[test]
fn reports_allocated_sizes_of_files_and_folders() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("big.bin"), vec![7u8; 100_000]).unwrap();
    std::fs::write(temp.path().join("файл.txt"), b"x").unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();

    let sizes = records(temp.path(), false).unwrap();

    assert_eq!(sizes.len(), 3, "{sizes:?}");
    let big = &sizes[&OsString::from("big.bin")];
    assert!(big.allocated.is_some_and(|allocated| allocated >= 100_000));
    assert!(big.changed.is_some_and(|time| time.elapsed().is_ok()));
    assert!(sizes.contains_key(&OsString::from("файл.txt")));
    assert!(!sizes.contains_key(&OsString::from("..")));
}

#[test]
fn reports_what_std_reported_of_each_entry() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("big.bin"), vec![7u8; 100_000]).unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();

    let found = records(temp.path(), false).unwrap();

    for (name, record) in &found {
        let metadata = std::fs::symlink_metadata(temp.path().join(name)).unwrap();
        assert_eq!(
            record.attributes.is_directory(),
            metadata.is_dir(),
            "{name:?}"
        );
        if !metadata.is_dir() {
            assert_eq!(record.size, metadata.len(), "{name:?}");
        }
        assert_eq!(record.modified, metadata.modified().ok(), "{name:?}");
        assert_eq!(record.created, metadata.created().ok(), "{name:?}");
    }
}

#[test]
fn reports_the_tags_of_links() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("target")).unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J", "junction", "target"])
        .current_dir(temp.path())
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    let details = records(temp.path(), false).unwrap();

    assert_eq!(
        details[&OsString::from("junction")].link,
        Some(LinkKind::Junction)
    );
    assert_eq!(details[&OsString::from("target")].link, None);
}

#[test]
fn many_entries_span_several_buffers() {
    let temp = tempfile::tempdir().unwrap();
    for index in 0..1500 {
        let name = format!("a-rather-long-file-name-to-fill-the-buffer-{index:04}.txt");
        std::fs::write(temp.path().join(name), b"").unwrap();
    }
    assert_eq!(records(temp.path(), false).unwrap().len(), 1500);
}

#[test]
fn a_set_cancel_stops_the_pass() {
    let temp = tempfile::tempdir().unwrap();
    let error = records(temp.path(), true).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn a_missing_directory_is_an_error() {
    let temp = tempfile::tempdir().unwrap();
    assert!(records(&temp.path().join("missing"), false).is_err());
}
