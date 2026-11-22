//! "Use filter": what Copy's filter leaves out stays where it is (FAR's `COPY_FILTERED`), a folder with
//! all in it; a move walks its folders instead of renaming them whole, and a folder that keeps what was
//! left out stays; the totals count what goes.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use super::tests::{Script, request};
use super::*;
use crate::vfs::FileMask;
use crate::vfs::filter::{AreaFilter, Conditions, Mark, Marked};

/// `tree\keep.rs`, `tree\drop.txt`, `tree\sub\deep.rs`, `tree\skip\inner.rs`.
fn tree(root: &Path) -> std::path::PathBuf {
    let tree = root.join("tree");
    fs::create_dir_all(tree.join("sub")).unwrap();
    fs::create_dir_all(tree.join("skip")).unwrap();
    fs::write(tree.join("keep.rs"), "k").unwrap();
    fs::write(tree.join("drop.txt"), "dd").unwrap();
    fs::write(tree.join("sub").join("deep.rs"), "eee").unwrap();
    fs::write(tree.join("skip").join("inner.rs"), "iiii").unwrap();
    tree
}

/// Leaves out `*.txt` and the folder `skip`.
fn filter() -> (AreaFilter, SystemTime) {
    let excluding = |mask: &str| Marked {
        mark: Mark::Exclude,
        conditions: Conditions {
            mask: FileMask::parse(mask),
            ..Conditions::default()
        },
    };
    let filter = AreaFilter {
        filters: vec![excluding("*.txt"), excluding("skip")],
        ..AreaFilter::default()
    };
    (filter, SystemTime::now())
}

fn filtered(sources: &[std::path::PathBuf], target: &Path, move_: bool) -> Request {
    Request {
        filter: Some(filter()),
        ..request(sources, target, move_, Existing::Ask)
    }
}

#[test]
fn a_copy_leaves_out_what_the_filter_does_and_a_folder_with_all_in_it() {
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let mut host = Script::default();
    let report = run(
        &filtered(std::slice::from_ref(&tree), &target, false),
        &mut host,
    );
    assert_eq!(report, Report::default(), "nothing failed");
    let copied = target.join("tree");
    assert!(copied.join("keep.rs").exists() && copied.join("sub").join("deep.rs").exists());
    assert!(!copied.join("drop.txt").exists());
    assert!(!copied.join("skip").exists(), "the folder is not made");
    assert!(tree.join("drop.txt").exists(), "the source stays");
}

#[test]
fn a_move_walks_its_folders_and_one_that_keeps_what_was_left_out_stays() {
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let mut host = Script::default();
    let report = run(
        &filtered(std::slice::from_ref(&tree), &target, true),
        &mut host,
    );
    assert_eq!(
        report,
        Report::default(),
        "a folder left with files is no error"
    );
    let moved = target.join("tree");
    assert!(moved.join("keep.rs").exists() && moved.join("sub").join("deep.rs").exists());
    assert!(
        !tree.join("keep.rs").exists() && !tree.join("sub").exists(),
        "moved"
    );
    assert!(tree.join("drop.txt").exists() && tree.join("skip").join("inner.rs").exists());
    assert!(
        tree.is_dir(),
        "the folder keeps what was left out and stays"
    );
}

#[test]
fn a_selected_item_the_filter_leaves_out_is_not_finished() {
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let sources = [tree.join("drop.txt"), tree.join("keep.rs")];
    let mut host = Script::default();
    let report = run(&filtered(&sources, &target, false), &mut host);
    assert_eq!(report, Report::default());
    assert!(!target.join("drop.txt").exists() && target.join("keep.rs").exists());
    let finished = &host.progress.finished;
    assert_eq!(
        finished,
        &[crate::vfs::VfsPath::local(tree.join("keep.rs"))],
        "its selection stays"
    );
}

#[test]
fn the_totals_count_only_what_goes() {
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let mut job = Job::start(filtered(std::slice::from_ref(&tree), &target, false), None);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(job.finished().is_some() && job.progress().totals_known) {
        assert!(Instant::now() < deadline, "the job did not end");
        std::thread::sleep(Duration::from_millis(5));
    }
    let progress = job.progress();
    assert_eq!(
        (progress.files_total, progress.bytes_total),
        (2, 4),
        "keep.rs and sub\\deep.rs"
    );
}

#[test]
fn a_moved_subfolder_that_keeps_what_was_left_out_stays() {
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    fs::write(tree.join("sub").join("x.txt"), "x").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let mut host = Script::default();
    let report = run(
        &filtered(std::slice::from_ref(&tree), &target, true),
        &mut host,
    );
    assert_eq!(report, Report::default());
    assert!(target.join("tree").join("sub").join("deep.rs").exists());
    assert!(!tree.join("sub").join("deep.rs").exists(), "moved");
    assert!(
        tree.join("sub").join("x.txt").exists(),
        "left out, its folder kept"
    );
}

#[test]
fn a_selected_item_the_filter_leaves_out_is_not_moved() {
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let sources = [
        tree.join("drop.txt"),
        tree.join("skip"),
        tree.join("keep.rs"),
    ];
    let mut host = Script::default();
    let report = run(&filtered(&sources, &target, true), &mut host);
    assert_eq!(report, Report::default());
    assert!(tree.join("drop.txt").exists() && tree.join("skip").join("inner.rs").exists());
    assert!(!target.join("drop.txt").exists() && !target.join("skip").exists());
    assert!(target.join("keep.rs").exists() && !tree.join("keep.rs").exists());
}

#[test]
fn a_retry_of_a_skipped_folder_keeps_the_filter() {
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let destination = crate::vfs::VfsPath::local(target.join("tree"));
    let source = crate::vfs::VfsPath::local(&tree);
    let retry = Request {
        retrying: vec![Skipped {
            source: source.clone(),
            destination,
            top: source,
            inside_link: false,
        }],
        ..filtered(std::slice::from_ref(&tree), &target, false)
    };
    let report = run(&retry, &mut Script::default());
    assert_eq!(report, Report::default());
    assert!(target.join("tree").join("keep.rs").exists());
    assert!(
        !target.join("tree").join("drop.txt").exists(),
        "still left out"
    );
    assert!(!target.join("tree").join("skip").exists());
}

#[cfg(windows)]
#[test]
fn hard_links_walk_through_the_filter_and_a_junction_is_made_whatever_it_says() {
    use crate::platform::files::NewLink;
    let temp = tempfile::tempdir().unwrap();
    let tree = tree(temp.path());
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let hard = Request {
        link: Some(NewLink::Hard),
        ..filtered(std::slice::from_ref(&tree), &target, false)
    };
    assert_eq!(run(&hard, &mut Script::default()), Report::default());
    assert!(target.join("tree").join("keep.rs").exists());
    assert!(!target.join("tree").join("drop.txt").exists());
    let skip = tree.join("skip");
    let junction = Request {
        link: Some(NewLink::Junction),
        ..filtered(std::slice::from_ref(&skip), &target, false)
    };
    assert_eq!(run(&junction, &mut Script::default()), Report::default());
    assert!(
        fs::symlink_metadata(target.join("skip")).is_ok(),
        "FAR makes the link before any filter"
    );
}
