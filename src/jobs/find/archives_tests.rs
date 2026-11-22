//! "Search in archives" (FAR's `ArchiveSearch`): a disk's files offered to an opener standing in for the
//! plugins, the archives' insides being trees in memory.

use super::tests::{names, request};
use super::*;
use crate::vfs::filter::{Conditions, Mark, Marked};
use crate::vfs::test_tree as tree;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

/// Opens the files it was given as the trees in memory it was given; tells what it was offered.
#[derive(Default)]
struct Opener {
    archives: HashMap<PathBuf, VfsPath>,
    offered: Mutex<Vec<String>>,
    closed: Arc<AtomicUsize>,
}

impl FileOpener for Opener {
    fn open(&self, file: &Path, _stop: &AtomicBool) -> Option<OpenedFile> {
        let name = file.file_name()?.to_string_lossy().into_owned();
        self.offered.lock().unwrap().push(name);
        let root = self.archives.get(file)?.clone();
        let closed = Arc::clone(&self.closed);
        let close = Box::new(move || {
            closed.fetch_add(1, Ordering::Relaxed);
        });
        Some(OpenedFile::new(root, close))
    }
}

/// A disk's folder with `notes.txt`, `a.zip` and `sub\b.zip`; `a.zip` holds `x.txt`, `in\y.txt` and
/// `in\nested.zip`; `b.zip` holds `z.txt`; the opener takes both.
struct Disk {
    temp: tempfile::TempDir,
    opener: Arc<Opener>,
}

fn disk(session: &str) -> Disk {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "hello").unwrap();
    std::fs::write(temp.path().join("a.zip"), "zip").unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("sub").join("b.zip"), "zip").unwrap();
    let a = tree::root(&format!("{session}-a"));
    tree::write(&a.join("x.txt"), b"hello archive");
    tree::dir(&a.join("in"));
    tree::write(&a.join("in").join("y.txt"), b"nothing");
    tree::write(&a.join("in").join("nested.zip"), b"zip");
    let b = tree::root(&format!("{session}-b"));
    tree::write(&b.join("z.txt"), b"hello too");
    let archives = HashMap::from([
        (temp.path().join("a.zip"), a),
        (temp.path().join("sub").join("b.zip"), b),
    ]);
    let opener = Arc::new(Opener {
        archives,
        ..Opener::default()
    });
    Disk { temp, opener }
}

impl Disk {
    fn request(&self, mask: &str) -> Request {
        let mut request = request(&VfsPath::local(self.temp.path()), mask);
        request.archives = Some(Archives(Arc::clone(&self.opener) as Arc<dyn FileOpener>));
        request
    }

    /// Every object found, as `place|name` with the temporary folder as `T`.
    fn run(&self, request: Request) -> Vec<String> {
        let search = Search::start(request);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut found = Vec::new();
        loop {
            let over = search.over();
            found.extend(search.take_found());
            if over {
                break;
            }
            assert!(Instant::now() < deadline, "the search did not end");
            std::thread::sleep(Duration::from_millis(2));
        }
        let temp = self.temp.path().display().to_string();
        (found.iter())
            .map(|item| {
                format!(
                    "{}|{}",
                    item.folder.shown().replace(&temp, "T"),
                    item.entry.name
                )
            })
            .collect()
    }

    fn offered(&self) -> Vec<String> {
        self.opener.offered.lock().unwrap().clone()
    }

    fn closed(&self) -> usize {
        self.opener.closed.load(Ordering::Relaxed)
    }
}

