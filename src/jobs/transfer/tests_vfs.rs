//! Copy and move with an end behind a provider (ADR-0013 §6), and names Windows would take for others.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::tests::Script;
use super::*;
use crate::platform::files::exact_path;
use crate::vfs::test_tree as tree;

/// More than one chunk of the stream.
pub(super) const LONG: usize = 3 * 1024 * 1024;

pub(super) fn over(
    sources: &[&VfsPath],
    target: &VfsPath,
    move_: bool,
    existing: Existing,
) -> Request {
    Request {
        sources: sources.iter().map(|&source| source.clone()).collect(),
        target: target.clone(),
        move_,
        existing,
        ask_read_only: true,
        keep: Keep::default(),
        link: None,
        retrying: Vec::new(),
        filter: None,
    }
}

pub(super) fn copy_into(sources: &[&VfsPath], target: &VfsPath) -> Request {
    over(sources, target, false, Existing::Ask)
}

/// Nothing failed, left out or stopped (a new name beside the source is no trouble).
pub(super) fn clean(report: &Report) {
    let troubles = Report {
        renamed: None,
        ..report.clone()
    };
    assert_eq!(troubles, Report::default());
}

/// A folder with a file in it and in a folder inside.
fn sample(root: &VfsPath) -> VfsPath {
    let folder = tree::dir(&root.join("folder"));
    tree::write(&folder.join("a.txt"), b"top");
    tree::dir(&folder.join("inner"));
    tree::write(&folder.join("inner").join("deep.txt"), b"deep");
    folder
}

#[test]
fn a_folder_goes_from_a_provider_to_a_provider_with_what_it_holds() {
    let root = tree::root("t2t");
    let folder = sample(&root);
    let target = tree::dir(&root.join("target"));
    let mut host = Script::default();

    clean(&run(&copy_into(&[&folder], &target), &mut host));

    let copied = target.join("folder");
    assert_eq!(tree::read(&copied.join("a.txt")), b"top");
    assert_eq!(tree::read(&copied.join("inner").join("deep.txt")), b"deep");
    assert_eq!(
        tree::read(&folder.join("a.txt")),
        b"top",
        "the source stays"
    );
    assert_eq!(host.progress.files_done, 2);
    assert_eq!(host.progress.bytes_done, 7);
}

#[test]
fn a_folder_goes_from_a_provider_to_a_disk() {
    let root = tree::root("t2l");
    let folder = sample(&root);
    let temp = tempfile::tempdir().unwrap();

    let request = copy_into(&[&folder], &VfsPath::local(temp.path()));
    clean(&run(&request, &mut Script::default()));

    let copied = temp.path().join("folder");
    assert_eq!(fs::read(copied.join("a.txt")).unwrap(), b"top");
    assert_eq!(fs::read(copied.join("inner/deep.txt")).unwrap(), b"deep");
}

#[test]
fn a_folder_goes_from_a_disk_to_a_provider() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("folder");
    fs::create_dir_all(folder.join("inner")).unwrap();
    fs::write(folder.join("a.txt"), "top").unwrap();
    fs::write(folder.join("inner/deep.txt"), "deep").unwrap();
    let root = tree::root("l2t");

    let request = copy_into(&[&VfsPath::local(&folder)], &root);
    clean(&run(&request, &mut Script::default()));

    assert_eq!(tree::read(&root.join("folder").join("a.txt")), b"top");
    assert_eq!(
        tree::read(&root.join("folder").join("inner").join("deep.txt")),
        b"deep"
    );
}

#[test]
fn a_move_within_a_provider_is_one_rename() {
    let root = tree::root("rename");
    let folder = sample(&root);
    let target = tree::dir(&root.join("target"));

    let request = over(&[&folder], &target, true, Existing::Ask);
    clean(&run(&request, &mut Script::default()));

    assert!(!tree::exists(&folder));
    assert_eq!(
        tree::read(&target.join("folder").join("a.txt")),
        b"top",
        "what is inside went with it"
    );
    assert!(
        !tree::log("rename")
            .iter()
            .any(|line| line.starts_with("open-")),
        "nothing was copied"
    );
}

