//! Rename with a name (FAR's "Rename" dialog and its `Retry`): a free name is written, one that exists
//! is asked about again, a folder or the source itself fails, "Remember choice" names the rest freely.

use std::fs;

use super::tests::{Script, read, request, setup};
use super::*;

fn renamed(path: &std::path::Path, for_all: bool) -> Reply {
    Reply::renamed(VfsPath::local(path), for_all)
}

#[test]
fn the_question_offers_the_free_name_and_a_name_that_exists_is_asked_about_again() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    fs::write(target.join("b.txt"), "taken").unwrap();
    let mut host = Script::answering(&[
        renamed(&target.join("b.txt"), false),
        Reply::once(Answer::Skip),
    ]);
    run(
        &request(&[tree.join("a.txt")], &target, false, Existing::Ask),
        &mut host,
    );
    let [
        Question::Exists { free, .. },
        Question::Exists { target: again, .. },
    ] = &host.asked[..]
    else {
        panic!("{:?}", host.asked);
    };
    assert_eq!(*free, VfsPath::local(target.join("a (2).txt")));
    assert_eq!(again.path, VfsPath::local(target.join("b.txt")));
    assert_eq!(read(&target.join("b.txt")), "taken", "Skip left it");
    assert!(!target.join("a (2).txt").exists());
}

#[test]
fn a_folder_or_the_source_itself_as_the_new_name_fails_and_the_rest_go_on() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    fs::write(target.join("b.txt"), "old b").unwrap();
    fs::create_dir(target.join("folder")).unwrap();
    let mut host = Script::answering(&[
        renamed(&target.join("folder"), false),
        renamed(&tree.join("b.txt"), false),
    ]);
    let report = run(
        &request(
            &[tree.join("a.txt"), tree.join("b.txt")],
            &target,
            false,
            Existing::Ask,
        ),
        &mut host,
    );
    assert_eq!(report.failures.len(), 2, "{:?}", report.failures);
    assert_eq!(
        read(&tree.join("b.txt")),
        "b-new",
        "the source is not written over"
    );
    assert_eq!(
        (read(&target.join("a.txt")), read(&target.join("b.txt"))),
        ("old".to_string(), "old b".to_string())
    );
}

#[test]
fn remember_choice_with_a_name_gives_the_files_after_it_free_names() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    fs::write(target.join("b.txt"), "old b").unwrap();
    let mut host = Script::answering(&[renamed(&target.join("mine.txt"), true)]);
    run(
        &request(
            &[tree.join("a.txt"), tree.join("b.txt")],
            &target,
            false,
            Existing::Ask,
        ),
        &mut host,
    );
    assert_eq!(host.asked.len(), 1);
    assert_eq!(read(&target.join("mine.txt")), "a-new");
    assert_eq!(read(&target.join("b (2).txt")), "b-new");
}

/// R7 review C10: a file with a name of 250 characters is overwritten (its temporary name not made
/// past 255 with it).
#[test]
fn a_file_with_a_long_name_is_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let name = format!("{}.txt", "n".repeat(246));
    fs::write(tree.join(&name), "new").unwrap();
    fs::write(target.join(&name), "old").unwrap();
    let mut host = Script::answering(&[]);
    let sources = [tree.join(&name)];
    let report = run(
        &request(&sources, &target, false, Existing::Overwrite),
        &mut host,
    );
    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(read(&target.join(&name)), "new");
}

/// R7 review D3 (FAR's `strRenamedName`): the report names the one item's new name beside it; not
/// one put into a folder there, one elsewhere, several, or one skipped after an error.
#[test]
fn the_report_names_a_new_name_beside_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let renamed = |sources: &[std::path::PathBuf], to: &std::path::Path, move_| {
        let mut host = Script::answering(&[Reply::once(Answer::Skip)]);
        run(&request(sources, to, move_, Existing::Skip), &mut host).renamed
    };
    let beside = tree.join("z.txt");
    let named = Some(VfsPath::local(&beside));
    assert_eq!(renamed(&[tree.join("a.txt")], &beside, true), named);
    assert_eq!(
        renamed(std::slice::from_ref(&beside), &tree.join("y.txt"), false),
        Some(VfsPath::local(tree.join("y.txt")))
    );
    assert_eq!(
        renamed(&[tree.join("b.txt")], &tree.join("inner"), false),
        None,
        "into a folder"
    );
    assert_eq!(
        renamed(&[tree.join("b.txt")], &target.join("b2.txt"), false),
        None,
        "elsewhere"
    );
    let both = [tree.join("b.txt"), tree.join("y.txt")];
    assert_eq!(renamed(&both, &tree.join("inner"), false), None, "several");
    assert_eq!(
        renamed(&[tree.join("gone.txt")], &tree.join("g.txt"), false),
        None,
        "skipped"
    );
    assert!(!tree.join("g.txt").exists());
}
