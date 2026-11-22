//! The safety rules with an end behind a provider, and the exact name at each step that could lose it.

use std::fs;
use std::io::ErrorKind;

use super::tests::{Script, make_junction};
use super::tests_vfs::{LONG, clean, copy_into, exact_in, local_in, over, siblings};
use super::*;
use crate::platform;
use crate::vfs::test_tree as tree;

fn skip() -> Script {
    Script::answering(&[Reply::once(Answer::Skip)])
}

#[test]
fn a_name_from_a_provider_never_leaves_the_target_on_a_disk() {
    for bad in ["..\\..\\victim.txt", "C:\\victim.txt", "a:stream", ".."] {
        let session = format!("slip-{}", bad.len());
        let root = tree::root(&session);
        let folder = tree::dir(&root.join("folder"));
        tree::write(&folder.join(bad), b"evil");
        tree::write(&folder.join("good.txt"), b"good");
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("one").join("target");
        fs::create_dir_all(&target).unwrap();

        let request = copy_into(&[&folder], &VfsPath::local(&target));
        let report = run(&request, &mut Script::default());

        assert_eq!(report.failures.len(), 1, "{bad}: {report:?}");
        assert!(!temp.path().join("victim.txt").exists(), "{bad}");
        assert!(!target.join("folder").join("a").exists(), "{bad}");
        assert_eq!(fs::read(target.join("folder/good.txt")).unwrap(), b"good");
    }
}

#[test]
fn a_listing_cannot_climb_out_of_the_target_on_another_provider_either() {
    let root = tree::root("slip-tree");
    let folder = tree::dir(&root.join("folder"));
    tree::write(&folder.join(".."), b"evil");
    tree::write(&folder.join("back\\slash"), b"fine here");
    let target = tree::dir(&root.join("target"));

    let report = run(&copy_into(&[&folder], &target), &mut Script::default());

    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert_eq!(
        tree::read(&target.join("folder").join("back\\slash")),
        b"fine here"
    );
}

#[test]
fn a_selected_object_with_such_a_name_is_refused_too() {
    let root = tree::root("slip-top");
    let source = tree::write(&root.join("..\\victim.txt"), b"evil");
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();

    let report = run(
        &copy_into(&[&source], &VfsPath::local(&target)),
        &mut Script::default(),
    );

    assert_eq!(report.failures.len(), 1);
    assert!(!temp.path().join("victim.txt").exists());
}

#[test]
fn a_moved_folder_stays_with_what_could_not_go() {
    let root = tree::root("slip-move");
    let folder = tree::dir(&root.join("folder"));
    let bad = tree::write(&folder.join("..\\x"), b"evil");
    let temp = tempfile::tempdir().unwrap();

    let request = over(
        &[&folder],
        &VfsPath::local(temp.path()),
        true,
        Existing::Ask,
    );
    let report = run(&request, &mut Script::default());

    assert_eq!(report.failures.len(), 1);
    assert!(tree::exists(&bad));
}

#[test]
fn an_unfinished_file_that_cannot_be_removed_is_said_and_not_retried() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("big.bin");
    fs::write(&path, vec![7; LONG]).unwrap();
    let root = tree::root("stays");
    let copy = root.join("big.bin");
    tree::limit_writes(&copy, 1024);
    tree::fail_call("remove-file", &copy, ErrorKind::ConnectionReset, 1);
    let mut host = skip();

    let request = over(&[&VfsPath::local(&path)], &root, true, Existing::Ask);
    let report = run(&request, &mut host);

    let [Question::Error { retry, message, .. }] = &host.asked[..] else {
        panic!("{:?}", host.asked);
    };
    assert!(!retry, "Retry would run into the file that stayed");
    assert!(message.contains("unfinished file"), "{message}");
    assert_eq!(report.failures.len(), 1);
    assert_eq!(fs::read(&path).unwrap().len(), LONG, "the source stays");
}

#[test]
fn a_cancelled_append_behind_a_provider_says_what_stayed() {
    let root = tree::root("append-cancel");
    let source = tree::write(&root.join("a.txt"), &vec![7; LONG]);
    let target = tree::dir(&root.join("target"));
    let old = tree::write(&target.join("a.txt"), b"old");
    let mut host = Script {
        cancel_at: Some(2),
        ..Script::default()
    };

    let request = over(&[&source], &target, false, Existing::Append);
    let report = run(&request, &mut host);

    assert!(report.cancelled);
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert!(report.failures[0].contains("stays in the file"));
    assert!(tree::read(&old).len() > 3);
}

#[test]
fn when_the_last_rename_of_an_overwrite_fails_a_move_keeps_its_source() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    fs::write(&path, "new").unwrap();
    let root = tree::root("last-step");
    let old = tree::write(&root.join("a.txt"), b"old");
    let taken = root.join(".a.txt.nc-tmp");
    tree::fail_call("rename", &taken, ErrorKind::PermissionDenied, 1);
    let mut host = skip();

    let request = over(&[&VfsPath::local(&path)], &root, true, Existing::Overwrite);
    let report = run(&request, &mut host);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(fs::read(&path).unwrap(), b"new", "nothing else holds it");
    assert_eq!(tree::read(&old), b"old");
    assert!(!tree::exists(&taken), "the temporary file is gone");
}