#[test]
fn a_move_across_providers_deletes_the_source_only_after_the_copy() {
    let root = tree::root("across");
    let folder = sample(&root);
    let temp = tempfile::tempdir().unwrap();

    let request = over(
        &[&folder],
        &VfsPath::local(temp.path()),
        true,
        Existing::Ask,
    );
    clean(&run(&request, &mut Script::default()));

    assert!(!tree::exists(&folder), "emptied and removed");
    assert_eq!(
        fs::read(temp.path().join("folder").join("a.txt")).unwrap(),
        b"top"
    );
    let log = tree::log("across");
    let read = log
        .iter()
        .position(|line| line == "open-read tree://across/folder/a.txt")
        .unwrap();
    let removed = log
        .iter()
        .position(|line| line == "remove-file tree://across/folder/a.txt")
        .unwrap();
    assert!(read < removed);
}

#[test]
fn a_cancelled_stream_leaves_nothing_under_the_name_and_the_source_in_place() {
    let root = tree::root("cancel");
    let source = tree::write(&root.join("big.bin"), &vec![7; LONG]);
    let temp = tempfile::tempdir().unwrap();
    let target = VfsPath::local(temp.path());
    let mut host = Script {
        // The look at the item, then the first chunk.
        cancel_at: Some(2),
        ..Script::default()
    };

    let report = run(&over(&[&source], &target, true, Existing::Ask), &mut host);

    assert!(report.cancelled);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    assert_eq!(tree::read(&source).len(), LONG);
}

#[test]
fn a_stream_that_fails_is_asked_about_and_leaves_nothing_behind() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("big.bin");
    fs::write(&path, vec![7; LONG]).unwrap();
    let source = VfsPath::local(&path);
    let root = tree::root("full");
    let target = tree::dir(&root.join("target"));
    tree::limit_writes(&target.join("big.bin"), 1024);
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);

    let report = run(&over(&[&source], &target, true, Existing::Ask), &mut host);

    assert_eq!(report.failures.len(), 1);
    assert!(matches!(
        host.asked[..],
        [Question::Error { retry: true, .. }]
    ));
    assert!(!tree::exists(&target.join("big.bin")));
    assert_eq!(
        fs::read(&path).unwrap().len(),
        LONG,
        "a move keeps what it failed on"
    );
}

#[test]
fn retry_writes_the_file_again_from_its_start() {
    let root = tree::root("retry");
    let source = tree::write(&root.join("big.bin"), &vec![7; LONG]);
    let target = tree::dir(&root.join("target"));
    let copy = target.join("big.bin");
    tree::limit_writes(&copy, 2 * 1024 * 1024);
    let lifted = copy.clone();
    let mut host = Script {
        on_ask: Some(Box::new(move || tree::limit_writes(&lifted, usize::MAX))),
        ..Script::answering(&[Reply::once(Answer::Retry)])
    };

    clean(&run(&copy_into(&[&source], &target), &mut host));

    assert_eq!(tree::read(&copy).len(), LONG);
    assert_eq!(
        host.progress.bytes_done, LONG as u64,
        "the retry is not counted twice"
    );
}

#[test]
fn a_provider_that_is_gone_is_a_failure_not_a_crash() {
    let root = tree::root("gone");
    let target = tree::dir(&root.join("target"));
    let missing = VfsPath::parse("nosuch://host/a.txt");
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);

    let report = run(&copy_into(&[&missing], &target), &mut host);

    assert_eq!(report.failures.len(), 1);
}

