//! Making links (Alt+F6) in temporary folders.

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};

use super::tests::{Script, names, read, request};
use super::*;
use crate::platform::files::{NewLink, file_identity, link_target, make_link};

fn linking(sources: &[PathBuf], target: &Path, kind: NewLink) -> Request {
    Request {
        link: Some(kind),
        ..request(sources, target, false, Existing::Ask)
    }
}

fn clean(report: &Report) {
    assert_eq!(report, &Report::default());
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).unwrap().is_symlink()
}

/// `root/tree` with `a.txt` and `inner/b.txt`, an empty `root/target`.
fn tree(root: &Path) -> (PathBuf, PathBuf) {
    let tree = root.join("tree");
    fs::create_dir_all(tree.join("inner")).unwrap();
    fs::write(tree.join("a.txt"), "a").unwrap();
    fs::write(tree.join("inner").join("b.txt"), "b").unwrap();
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    (tree, target)
}

#[test]
fn each_selected_folder_gets_a_junction_in_the_target() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = tree(temp.path());
    let other = temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let mut host = Script::default();

    clean(&run(
        &linking(&[tree.clone(), other.clone()], &target, NewLink::Junction),
        &mut host,
    ));

    assert!(is_link(&target.join("tree")) && is_link(&target.join("other")));
    assert_eq!(link_target(&target.join("tree")).unwrap(), tree);
    assert_eq!(read(&target.join("tree").join("a.txt")), "a");
    assert_eq!(host.progress.files_done, 2);
    assert_eq!(host.progress.finished.len(), 2, "the marks come off");
}

#[test]
fn hard_links_go_as_a_copy_would_each_file_another_name_of_its_own() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = tree(temp.path());

    clean(&run(
        &linking(std::slice::from_ref(&tree), &target, NewLink::Hard),
        &mut Script::default(),
    ));

    let linked = target.join("tree");
    assert!(!is_link(&linked) && linked.is_dir(), "a folder of its own");
    assert_eq!(names(&linked), ["a.txt", "inner"]);
    for name in ["a.txt", "inner/b.txt"] {
        assert_eq!(
            file_identity(&linked.join(name)).unwrap(),
            file_identity(&tree.join(name)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn an_existing_file_is_asked_about_without_append_and_replaced_by_choice() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("a.txt"), "old").unwrap();
    let mut host = Script::answering(&[Reply::once(Answer::Overwrite)]);

    clean(&run(
        &linking(std::slice::from_ref(&source), &target, NewLink::Hard),
        &mut host,
    ));

    assert!(matches!(
        host.asked[..],
        [Question::Exists { append: false, .. }]
    ));
    assert_eq!(
        file_identity(&target.join("a.txt")).unwrap(),
        file_identity(&source).unwrap()
    );
    assert_eq!(names(&target), ["a.txt"], "no temporary file is left");
}

#[test]
fn a_hard_link_to_a_file_that_is_already_the_same_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "x").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::hard_link(&source, target.join("a.txt")).unwrap();

    let report = run(
        &linking(&[source], &target, NewLink::Hard),
        &mut Script::default(),
    );

    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert_eq!(read(&target.join("a.txt")), "x");
}

#[test]
fn a_link_never_replaces_what_exists() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = tree(temp.path());
    fs::create_dir(target.join("tree")).unwrap();
    fs::write(target.join("tree").join("keep.txt"), "k").unwrap();
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);

    let report = run(&linking(&[tree], &target, NewLink::Junction), &mut host);

    assert!(matches!(host.asked[..], [Question::Error { .. }]));
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.skipped.len(), 1, "Retry skipped can make it later");
    assert!(!is_link(&target.join("tree")));
    assert_eq!(read(&target.join("tree").join("keep.txt")), "k");
}

#[test]
fn a_junction_may_stand_inside_its_folder_and_a_hard_link_walk_may_not() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, _) = tree(temp.path());

    clean(&run(
        &linking(std::slice::from_ref(&tree), &tree, NewLink::Junction),
        &mut Script::default(),
    ));
    assert!(is_link(&tree.join("tree")));
    fs::remove_dir(tree.join("tree")).unwrap();

    let report = run(
        &linking(std::slice::from_ref(&tree), &tree, NewLink::Hard),
        &mut Script::default(),
    );
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert_eq!(names(&tree), ["a.txt", "inner"]);
}

/// A symbolic link needs Developer Mode or administrator rights.
#[test]
fn a_symbolic_link_leads_to_the_selected_file_by_its_full_path() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "x").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();

    clean(&run(
        &linking(std::slice::from_ref(&source), &target, NewLink::Symbolic),
        &mut Script::default(),
    ));

    assert!(is_link(&target.join("a.txt")));
    assert_eq!(link_target(&target.join("a.txt")).unwrap(), source);
    assert_eq!(read(&target.join("a.txt")), "x");
}

#[test]
fn a_folder_link_in_a_hard_link_walk_is_made_again_as_a_link() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = tree(temp.path());
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    make_link(NewLink::Junction, &elsewhere, &tree.join("j")).unwrap();

    clean(&run(
        &linking(std::slice::from_ref(&tree), &target, NewLink::Hard),
        &mut Script::default(),
    ));

    let copied = target.join("tree").join("j");
    assert!(is_link(&copied));
    assert_eq!(link_target(&copied).unwrap(), elsewhere);
}

#[test]
fn behind_a_provider_nothing_is_made_and_nothing_is_asked() {
    let temp = tempfile::tempdir().unwrap();
    let root = crate::vfs::test_tree::root("make_links_provider");
    let file = crate::vfs::test_tree::file(&root.join("a.txt"));
    let folder = crate::vfs::test_tree::dir(&root.join("d"));
    crate::vfs::test_tree::file(&folder.join("b.txt"));
    let mut plan = linking(&[], temp.path(), NewLink::Hard);
    plan.sources = vec![file, folder];
    let mut host = Script::default();

    let report = run(&plan, &mut host);

    assert!(host.asked.is_empty(), "{:?}", host.asked);
    assert_eq!(report.failures.len(), 2, "{report:?}");
    assert!(
        report.skipped.is_empty(),
        "no Retry skipped for what can never work"
    );
    assert!(names(temp.path()).is_empty(), "no empty folders are left");
}

#[test]
fn a_hard_link_that_cannot_take_the_old_files_place_leaves_it_and_no_temporary_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("a.txt"), "old").unwrap();
    // Open without FILE_SHARE_DELETE: nothing can take its name while it is held.
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1)
        .open(target.join("a.txt"))
        .unwrap();
    let mut host = Script::answering(&[Reply::once(Answer::Overwrite), Reply::once(Answer::Skip)]);

    let report = run(&linking(&[source], &target, NewLink::Hard), &mut host);
    drop(held);

    assert!(matches!(
        host.asked[..],
        [Question::Exists { .. }, Question::Error { .. }]
    ));
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert_eq!(read(&target.join("a.txt")), "old");
    assert_eq!(names(&target), ["a.txt"], "no temporary file is left");
}
