use super::*;
use crate::test_archives::{opened, zip};

/// `a.zip`: the folder `docs/` with `a.txt` and `sub/b.txt`, the folder `implied` only implied by its
/// `c.txt`, and `top.txt`.
fn archive(temp: &std::path::Path) -> Opened {
    let path = temp.join("a.zip");
    zip(
        &path,
        &[
            ("docs", None),
            ("docs/a.txt", Some(b"docs/a.txt")),
            ("docs/sub/b.txt", Some(b"docs/sub/b.txt")),
            ("implied/c.txt", Some(b"implied/c.txt")),
            ("top.txt", Some(b"top.txt")),
        ],
    );
    opened(&path)
}

fn names_of(opened: &Opened, items: &BTreeSet<usize>) -> Vec<String> {
    (items.iter())
        .map(|&item| opened.items[item].path.join("/"))
        .collect()
}

/// arclite's `enum_deleted_indices`: an entry's item and every item under it, a folder only implied
/// by what is in it; the names whatever their case, ".." and names not there none.
#[test]
fn an_entry_goes_with_all_under_it() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let names = ["DOCS".to_string(), "implied".to_string(), "..".to_string()];
    let nodes = nodes_of(&opened, "/", &names).unwrap();
    assert_eq!(nodes.len(), 2);
    let mut gone = names_of(&opened, &items_under(&opened, &nodes));
    gone.sort();
    assert_eq!(
        gone,
        ["docs", "docs/a.txt", "docs/sub/b.txt", "implied/c.txt"]
    );
    assert!(
        nodes_of(&opened, "/", &["none".to_string()])
            .unwrap()
            .is_empty()
    );
    assert!(nodes_of(&opened, "/missing", &names).unwrap().is_empty());
    let sub = nodes_of(&opened, "/docs", &["a.txt".to_string()]).unwrap();
    let gone = names_of(&opened, &items_under(&opened, &sub));
    assert_eq!(gone, ["docs/a.txt"]);
}

fn index_of(opened: &Opened, path: &[&str]) -> usize {
    (opened.items.iter())
        .position(|item| item.path == path)
        .unwrap()
}

/// arclite's `extracted_indices`: an entry with a file the user skipped somewhere under it is kept
/// whole (the skipped file and its folders taken out of the selected ones), the others go.
#[test]
fn an_entry_with_a_skipped_file_is_kept_whole() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let names = ["docs".to_string(), "top.txt".to_string()];
    let nodes = nodes_of(&opened, "/", &names).unwrap();
    let skipped = BTreeSet::from([index_of(&opened, &["docs", "sub", "b.txt"])]);
    let kept = extracted(&opened, &nodes, &skipped);
    assert_eq!(names_of(&opened, &items_under(&opened, &kept)), ["top.txt"]);
    assert_eq!(extracted(&opened, &nodes, &BTreeSet::new()), nodes);
    let skipped = BTreeSet::from([index_of(&opened, &["top.txt"])]);
    let mut gone = names_of(
        &opened,
        &items_under(&opened, &extracted(&opened, &nodes, &skipped)),
    );
    gone.sort();
    assert_eq!(gone, ["docs", "docs/a.txt", "docs/sub/b.txt"]);
}

/// The extraction's questions answered `answer` each time (never for all), the names asked about kept.
struct Answering {
    answer: Overwrite,
    asked: Vec<String>,
}

impl crate::extract::Asks for Answering {
    fn overwrite(
        &mut self,
        path: &std::path::Path,
        _: &crate::extract::FileInfo,
        _: &crate::extract::FileInfo,
    ) -> Option<(Overwrite, bool)> {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.asked.push(name.into_owned());
        Some((self.answer, false))
    }
    fn password(&mut self) -> Option<String> {
        None
    }
    fn fault(&mut self, _: &crate::extract::Fault, _: bool) -> crate::extract::Choice {
        crate::extract::Choice::Cancel
    }
    fn progress(&mut self, _: &std::path::Path, _: u64, _: u64) {}
    fn stopped(&mut self) -> bool {
        false
    }
}

/// `docs` and `top.txt` extracted where `docs/sub/b.txt` and `top.txt` are already, each answered
/// `answer`: the files the extraction noted as skipped, and what is on the disk after.
fn extracted_over(answer: Overwrite) -> (Opened, Vec<usize>, BTreeSet<usize>, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out");
    std::fs::create_dir_all(out.join("docs").join("sub")).unwrap();
    std::fs::write(out.join("docs").join("sub").join("b.txt"), "old b").unwrap();
    std::fs::write(out.join("top.txt"), "old top").unwrap();
    let settings = crate::extract_options::ExtractSettings::default();
    let mut options =
        crate::extract_options::ExtractOptions::starting(out, GetMode::default(), &settings);
    options.overwrite = Overwrite::Ask;
    let mut asks = Answering {
        answer,
        asked: Vec::new(),
    };
    let names = ["docs".to_string(), "top.txt".to_string()];
    let nodes = nodes_of(&opened, "/", &names).unwrap();
    let (log, skipped) =
        crate::extract::extract_noting(&mut opened, &nodes, &options, &mut asks).unwrap();
    assert!(log.is_empty(), "{log:?}");
    asks.asked.sort();
    assert_eq!(asks.asked, ["b.txt", "top.txt"], "both asked about");
    (opened, nodes, skipped, temp)
}