#[test]
fn an_overwrite_goes_through_a_temporary_name_and_keeps_the_old_file_on_failure() {
    let root = tree::root("over");
    let source = tree::write(&root.join("a.txt"), b"new");
    let target = tree::dir(&root.join("target"));
    let old = tree::write(&target.join("a.txt"), b"old");
    let temp = target.join(".a.txt.nc-tmp");

    tree::limit_writes(&temp, 1);
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);
    let request = over(&[&source], &target, false, Existing::Overwrite);
    let report = run(&request, &mut host);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(tree::read(&old), b"old");
    assert!(!tree::exists(&temp));

    tree::limit_writes(&temp, usize::MAX);
    clean(&run(&request, &mut Script::default()));
    assert_eq!(tree::read(&old), b"new");
    assert!(!tree::exists(&temp), "it took the file's place");
    assert!(
        tree::log("over")
            .iter()
            .any(|line| line == "rename tree://over/target/.a.txt.nc-tmp tree://over/target/a.txt")
    );
}

#[test]
fn an_existing_file_behind_a_provider_is_asked_about_with_its_facts() {
    let root = tree::root("ask");
    let source = tree::write(&root.join("a.txt"), b"longer");
    let target = tree::dir(&root.join("target"));
    let old = tree::write(&target.join("a.txt"), b"old");
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);

    clean(&run(&copy_into(&[&source], &target), &mut host));

    let Question::Exists {
        source: new,
        target: existing,
        append: true,
        ..
    } = &host.asked[0]
    else {
        panic!("{:?}", host.asked);
    };
    assert_eq!((new.size, existing.size), (6, 3));
    assert_eq!(existing.path, old);
    assert_eq!(tree::read(&old), b"old");
}

#[test]
fn rename_finds_a_free_name_behind_a_provider() {
    let root = tree::root("free");
    let source = tree::write(&root.join("a.txt"), b"new");
    let target = tree::dir(&root.join("target"));
    tree::write(&target.join("a.txt"), b"old");
    tree::file(&target.join("a (2).txt"));

    let request = over(&[&source], &target, false, Existing::Rename);
    clean(&run(&request, &mut Script::default()));

    assert_eq!(tree::read(&target.join("a (3).txt")), b"new");
    assert_eq!(tree::read(&target.join("a.txt")), b"old");
}

#[test]
fn appending_behind_a_provider_adds_to_the_end_and_never_offers_to_repeat_a_failure() {
    let root = tree::root("append");
    let source = tree::write(&root.join("a.txt"), b"+new");
    let target = tree::dir(&root.join("target"));
    let old = tree::write(&target.join("a.txt"), b"old");
    let request = over(&[&source], &target, false, Existing::Append);

    clean(&run(&request, &mut Script::default()));
    assert_eq!(tree::read(&old), b"old+new");

    tree::limit_writes(&old, 8);
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);
    let report = run(&request, &mut host);
    assert_eq!(report.failures.len(), 1);
    assert!(matches!(
        host.asked[..],
        [Question::Error { retry: false, .. }]
    ));
}

#[test]
fn a_read_only_file_behind_a_provider_is_asked_about() {
    let root = tree::root("ro");
    let source = tree::write(&root.join("a.txt"), b"new");
    let target = tree::dir(&root.join("target"));
    tree::read_only(&target.join("a.txt"));
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);

    let request = over(&[&source], &target, false, Existing::Overwrite);
    clean(&run(&request, &mut host));

    assert!(matches!(host.asked[..], [Question::ReadOnly { .. }]));
    assert!(tree::read(&target.join("a.txt")).is_empty());
}

#[test]
fn nothing_goes_onto_itself_or_into_itself_behind_a_provider() {
    let root = tree::root("self");
    let folder = sample(&root);
    let file = folder.join("a.txt");

    let report = run(&copy_into(&[&file], &file), &mut Script::default());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(tree::read(&file), b"top");

    let inside = folder.join("inner");
    let report = run(&copy_into(&[&folder], &inside), &mut Script::default());
    assert_eq!(report.failures.len(), 1);
    assert!(!tree::exists(&inside.join("folder")));
}

