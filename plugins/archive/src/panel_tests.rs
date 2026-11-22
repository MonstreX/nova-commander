use std::path::Path;

use super::*;

/// `docs/` named, `docs/a.txt` (10 bytes), `deep/b.txt` (its folder implied), `top.txt` (empty): a zip
/// stored through 7-Zip's library.
fn opened(path: &Path) -> Opened {
    crate::test_archives::zip(
        path,
        &[
            ("docs", None),
            ("docs/a.txt", Some(b"0123456789")),
            ("deep/b.txt", Some(b"b")),
            ("top.txt", Some(b"")),
        ],
    );
    crate::test_archives::opened(path)
}

/// The root's entries: the folders named and implied, the file; a file's sizes; a folder only implied
/// has the archive file's time; a folder not there none.
#[test]
fn a_folders_entries_are_its_items() {
    let temp = tempfile::tempdir().unwrap();
    let opened = opened(&temp.path().join("a.zip"));
    let root = entries(&opened, "/").unwrap();
    let names: Vec<(&str, bool)> = (root.iter())
        .map(|entry| (entry.name.as_str(), entry.is_dir))
        .collect();
    assert_eq!(names, [("docs", true), ("deep", true), ("top.txt", false)]);
    let docs = entries(&opened, "/docs").unwrap();
    assert_eq!(docs[0].name, "a.txt");
    assert_eq!((docs[0].size, docs[0].allocated), (10, Some(10)));
    let deep = &root[1];
    assert_eq!(deep.modified, opened.file.modified, "the archive file's");
    assert_eq!(deep.attributes & DIRECTORY, DIRECTORY);
    assert_eq!(deep.allocated, Some(0));
    assert!(entries(&opened, "/none").is_none());
    assert!(
        entries(&opened, "/top.txt").is_none(),
        "a file is no folder"
    );
}

/// arclite's title: its name, the format, the archive, the folder as FAR writes it.
#[test]
fn the_title_names_the_archive_and_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    let opened = opened(&temp.path().join("a.zip"));
    assert_eq!(title(&opened, "/"), "Archive:zip:a.zip");
    assert_eq!(title(&opened, "/docs/sub"), r"Archive:zip:a.zip:\docs\sub");
}

/// arclite's info lines of a zip (7-Zip's archive properties, then the ratio, the sizes and the
/// counts): the path first, the file and folder counts.
#[test]
fn the_info_lines_are_arclites() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.zip");
    let opened = opened(&path);
    let lines = info_lines(&opened);
    let text: Vec<(&str, &str)> = (lines.iter())
        .map(|line| (line.text.as_str(), line.data.as_str()))
        .collect();
    assert_eq!(text[0], ("Path", path.to_str().unwrap()));
    assert!(text.contains(&("Data size", "11")), "{text:?}");
    assert!(text.contains(&("File count", "3")), "{text:?}");
    assert!(text.contains(&("Directory count", "1")), "{text:?}");
}

/// arclite's `format_data_size`.
#[test]
fn sizes_are_written_as_arclite_writes_them() {
    assert_eq!(size_text(0), "0");
    assert_eq!(size_text(999), "999");
    assert_eq!(size_text(1000), "0.98 KB = 1000");
    assert_eq!(size_text(1536), "1.50 KB = 1536");
    assert_eq!(size_text(10 * 1024), "10 KB = 10240");
    assert_eq!(size_text(1_048_576), "1 MB = 1048576");
    assert_eq!(size_text(1_022_976), "999 KB = 1022976");
    assert_eq!(size_text(1_023_500), "0.98 MB = 1023500", "999.5 KB up");
}
