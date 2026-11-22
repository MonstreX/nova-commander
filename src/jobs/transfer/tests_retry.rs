//! "Retry skipped": only what errors left out, each to its place, and a move ends as if nothing had
//! gone wrong.

use std::fs;

use super::tests::{Script, names, read, request, setup};
use super::*;

/// Claims no room: every file is left out after the question, answered Skip all.
fn no_room() -> Script {
    Script {
        free: Some(0),
        ..Script::answering(&[Reply::always(Answer::Skip)])
    }
}

fn retry_of(plan: &Request, report: &Report) -> Request {
    Request {
        sources: report
            .skipped
            .iter()
            .map(|item| item.source.clone())
            .collect(),
        retrying: report.skipped.clone(),
        ..plan.clone()
    }
}

fn file_names(report: &Report) -> Vec<String> {
    let mut names: Vec<String> = (report.skipped.iter())
        .map(|item| item.source.file_name().unwrap())
        .collect();
    names.sort();
    names
}

fn clean(report: &Report) {
    assert_eq!(report, &Report::default());
}

#[test]
fn what_an_error_left_out_is_told_with_where_it_was_going() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let plan = request(std::slice::from_ref(&tree), &target, false, Existing::Ask);

    let report = run(&plan, &mut no_room());

    assert_eq!(file_names(&report), ["a.txt", "b.txt", "c.txt"]);
    let deep = (report.skipped.iter())
        .find(|item| item.source.file_name().unwrap() == "c.txt")
        .unwrap();
    assert_eq!(
        deep.destination,
        VfsPath::local(target.join("tree").join("inner").join("c.txt"))
    );
    assert_eq!(deep.top, VfsPath::local(&tree));
    assert!(!deep.inside_link);
}

#[test]
fn a_retry_does_only_what_was_left_out() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let plan = request(std::slice::from_ref(&tree), &target, false, Existing::Ask);
    let report = run(&plan, &mut no_room());
    // One got done meanwhile: a retry that walked the folder would ask about it.
    let retry = retry_of(&plan, &report);
    fs::write(tree.join("unrelated.txt"), "new").unwrap();

    clean(&run(&retry, &mut Script::default()));

    assert_eq!(read(&target.join("tree").join("a.txt")), "a-new");
    assert_eq!(
        read(&target.join("tree").join("inner").join("c.txt")),
        "c-new"
    );
    assert!(!target.join("tree").join("unrelated.txt").exists());
}

#[test]
fn a_retry_that_fails_again_can_be_retried_again() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let plan = request(std::slice::from_ref(&tree), &target, false, Existing::Ask);
    let first = run(&plan, &mut no_room());

    let second = run(&retry_of(&plan, &first), &mut no_room());

    assert_eq!(second.skipped, first.skipped);
}

#[test]
fn a_retried_move_takes_the_emptied_folders_away() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let plan = request(std::slice::from_ref(&tree), &target, true, Existing::Ask);
    let mut host = no_room();
    host.cross_volume = true;
    let report = run(&plan, &mut host);
    assert!(tree.join("inner").join("c.txt").exists());

    let mut host = Script {
        cross_volume: true,
        ..Script::default()
    };
    clean(&run(&retry_of(&plan, &report), &mut host));

    assert!(
        !tree.exists(),
        "the selected folder went with the last of it"
    );
    assert_eq!(
        read(&target.join("tree").join("inner").join("c.txt")),
        "c-new"
    );
}

#[test]
fn a_retried_move_stops_at_the_selected_folder_and_keeps_what_is_still_there() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let plan = request(std::slice::from_ref(&tree), &target, true, Existing::Ask);
    let mut host = no_room();
    host.cross_volume = true;
    let report = run(&plan, &mut host);
    let only_deep = Report {
        skipped: (report.skipped.iter())
            .filter(|item| item.source.file_name().unwrap() == "c.txt")
            .cloned()
            .collect(),
        ..Report::default()
    };

    let mut host = Script {
        cross_volume: true,
        ..Script::default()
    };
    clean(&run(&retry_of(&plan, &only_deep), &mut host));

    assert!(!tree.join("inner").exists(), "emptied");
    assert_eq!(names(&tree), ["a.txt", "b.txt"], "the rest stays");
    assert!(
        temp.path().exists(),
        "nothing above the selected folder is looked at"
    );
}

#[test]
fn a_refusal_and_the_users_own_skip_are_not_for_retrying() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::create_dir(target.join("a")).unwrap();
    let file = tree.join("a.txt");
    fs::write(target.join("a.txt"), "old").unwrap();
    let plan = request(std::slice::from_ref(&file), &target, false, Existing::Skip);
    assert!(run(&plan, &mut Script::default()).skipped.is_empty());

    let onto_itself = request(std::slice::from_ref(&file), &tree, false, Existing::Ask);
    let report = run(&onto_itself, &mut Script::default());
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert!(report.skipped.is_empty());
}

#[cfg(windows)]
fn junction(link: &std::path::Path, target: &std::path::Path) {
    let done = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(done.status.success(), "{done:?}");
}

#[cfg(windows)]
#[test]
fn a_retried_move_leaves_what_lies_behind_a_followed_link() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let holder = temp.path().join("holder");
    fs::create_dir(&holder).unwrap();
    junction(&holder.join("link"), &tree);
    let mut plan = request(std::slice::from_ref(&holder), &target, true, Existing::Ask);
    plan.keep.link_contents = true;
    let mut host = no_room();
    host.cross_volume = true;
    let report = run(&plan, &mut host);
    assert!(
        report.skipped.iter().all(|item| item.inside_link),
        "{report:?}"
    );

    let mut host = Script {
        cross_volume: true,
        ..Script::default()
    };
    clean(&run(&retry_of(&plan, &report), &mut host));

    assert_eq!(
        read(&tree.join("a.txt")),
        "a-new",
        "behind the link it stays"
    );
    assert_eq!(
        read(&target.join("holder").join("link").join("a.txt")),
        "a-new"
    );
}