#[test]
fn a_file_that_appears_just_after_the_look_is_kept() {
    let root = tree::root("appears");
    let source = tree::write(&root.join("a.txt"), b"new");
    let target = tree::dir(&root.join("target"));
    let other = tree::write(&target.join("a.txt"), b"theirs");
    tree::hide(&other);
    let mut host = skip();

    let report = run(&copy_into(&[&source], &target), &mut host);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(tree::read(&other), b"theirs");
    assert!(
        !tree::log("appears")
            .iter()
            .any(|line| line.starts_with("remove-")),
        "it was not ours to remove"
    );
}

#[test]
fn a_move_by_rename_never_replaces_what_appeared_meanwhile() {
    let root = tree::root("no-replace");
    let source = tree::write(&root.join("a.txt"), b"new");
    let target = tree::dir(&root.join("target"));
    let other = tree::write(&target.join("a.txt"), b"theirs");
    tree::hide(&other);
    let mut host = skip();

    let request = over(&[&source], &target, true, Existing::Ask);
    let report = run(&request, &mut host);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(tree::read(&other), b"theirs");
    assert_eq!(tree::read(&source), b"new");
}

/// A folder elsewhere and a folder to copy into, in one temporary folder.
fn with_junction() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    make_junction(&target.join("x"), &elsewhere);
    (temp, target)
}

#[test]
fn a_file_meeting_a_link_of_its_name_is_asked_about() {
    let (temp, target) = with_junction();
    fs::write(temp.path().join("x"), "file").unwrap();
    let mut host = skip();

    let request = copy_into(&[&local_in(temp.path(), "x")], &VfsPath::local(&target));
    clean(&run(&request, &mut host));

    assert!(matches!(host.asked[..], [Question::Exists { .. }]));
}

#[test]
fn a_link_meeting_a_link_of_its_name_is_asked_about() {
    let (temp, target) = with_junction();
    let another = temp.path().join("another");
    fs::create_dir(&another).unwrap();
    make_junction(&temp.path().join("x"), &another);
    let mut host = skip();

    let request = copy_into(&[&local_in(temp.path(), "x")], &VfsPath::local(&target));
    clean(&run(&request, &mut host));

    assert!(matches!(host.asked[..], [Question::Exists { .. }]));
}

#[test]
fn the_read_only_mark_is_cleared_on_the_exact_name() {
    let (temp, target) = siblings("a ", "a");
    fs::write(exact_in(&target, "a "), "old").unwrap();
    platform::files::set_read_only(&exact_in(&target, "a "), true).unwrap();
    fs::write(exact_in(&target, "a"), "keep").unwrap();

    let mut request = over(
        &[&local_in(temp.path(), "a ")],
        &VfsPath::local(&target),
        false,
        Existing::Overwrite,
    );
    request.ask_read_only = false;
    clean(&run(&request, &mut Script::default()));

    assert_eq!(fs::read(exact_in(&target, "a ")).unwrap(), b"exact");
    assert_eq!(fs::read(exact_in(&target, "a")).unwrap(), b"keep");
}

#[test]
fn an_append_goes_onto_the_exact_name() {
    let (temp, target) = siblings("a ", "a");
    fs::write(exact_in(&target, "a "), "old+").unwrap();
    fs::write(exact_in(&target, "a"), "keep").unwrap();

    let request = over(
        &[&local_in(temp.path(), "a ")],
        &VfsPath::local(&target),
        false,
        Existing::Append,
    );
    clean(&run(&request, &mut Script::default()));

    assert_eq!(fs::read(exact_in(&target, "a ")).unwrap(), b"old+exact");
    assert_eq!(fs::read(exact_in(&target, "a")).unwrap(), b"keep");
}

#[test]
fn a_target_folder_is_found_by_its_exact_name() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(exact_in(temp.path(), "d ")).unwrap();
    fs::write(exact_in(temp.path(), "d"), "file").unwrap();
    fs::write(temp.path().join("x"), "copied").unwrap();

    let request = copy_into(&[&local_in(temp.path(), "x")], &local_in(temp.path(), "d "));
    clean(&run(&request, &mut Script::default()));

    assert_eq!(
        fs::read(exact_in(temp.path(), "d ").join("x")).unwrap(),
        b"copied"
    );
    assert_eq!(fs::read(exact_in(temp.path(), "d")).unwrap(), b"file");
}

// A move deletes a source only after its copy is complete: every step that ends in Skip keeps it.