#[test]
fn several_items_need_a_folder_that_exists_behind_a_provider_too() {
    let root = tree::root("several");
    let one = tree::write(&root.join("1.txt"), b"1");
    let two = tree::write(&root.join("2.txt"), b"2");
    let target = root.join("none");

    let report = run(&copy_into(&[&one, &two], &target), &mut Script::default());

    assert_eq!(report.failures.len(), 1);
    assert!(!tree::exists(&target));
}

#[test]
fn a_link_is_reported_when_it_cannot_go_there() {
    let root = tree::root("link");
    let link = tree::link(&root.join("link"));
    let target = tree::dir(&root.join("target"));
    let mut host = Script::default();

    let report = run(&over(&[&link], &target, true, Existing::Ask), &mut host);

    // Within the provider a move renames it, whatever it is.
    clean(&report);
    assert!(tree::exists(&target.join("link")));

    let temp = tempfile::tempdir().unwrap();
    let moved = target.join("link");
    let request = copy_into(&[&moved], &VfsPath::local(temp.path()));
    let report = run(&request, &mut Script::default());
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("link"), "{report:?}");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn the_totals_count_what_is_behind_a_provider() {
    let root = tree::root("totals-vfs");
    let folder = sample(&root);
    let target = tree::dir(&root.join("target"));

    let mut job = Job::start(copy_into(&[&folder], &target), None);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let report = loop {
        if let Some(report) = job.finished() {
            break report;
        }
        assert!(std::time::Instant::now() < deadline, "the job did not end");
        std::thread::yield_now();
    };
    clean(&report);
    while !job.progress().totals_known {
        assert!(std::time::Instant::now() < deadline, "no totals");
        std::thread::yield_now();
    }
    let progress = job.progress();
    assert_eq!((progress.files_total, progress.bytes_total), (2, 7));
}

// Names Windows drops the end of: every step must take the exact one.

pub(super) fn exact_in(dir: &Path, name: &str) -> PathBuf {
    exact_path(&dir.join(name))
}

pub(super) fn local_in(dir: &Path, name: &str) -> VfsPath {
    VfsPath::local(dir.join(name))
}

/// `from` with its sibling without the last space or dot, and a folder to copy to.
pub(super) fn siblings(from: &str, plain: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    fs::write(exact_in(temp.path(), from), "exact").unwrap();
    fs::write(exact_in(temp.path(), plain), "plain").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    (temp, target)
}

#[test]
fn a_copy_takes_the_exact_name_and_keeps_it() {
    for (from, plain) in [("a ", "a"), ("b.", "b")] {
        let (temp, target) = siblings(from, plain);

        let request = copy_into(&[&local_in(temp.path(), from)], &VfsPath::local(&target));
        clean(&run(&request, &mut Script::default()));

        assert_eq!(fs::read(exact_in(&target, from)).unwrap(), b"exact");
        assert!(
            fs::symlink_metadata(exact_in(&target, plain)).is_err(),
            "{from:?} did not become {plain:?}"
        );
    }
}

#[test]
fn a_move_takes_the_exact_name_and_leaves_the_sibling() {
    let (temp, target) = siblings("a ", "a");

    let request = over(
        &[&local_in(temp.path(), "a ")],
        &VfsPath::local(&target),
        true,
        Existing::Ask,
    );
    clean(&run(&request, &mut Script::default()));

    assert_eq!(fs::read(exact_in(&target, "a ")).unwrap(), b"exact");
    assert!(fs::symlink_metadata(exact_in(temp.path(), "a ")).is_err());
    assert_eq!(fs::read(exact_in(temp.path(), "a")).unwrap(), b"plain");
}

#[test]
fn a_move_across_volumes_deletes_the_exact_source() {
    let (temp, target) = siblings("a ", "a");
    let mut host = Script {
        cross_volume: true,
        ..Script::default()
    };

    let request = over(
        &[&local_in(temp.path(), "a ")],
        &VfsPath::local(&target),
        true,
        Existing::Ask,
    );
    clean(&run(&request, &mut host));

    assert!(fs::symlink_metadata(exact_in(temp.path(), "a ")).is_err());
    assert_eq!(fs::read(exact_in(temp.path(), "a")).unwrap(), b"plain");
}

