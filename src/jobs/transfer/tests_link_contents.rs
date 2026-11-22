//! "Copy contents of symbolic links" (ADR-0009 §10), on junctions in temporary folders.

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::tests::{Script, names, read, request};
use super::*;

fn junction(link: &Path, target: &Path) {
    let done = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(done.status.success(), "{done:?}");
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).unwrap().is_symlink()
}

fn following(mut request: Request) -> Request {
    request.keep.link_contents = true;
    request
}

/// `root/real` with `a.txt` and `inner/b.txt`, an empty `root/target`.
fn real_folder(root: &Path) -> (PathBuf, PathBuf) {
    let real = root.join("real");
    fs::create_dir_all(real.join("inner")).unwrap();
    fs::write(real.join("a.txt"), "a").unwrap();
    fs::write(real.join("inner").join("b.txt"), "b").unwrap();
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    (real, target)
}

fn clean(report: &Report) {
    assert_eq!(report, &Report::default());
}

#[test]
fn a_link_followed_becomes_a_folder_of_its_own() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let link = temp.path().join("link");
    junction(&link, &real);
    let plan = following(request(&[link], &target, false, Existing::Ask));

    clean(&run(&plan, &mut Script::default()));

    let copy = target.join("link");
    assert!(!is_link(&copy), "a folder, not a link");
    assert_eq!(read(&copy.join("a.txt")), "a");
    assert_eq!(read(&copy.join("inner").join("b.txt")), "b");
}

#[test]
fn a_link_not_followed_stays_a_link() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let link = temp.path().join("link");
    junction(&link, &real);

    clean(&run(
        &request(&[link], &target, false, Existing::Ask),
        &mut Script::default(),
    ));

    assert!(is_link(&target.join("link")));
}

#[test]
fn a_move_takes_the_link_and_leaves_what_it_leads_to() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let link = temp.path().join("link");
    junction(&link, &real);
    let plan = following(request(
        std::slice::from_ref(&link),
        &target,
        true,
        Existing::Ask,
    ));

    clean(&run(&plan, &mut Script::default()));

    assert!(fs::symlink_metadata(&link).is_err(), "the link went");
    assert_eq!(read(&real.join("a.txt")), "a", "what it leads to stayed");
    assert_eq!(read(&real.join("inner").join("b.txt")), "b");
    assert_eq!(read(&target.join("link").join("a.txt")), "a");
}

#[test]
fn a_moved_folder_leaves_what_a_link_inside_it_leads_to() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let moved = temp.path().join("moved");
    fs::create_dir(&moved).unwrap();
    fs::write(moved.join("own.txt"), "own").unwrap();
    junction(&moved.join("link"), &real);
    let plan = following(request(
        std::slice::from_ref(&moved),
        &target,
        true,
        Existing::Ask,
    ));
    let mut host = Script {
        cross_volume: true,
        ..Script::default()
    };

    clean(&run(&plan, &mut host));

    assert!(!moved.exists(), "the moved folder went");
    assert_eq!(read(&real.join("a.txt")), "a");
    assert_eq!(read(&target.join("moved").join("link").join("a.txt")), "a");
    assert_eq!(read(&target.join("moved").join("own.txt")), "own");
}

#[test]
fn a_link_stays_when_something_behind_it_was_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let link = temp.path().join("link");
    junction(&link, &real);
    fs::create_dir(target.join("link")).unwrap();
    fs::write(target.join("link").join("a.txt"), "old").unwrap();
    let plan = following(request(
        std::slice::from_ref(&link),
        &target,
        true,
        Existing::Skip,
    ));
    let mut host = Script::default();

    run(&plan, &mut host);

    assert!(is_link(&link), "something behind it did not go");
    assert_eq!(read(&target.join("link").join("a.txt")), "old");
    assert!(host.progress.finished.is_empty(), "it keeps its mark");
}