/// A source on a disk, moved behind a provider whose `a.txt` holds "old".
fn moved_to_a_provider(session: &str) -> (tempfile::TempDir, std::path::PathBuf, VfsPath, VfsPath) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    fs::write(&path, "new").unwrap();
    let root = tree::root(session);
    let old = tree::write(&root.join("a.txt"), b"old");
    (temp, path, root, old)
}

fn assert_keeps_source(request: &Request, host: &mut Script, keeps: impl FnOnce() -> bool) {
    let report = run(request, host);
    assert!(keeps(), "the source is gone: {:?}", host.asked);
    assert!(!report.cancelled);
}

#[test]
fn skipping_a_failed_copy_to_the_temporary_name_keeps_the_source() {
    let (_temp, path, root, old) = moved_to_a_provider("skip-temp");
    tree::limit_writes(&root.join(".a.txt.nc-tmp"), 1);
    let request = over(&[&VfsPath::local(&path)], &root, true, Existing::Overwrite);

    assert_keeps_source(&request, &mut skip(), || path.exists());

    assert_eq!(tree::read(&old), b"old");
}

#[test]
fn skipping_a_failed_rename_onto_the_target_keeps_the_source() {
    let root = tree::root("skip-rename");
    let source = tree::write(&root.join("a.txt"), b"new");
    let target = tree::dir(&root.join("target"));
    let old = tree::write(&target.join("a.txt"), b"old");
    tree::fail_call("rename", &source, ErrorKind::PermissionDenied, 1);
    let request = over(&[&source], &target, true, Existing::Overwrite);

    assert_keeps_source(&request, &mut skip(), || tree::exists(&source));

    assert_eq!(tree::read(&old), b"old");
}

#[test]
fn skipping_a_failed_append_keeps_the_source() {
    let (_temp, path, root, old) = moved_to_a_provider("skip-append");
    tree::limit_writes(&old, 1);
    let request = over(&[&VfsPath::local(&path)], &root, true, Existing::Append);

    assert_keeps_source(&request, &mut skip(), || path.exists());
}

/// A file on one disk and its target folder, with a host that says the rename crosses volumes and
/// that `free` bytes are left.
fn across_volumes(
    free: u64,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    Script,
) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.txt");
    fs::write(&path, "new content").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let host = Script {
        cross_volume: true,
        free: Some(free),
        ..skip()
    };
    (temp, path, target, host)
}

#[test]
fn skipping_the_low_space_warning_keeps_the_source() {
    let (_temp, path, target, mut host) = across_volumes(1);
    fs::write(target.join("a.txt"), "old content that is long").unwrap();
    let request = over(
        &[&VfsPath::local(&path)],
        &VfsPath::local(&target),
        true,
        Existing::Overwrite,
    );

    assert_keeps_source(&request, &mut host, || path.exists());

    assert!(matches!(host.asked[..], [Question::LowSpace { .. }]));
}

#[test]
fn skipping_a_new_file_without_room_keeps_the_source() {
    let (_temp, path, target, mut host) = across_volumes(0);
    let request = over(
        &[&VfsPath::local(&path)],
        &VfsPath::local(&target),
        true,
        Existing::Ask,
    );

    assert_keeps_source(&request, &mut host, || path.exists());

    assert!(!target.join("a.txt").exists());
}

#[test]
fn a_name_that_means_a_folder_is_no_name_anywhere() {
    let root = tree::root("names");
    for place in [VfsPath::local("folder"), root] {
        for name in ["", ".", "..", "a/b"] {
            assert!(!place.accepts_name(name), "{place}: {name:?}");
        }
        assert!(place.accepts_name("a "), "{place}");
    }
}

/// The script, its task's cancel flag set (the user cancelled while a folder was listed).
struct Cancelled(Script, std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Host for Cancelled {
    fn ask(&mut self, question: Question) -> Reply {
        self.0.ask(question)
    }

    fn cancelled(&mut self) -> bool {
        self.0.cancelled()
    }

    fn progress(&mut self, change: &mut dyn FnMut(&mut Progress)) {
        self.0.progress(change);
    }

    fn stop_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        std::sync::Arc::clone(&self.1)
    }
}

#[test]
fn a_listing_the_cancel_stopped_ends_the_copy_as_cancelled_with_no_question() {
    // Review of 19-2 (R15): the cancel, not a failure, a skipped item or a question.
    let root = tree::root("copy-listing-cancelled");
    let folder = tree::dir(&root.join("folder"));
    tree::write(&folder.join("a.txt"), b"a");
    tree::fail(&folder, ErrorKind::Interrupted, 1);
    let temp = tempfile::tempdir().unwrap();
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut host = Cancelled(Script::default(), flag);

    let report = run(
        &copy_into(&[&folder], &VfsPath::local(temp.path())),
        &mut host,
    );

    assert!(report.cancelled, "{report:?}");
    assert!(
        report.failures.is_empty() && report.skipped.is_empty(),
        "{report:?}"
    );
    assert!(host.0.asked.is_empty());
}
