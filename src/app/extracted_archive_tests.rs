//! A file found in an archive copied for F3 / F4 from the archive opened again (`extract_from_archive`),
//! against stand-ins for the plugins' opener: the place's archive not opened (which one said), a stop
//! before or during the opening, the copy made from the opened archive's session.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::super::extract_from_archive;
use crate::vfs::test_tree as tree;
use crate::vfs::{self, FileOpener, OpenedFile, VfsPath};

/// Opens every file as `root` (none: opens nothing), setting the stop first when it `stops`; counts
/// the files offered.
struct Fake {
    root: Option<VfsPath>,
    stops: bool,
    offered: AtomicUsize,
}

impl FileOpener for Fake {
    fn open(&self, _file: &Path, stop: &AtomicBool) -> Option<OpenedFile> {
        self.offered.fetch_add(1, Ordering::Relaxed);
        if self.stops {
            stop.store(true, Ordering::Relaxed);
        }
        let root = self.root.clone()?;
        Some(OpenedFile::new(root, Box::new(|| {})))
    }
}

fn fake(root: Option<VfsPath>, stops: bool) -> Fake {
    Fake {
        root,
        stops,
        offered: AtomicUsize::new(0),
    }
}

fn offered(opener: &Fake) -> usize {
    opener.offered.load(Ordering::Relaxed)
}

#[test]
fn an_archive_not_opened_is_named_by_its_file() {
    let place = VfsPath::parse(r"archive:D:\a.zip!/docs/x.txt");
    let refusing = fake(None, false);
    let error = extract_from_archive(&refusing, &place, "x.txt").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(error.to_string(), r"Cannot open D:\a.zip");
    assert_eq!(offered(&refusing), 1);
    // No place inside a file: nothing offered, the place itself named.
    let disk = VfsPath::local(r"D:\x.txt");
    let opener = fake(Some(tree::root("extract-arc-disk")), false);
    let error = extract_from_archive(&opener, &disk, "x.txt").unwrap_err();
    assert_eq!(error.to_string(), r"Cannot open D:\x.txt");
    assert_eq!(offered(&opener), 0);
}

#[test]
fn a_stop_before_or_during_the_opening_is_a_stop() {
    let place = VfsPath::parse(r"archive:D:\a.zip!/x.txt");
    let before = fake(Some(tree::root("extract-arc-stop")), false);
    let stopped = Arc::new(AtomicBool::new(true));
    let error = vfs::stop::stopped_by(stopped, || extract_from_archive(&before, &place, "x.txt"))
        .unwrap_err();
    assert!(vfs::stop::is_stopped(&error), "{error}");
    assert_eq!(offered(&before), 0, "not opened once stopped");
    let during = fake(None, true);
    let error = vfs::stop::stopped_by(Arc::default(), || {
        extract_from_archive(&during, &place, "x.txt")
    })
    .unwrap_err();
    assert!(vfs::stop::is_stopped(&error), "{error}");
}

#[test]
fn the_copy_is_made_from_the_archive_opened() {
    let root = tree::root("extract-arc-copy");
    tree::dir(&root.join("docs"));
    tree::write(&root.join("docs").join("x.txt"), b"inside");
    let place = VfsPath::parse(r"tree:D:\a.zip!/docs/x.txt");
    let opener = fake(Some(root), false);
    let (folder, copy) = extract_from_archive(&opener, &place, "x.txt").unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), b"inside");
    assert_eq!(copy.file_name().unwrap(), "x.txt");
    assert!(copy.starts_with(&folder));
    std::fs::remove_dir_all(folder).unwrap();
}