#[test]
fn a_link_back_up_is_reported_and_the_rest_is_copied() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    junction(&real.join("inner").join("back"), &real);
    let mut host = Script::default();

    let report = run(&request_following(&real, &target), &mut host);

    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert!(report.failures[0].contains("leads back"), "{report:?}");
    let copy = target.join("real");
    assert_eq!(read(&copy.join("a.txt")), "a");
    assert_eq!(names(&copy.join("inner")), ["b.txt"]);
    assert!(host.progress.finished.is_empty(), "it keeps its mark");
}

fn request_following(source: &Path, target: &Path) -> Request {
    following(request(
        &[source.to_path_buf()],
        target,
        false,
        Existing::Ask,
    ))
}

#[test]
fn two_links_to_one_folder_are_two_copies() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let holder = temp.path().join("holder");
    fs::create_dir(&holder).unwrap();
    junction(&holder.join("one"), &real);
    junction(&holder.join("two"), &real);

    clean(&run(
        &request_following(&holder, &target),
        &mut Script::default(),
    ));

    assert_eq!(read(&target.join("holder").join("one").join("a.txt")), "a");
    assert_eq!(read(&target.join("holder").join("two").join("a.txt")), "a");
}

#[test]
fn a_link_to_the_folder_written_into_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (real, _target) = real_folder(temp.path());
    let link = temp.path().join("link");
    junction(&link, &real);
    let into = real.join("inner");

    let report = run(&request_following(&link, &into), &mut Script::default());

    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert!(report.failures[0].contains("into itself"), "{report:?}");
    assert_eq!(names(&into), ["b.txt"], "nothing was written");
}

#[test]
fn a_nested_link_to_where_the_copy_goes_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let holder = temp.path().join("holder");
    fs::create_dir(&holder).unwrap();
    junction(&holder.join("up"), temp.path());

    let report = run(&request_following(&holder, &target), &mut Script::default());

    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert!(report.failures[0].contains("into itself"), "{report:?}");
    assert_eq!(names(&target.join("holder")), Vec::<String>::new());
    assert_eq!(read(&real.join("a.txt")), "a");
}

#[test]
fn a_link_to_nowhere_is_asked_about() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let link = temp.path().join("link");
    junction(&link, &real.join("inner"));
    fs::remove_file(real.join("inner").join("b.txt")).unwrap();
    fs::remove_dir(real.join("inner")).unwrap();
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);

    let report = run(&request_following(&link, &target), &mut host);

    assert!(
        matches!(host.asked[..], [Question::Error { .. }]),
        "{:?}",
        host.asked
    );
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert_eq!(names(&target), Vec::<String>::new());
}

fn totals(request: Request) -> Progress {
    let mut job = Job::start(request, None);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !job.progress().totals_known {
        assert!(Instant::now() < deadline, "no totals");
        std::thread::sleep(Duration::from_millis(2));
    }
    let progress = job.progress();
    job.cancel();
    progress
}

#[test]
fn the_totals_count_what_links_lead_to_and_a_link_back_ends() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    junction(&real.join("inner").join("back"), &real);
    let holder = temp.path().join("holder");
    fs::create_dir(&holder).unwrap();
    junction(&holder.join("link"), &real);
    // The copy has its own question to wait on: the totals go on beside it.
    fs::create_dir_all(target.join("holder").join("link")).unwrap();
    fs::write(target.join("holder").join("link").join("a.txt"), "old").unwrap();

    let progress = totals(request_following(&holder, &target));

    assert_eq!((progress.files_total, progress.bytes_total), (2, 2));
}

#[test]
fn the_totals_take_a_link_for_one_file_when_it_is_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let (real, target) = real_folder(temp.path());
    let holder = temp.path().join("holder");
    fs::create_dir(&holder).unwrap();
    junction(&holder.join("link"), &real);
    fs::create_dir_all(target.join("holder")).unwrap();
    let link_there = target.join("holder").join("link");
    fs::write(&link_there, "in the way").unwrap();

    let progress = totals(request(&[holder], &target, false, Existing::Ask));

    assert_eq!(progress.files_total, 1);
}