#[test]
fn an_archive_is_searched_after_its_file_and_named_by_its_place_inside() {
    let disk = disk("arc-order");
    let found = disk.run(disk.request("*"));
    let expected = [
        "T|a.zip",
        r"T\a.zip:\|x.txt",
        r"T\a.zip:\|in",
        r"T\a.zip:\in|nested.zip",
        r"T\a.zip:\in|y.txt",
        "T|notes.txt",
        "T|sub",
        r"T\sub|b.zip",
        r"T\sub\b.zip:\|z.txt",
    ];
    assert_eq!(found, expected);
    assert_eq!(
        disk.offered(),
        ["a.zip", "notes.txt", "b.zip"],
        "each disk's file once, never one inside an archive (it has no real name)"
    );
    assert_eq!(disk.closed(), 2, "each archive closed after its walk");
    let first = &found[1];
    assert!(first.starts_with(r"T\a.zip:\"), "{first}");
}

#[test]
fn every_file_is_offered_whatever_the_mask_and_the_mask_takes_names_inside() {
    let disk = disk("arc-mask");
    let found = disk.run(disk.request("*.txt"));
    assert_eq!(
        names(&found),
        ["x.txt", "y.txt", "notes.txt", "z.txt"],
        "in the walk's order: {found:?}"
    );
    assert_eq!(disk.offered(), ["a.zip", "notes.txt", "b.zip"]);
}

#[test]
fn an_archive_is_walked_into_from_the_current_folder_only_too() {
    let disk = disk("arc-current");
    let mut request = disk.request("*.txt");
    request.recurse = false;
    let found = disk.run(request);
    assert_eq!(names(&found), ["x.txt", "y.txt", "notes.txt"], "{found:?}");
}

#[test]
fn the_text_is_looked_for_in_the_archives_files() {
    let disk = disk("arc-text");
    let mut request = disk.request("*");
    request.pattern = Some(text::Pattern::Bytes(b"hello".to_vec()));
    let found = disk.run(request);
    assert_eq!(
        found,
        [r"T\a.zip:\|x.txt", "T|notes.txt", r"T\sub\b.zip:\|z.txt"]
    );
}

#[test]
fn a_file_the_filter_leaves_out_is_not_searched_in() {
    let disk = disk("arc-filter");
    let mut request = disk.request("*");
    let excluding = Marked {
        mark: Mark::Exclude,
        conditions: Conditions {
            mask: FileMask::parse("a.zip"),
            ..Conditions::default()
        },
    };
    let filter = AreaFilter {
        filters: vec![excluding],
        ..AreaFilter::default()
    };
    request.filter = Some((filter, SystemTime::now()));
    let found = disk.run(request);
    assert!(
        !found.iter().any(|line| line.contains("a.zip")),
        "{found:?}"
    );
    assert_eq!(disk.offered(), ["notes.txt", "b.zip"]);
}

/// Holds the search in its `open` until the search is stopped, and tells what the walk's `vfs::stop`
/// (the one a plugin's file read waits by) said then.
#[derive(Default)]
struct Holding {
    entered: AtomicBool,
    seen: Mutex<Option<bool>>,
}

impl FileOpener for Holding {
    fn open(&self, _file: &Path, stop: &AtomicBool) -> Option<OpenedFile> {
        self.entered.store(true, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        let current = crate::vfs::stop::current().load(Ordering::Relaxed);
        *self.seen.lock().unwrap() = Some(current);
        None
    }
}

#[test]
fn a_stop_reaches_the_reads_through_the_plugins() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.zip"), "zip").unwrap();
    let holding = Arc::new(Holding::default());
    let mut request = request(&VfsPath::local(temp.path()), "*");
    request.archives = Some(Archives(Arc::clone(&holding) as Arc<dyn FileOpener>));
    let search = Search::start(request);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !holding.entered.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline, "the file was not offered");
        std::thread::sleep(Duration::from_millis(2));
    }
    search.stop();
    while !search.over() {
        assert!(Instant::now() < deadline, "the search did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        *holding.seen.lock().unwrap(),
        Some(true),
        "the walk's stop is the search's"
    );
}

#[test]
fn a_file_the_search_is_inside_of_is_not_opened_again() {
    // `self.zip` opens as its own folder (a list naming itself): offered once inside it, never opened.
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("self.zip");
    std::fs::write(&file, "list").unwrap();
    let opener = Arc::new(Opener {
        archives: HashMap::from([(file, VfsPath::local(temp.path()))]),
        ..Opener::default()
    });
    let mut request = request(&VfsPath::local(temp.path()), "*");
    request.archives = Some(Archives(Arc::clone(&opener) as Arc<dyn FileOpener>));
    let disk = Disk { temp, opener };
    let found = disk.run(request);
    assert_eq!(found.len(), 2, "on the disk, then inside itself: {found:?}");
    assert_eq!(disk.offered(), ["self.zip"], "opened once");
    assert_eq!(disk.closed(), 1);
}

#[test]
fn without_search_in_archives_nothing_is_offered() {
    let disk = disk("arc-off");
    let mut request = disk.request("*");
    request.archives = None;
    let found = disk.run(request);
    assert_eq!(found.len(), 4, "{found:?}");
    assert!(disk.offered().is_empty());
}

#[path = "archives_walk_tests.rs"]
mod walk;