/// arclite's `skipped_indices`: a file the user answered Skip is noted (and left as it was on the disk);
/// F6 then keeps its top-level entry whole and deletes only the others.
#[test]
fn a_file_skipped_by_the_user_keeps_its_entry_in_the_archive() {
    let (opened, nodes, skipped, temp) = extracted_over(Overwrite::Skip);
    assert_eq!(
        names_of(&opened, &skipped),
        ["docs/sub/b.txt", "top.txt"],
        "both answered Skip"
    );
    let out = temp.path().join("out");
    let b = std::fs::read_to_string(out.join("docs").join("sub").join("b.txt")).unwrap();
    assert_eq!(b, "old b", "skipped: not written");
    assert_eq!(
        std::fs::read_to_string(out.join("docs").join("a.txt")).unwrap(),
        "docs/a.txt",
        "a new file is written"
    );
    assert!(
        extracted(&opened, &nodes, &skipped).is_empty(),
        "an entry with a skipped file under it is kept whole"
    );
}

/// A file overwritten (or new) is not skipped: everything extracted goes.
#[test]
fn a_file_overwritten_is_not_noted_as_skipped() {
    let (opened, nodes, skipped, temp) = extracted_over(Overwrite::Overwrite);
    assert!(skipped.is_empty(), "{:?}", names_of(&opened, &skipped));
    let top = std::fs::read_to_string(temp.path().join("out").join("top.txt")).unwrap();
    assert_eq!(top, "top.txt", "overwritten");
    assert_eq!(extracted(&opened, &nodes, &skipped), nodes);
}

/// `x` and `x/`: the program names the entry only by its name, so neither is deleted (the other might
/// go in its place); a name of one entry is fine.
#[test]
fn a_file_and_a_folder_of_one_name_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("twins.zip");
    zip(
        &path,
        &[
            ("x/y.txt", Some(b"1")),
            ("x", Some(b"1")),
            ("z", Some(b"1")),
        ],
    );
    let opened = opened(&path);
    for names in [vec!["x"], vec!["z", "x"]] {
        let names: Vec<String> = names.into_iter().map(str::to_string).collect();
        let error = nodes_of(&opened, "/", &names).unwrap_err();
        assert!(error.message.contains("both named \"x\""), "{error:?}");
        assert!(refuse_twins(&opened, 0, &names).is_err());
    }
    let nodes = nodes_of(&opened, "/", &["z".to_string()]).unwrap();
    assert_eq!(names_of(&opened, &items_under(&opened, &nodes)), ["z"]);
}

/// F7's name: its parts under the panel's folder, `\` as `/`; "." and ".." refused, a name there already
/// (a folder whatever its case, a file, a folder only implied) refused as existing.
#[test]
fn a_new_folder_s_name_is_checked() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    assert_eq!(
        new_folder(&opened, "/docs", r"new\deep").unwrap(),
        ["docs", "new", "deep"]
    );
    assert_eq!(new_folder(&opened, "/", "fresh").unwrap(), ["fresh"]);
    for name in ["..", "a/../b", r".\c", "x/."] {
        let error = new_folder(&opened, "/", name).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Other, "{name}");
    }
    for (path, name) in [
        ("/", "DOCS"),
        ("/", "top.txt"),
        ("/", "implied"),
        ("/docs", "sub"),
    ] {
        let error = new_folder(&opened, path, name).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Exists, "{path} {name}");
        assert!(error.message.contains("already in the archive"));
    }
}

/// The plan written: the items removed left out, the others whole and in order.
#[test]
fn the_archive_is_written_without_them() {
    let temp = tempfile::tempdir().unwrap();
    let opened = archive(temp.path());
    let nodes = nodes_of(&opened, "/", &["docs".to_string()]).unwrap();
    let plan = Plan {
        removed: items_under(&opened, &nodes),
        ..Plan::default()
    };
    struct Quiet;
    impl crate::update::UpdateAsks for Quiet {
        fn overwrite(
            &mut self,
            _: &std::path::Path,
            _: &crate::extract::FileInfo,
            _: &crate::extract::FileInfo,
        ) -> Option<(Overwrite, bool)> {
            None
        }
        fn fault(&mut self, _: &crate::extract::Fault, _: bool) -> crate::extract::Choice {
            crate::extract::Choice::Cancel
        }
        fn progress(&mut self, _: &crate::update::UpdateAt<'_>) {}
        fn stopped(&mut self) -> bool {
            false
        }
    }
    let mut asks = Quiet;
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let options = UpdateOptions::for_archive(&UpdateSettings::default(), ("", false), false, true);
    let temp_zip = write(&opened, &plan, &options, &mut faults).unwrap();
    let reread = crate::test_archives::opened(&temp_zip);
    let names: Vec<String> = reread
        .items
        .iter()
        .map(|item| item.path.join("/"))
        .collect();
    assert_eq!(names, ["implied/c.txt", "top.txt"]);
}

/// A gzip's one file is not deleted from it (arclite's `delete_files`: "Can't change single file
/// archive format"); a zip's are.
#[test]
fn a_single_file_archive_is_not_deleted_from() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt.gz");
    crate::test_archives::make(&path, "gzip", &[("a.txt", Some(b"alpha"))], (false, None));
    let gzip = opened(&path);
    assert_eq!(gzip.writes.as_deref(), Some("gzip"));
    let refused = crate::updating::deletable(&gzip).unwrap_err();
    assert_eq!(refused.message, crate::updating::SINGLE_FILE);
    assert!(crate::updating::deletable(&archive(temp.path())).is_ok());
}