#[test]
fn an_existing_sibling_is_not_taken_for_the_target() {
    let (temp, target) = siblings("a ", "a");
    fs::write(exact_in(&target, "a"), "keep").unwrap();

    // No question: `a ` does not exist there.
    let request = copy_into(&[&local_in(temp.path(), "a ")], &VfsPath::local(&target));
    clean(&run(&request, &mut Script::default()));

    assert_eq!(fs::read(exact_in(&target, "a ")).unwrap(), b"exact");
    assert_eq!(fs::read(exact_in(&target, "a")).unwrap(), b"keep");
}

#[test]
fn a_file_is_not_its_sibling_when_copied_onto_that_name() {
    let (temp, _) = siblings("a ", "a");
    let mut host = Script::answering(&[Reply::once(Answer::Overwrite)]);

    let source = local_in(temp.path(), "a ");
    clean(&run(
        &copy_into(&[&source], &local_in(temp.path(), "a")),
        &mut host,
    ));

    assert_eq!(
        host.asked.len(),
        1,
        "the other file exists: {:?}",
        host.asked
    );
    assert_eq!(fs::read(exact_in(temp.path(), "a")).unwrap(), b"exact");
    assert_eq!(fs::read(exact_in(temp.path(), "a ")).unwrap(), b"exact");
}

#[test]
fn an_overwrite_replaces_the_exact_name_and_leaves_no_temporary_file() {
    let (temp, target) = siblings("a ", "a");
    fs::write(exact_in(&target, "a "), "old").unwrap();
    fs::write(exact_in(&target, "a"), "keep").unwrap();

    let request = over(
        &[&local_in(temp.path(), "a ")],
        &VfsPath::local(&target),
        false,
        Existing::Overwrite,
    );
    clean(&run(&request, &mut Script::default()));

    assert_eq!(fs::read(exact_in(&target, "a ")).unwrap(), b"exact");
    assert_eq!(fs::read(exact_in(&target, "a")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(&target).unwrap().count(), 2);
}

#[test]
fn a_folder_keeps_its_exact_name_and_what_is_inside_is_found() {
    let temp = tempfile::tempdir().unwrap();
    let folder = exact_in(temp.path(), "d.");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("f "), "inside").unwrap();
    fs::create_dir(exact_in(temp.path(), "d")).unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();

    let request = copy_into(&[&local_in(temp.path(), "d.")], &VfsPath::local(&target));
    clean(&run(&request, &mut Script::default()));

    assert_eq!(
        fs::read(exact_in(&target, "d.").join("f ")).unwrap(),
        b"inside"
    );
    assert!(fs::symlink_metadata(exact_in(&target, "d")).is_err());
}

#[test]
fn an_exact_name_goes_to_a_provider_and_back_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(exact_in(temp.path(), "a "), "exact").unwrap();
    let root = tree::root("exact-names");

    let request = copy_into(&[&local_in(temp.path(), "a ")], &root);
    clean(&run(&request, &mut Script::default()));
    let on_tree = root.join("a ");
    assert_eq!(tree::read(&on_tree), b"exact");

    let back = temp.path().join("back");
    fs::create_dir(&back).unwrap();
    clean(&run(
        &copy_into(&[&on_tree], &VfsPath::local(&back)),
        &mut Script::default(),
    ));
    assert_eq!(fs::read(exact_in(&back, "a ")).unwrap(), b"exact");
}

#[test]
fn a_failed_open_of_the_source_is_asked_about() {
    let root = tree::root("open");
    let source = tree::write(&root.join("a.txt"), b"x");
    let target = tree::dir(&root.join("target"));
    tree::fail(&source, ErrorKind::PermissionDenied, 1);
    let mut host = Script::answering(&[Reply::once(Answer::Retry)]);

    clean(&run(&copy_into(&[&source], &target), &mut host));

    assert_eq!(tree::read(&target.join("a.txt")), b"x");
    assert_eq!(host.asked.len(), 1);
}
