//! The engine's hard paths: moves across volumes cancelled at every step, append undone, Retry, the
//! worker paused, the totals, a file reached twice through a hard link (ADR-0009).

use std::fs;
use std::time::{Duration, Instant};

use super::tests::{Script, names, read, request, setup};
use super::*;

/// The files of `setup`, relative to the tree.
const FILES: [&str; 3] = ["a.txt", "b.txt", "inner/c.txt"];

#[test]
fn a_move_across_volumes_cancelled_at_any_step_leaves_every_file_in_one_place() {
    for cancel_at in 1..12 {
        let temp = tempfile::tempdir().unwrap();
        let (tree, target) = setup(temp.path());
        let mut host = Script {
            cross_volume: true,
            cancel_at: Some(cancel_at),
            ..Script::default()
        };
        run(
            &request(std::slice::from_ref(&tree), &target, true, Existing::Ask),
            &mut host,
        );
        for file in FILES {
            let at_source = tree.join(file).exists();
            let at_target = target.join("tree").join(file).exists();
            assert!(
                at_source != at_target,
                "cancel at {cancel_at}: {file} source {at_source}, target {at_target}"
            );
        }
        let copied = target.join("tree");
        assert!(
            !copied.exists() || names(&copied).iter().all(|name| !name.ends_with(".nc-tmp")),
            "no temporary file"
        );
    }
}

#[test]
fn a_move_across_volumes_copies_deletes_and_removes_emptied_folders() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    let mut host = Script {
        cross_volume: true,
        ..Script::default()
    };
    let report = run(
        &request(std::slice::from_ref(&tree), &target, true, Existing::Ask),
        &mut host,
    );
    assert!(report.failures.is_empty(), "{report:?}");
    assert!(!tree.exists(), "the emptied source folders are gone");
    for file in FILES {
        assert!(target.join("tree").join(file).exists(), "{file}");
    }
    // Overwrite across volumes: a safe copy, then the source goes.
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    run(
        &request(
            std::slice::from_ref(&source),
            &target,
            true,
            Existing::Overwrite,
        ),
        &mut Script {
            cross_volume: true,
            ..Script::default()
        },
    );
    assert_eq!(read(&target.join("a.txt")), "new");
    assert!(!source.exists());
}

#[test]
fn a_read_only_target_keeps_its_attribute_when_it_is_not_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let old = target.join("a.txt");
    fs::write(&old, "ol").unwrap();
    let mut permissions = fs::metadata(&old).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&old, permissions).unwrap();
    // A move across volumes finds no room even without the old file: skipped.
    let mut host = Script {
        cross_volume: true,
        free: Some(1),
        ..Script::answering(&[Reply::once(Answer::Overwrite), Reply::once(Answer::Skip)])
    };
    run(
        &request(&[tree.join("a.txt")], &target, true, Existing::Overwrite),
        &mut host,
    );
    assert!(fs::metadata(&old).unwrap().permissions().readonly());
    assert_eq!(read(&old), "ol");
    assert!(tree.join("a.txt").exists());
}

#[test]
fn a_cancelled_append_cuts_the_file_back() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("big.bin");
    fs::write(&source, vec![b'x'; 3_000_000]).unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("big.bin"), "old+").unwrap();
    // The first look is before the item, the second after the first chunk appended.
    let mut host = Script {
        cancel_at: Some(2),
        ..Script::default()
    };
    let report = run(
        &request(&[source], &target, false, Existing::Append),
        &mut host,
    );
    assert!(report.cancelled);
    assert_eq!(read(&target.join("big.bin")), "old+");
}

#[cfg(windows)]
#[test]
fn retry_after_the_file_was_let_go_copies_it() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(tree.join("a.txt"))
        .unwrap();
    let mut lock = Some(lock);
    let mut host = Script {
        on_ask: Some(Box::new(move || drop(lock.take()))),
        ..Script::answering(&[Reply::once(Answer::Retry)])
    };
    let report = run(
        &request(&[tree.join("a.txt")], &target, false, Existing::Ask),
        &mut host,
    );
    assert!(report.failures.is_empty(), "{report:?}");
    assert!(matches!(
        host.asked[..],
        [Question::Error { retry: true, .. }]
    ));
    assert_eq!(read(&target.join("a.txt")), "a-new");
}

#[test]
fn a_new_file_without_room_is_asked_about_and_not_started() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let mut host = Script {
        free: Some(1),
        ..Script::answering(&[Reply::once(Answer::Skip)])
    };
    let report = run(
        &request(&[tree.join("a.txt")], &target, false, Existing::Ask),
        &mut host,
    );
    assert!(matches!(
        host.asked[..],
        [Question::Error { retry: true, .. }]
    ));
    assert_eq!(report.failures.len(), 1);
    assert!(!target.join("a.txt").exists());
}

#[test]
fn a_file_reached_twice_through_a_hard_link_is_not_overwritten_with_itself() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::create_dir(target.join("tree")).unwrap();
    fs::hard_link(tree.join("a.txt"), target.join("tree").join("a.txt")).unwrap();
    let report = run(
        &request(
            std::slice::from_ref(&tree),
            &target,
            false,
            Existing::Overwrite,
        ),
        &mut Script::default(),
    );
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert_eq!(read(&tree.join("a.txt")), "a-new");
    assert_eq!(read(&target.join("tree").join("b.txt")), "b-new");
}

#[test]
fn a_folder_whose_name_changes_only_in_case_is_renamed() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, _) = setup(temp.path());
    let report = run(
        &request(
            std::slice::from_ref(&tree),
            &temp.path().join("TREE"),
            true,
            Existing::Ask,
        ),
        &mut Script::default(),
    );
    assert!(report.failures.is_empty(), "{report:?}");
    assert!(names(temp.path()).contains(&"TREE".to_string()));
    assert_eq!(read(&temp.path().join("TREE").join("a.txt")), "a-new");
}

fn wait(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "waited too long");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_paused_job_waits_and_the_totals_are_counted() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("tree.txt"), "x").unwrap();
    fs::create_dir(target.join("tree")).unwrap();
    fs::write(target.join("tree").join("a.txt"), "old").unwrap();
    let mut job = Job::start(
        request(std::slice::from_ref(&tree), &target, false, Existing::Ask),
        None,
    );
    wait(|| job.question().is_some());
    job.pause(true);
    job.answer(Reply::once(Answer::Overwrite));
    std::thread::sleep(Duration::from_millis(300));
    assert!(job.finished().is_none(), "the paused worker waits");
    assert_eq!(read(&target.join("tree").join("a.txt")), "old");
    job.pause(false);
    let mut report = None;
    wait(|| {
        report = job.finished();
        report.is_some()
    });
    assert_eq!(report, Some(Report::default()));
    assert_eq!(read(&target.join("tree").join("a.txt")), "a-new");
    wait(|| job.progress().totals_known);
    let progress = job.progress();
    assert_eq!((progress.files_total, progress.bytes_total), (3, 15));
    assert_eq!(progress.files_done, 3);
}

#[test]
fn only_what_went_entirely_is_finished() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("b.txt"), "old").unwrap();
    fs::create_dir(target.join("inner")).unwrap();
    fs::write(target.join("inner").join("c.txt"), "old").unwrap();
    let (a, b, inner) = (tree.join("a.txt"), tree.join("b.txt"), tree.join("inner"));
    let mut host = Script::default();

    run(
        &request(&[a.clone(), b, inner], &target, false, Existing::Skip),
        &mut host,
    );

    assert_eq!(
        host.progress.finished,
        [crate::vfs::VfsPath::local(&a)],
        "the skipped file and the folder with a skipped file are not"
    );
}

#[test]
fn a_failed_object_is_not_finished_and_the_next_one_is() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let missing = tree.join("missing.txt");
    let a = tree.join("a.txt");
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);

    run(
        &request(&[missing, a.clone()], &target, true, Existing::Ask),
        &mut host,
    );

    assert_eq!(host.progress.finished, [crate::vfs::VfsPath::local(&a)]);
}

/// `path` open with no sharing, by this very process.
#[cfg(windows)]
fn held_here(path: &std::path::Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap()
}

#[cfg(windows)]
fn asked_holders(script: &Script) -> Vec<String> {
    script
        .asked
        .iter()
        .find_map(|question| match question {
            Question::Error { held, .. } => Some(held.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[cfg(windows)]
#[test]
fn the_error_question_names_who_holds_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let source = tree.join("a.txt");
    let _held = held_here(&source);
    let mut script = Script::answering(&[Reply::once(Answer::Skip)]);

    run(
        &request(std::slice::from_ref(&source), &target, false, Existing::Ask),
        &mut script,
    );

    let held = asked_holders(&script);
    assert_eq!(held[0], "Object is being opened in:", "{held:?}");
    assert!(
        held.iter().any(|line| line.contains("current process")),
        "{held:?}"
    );
}

/// The source is free: what is named must be the holder of the file it would replace.
#[cfg(windows)]
#[test]
fn a_move_over_a_held_file_names_who_holds_the_target() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let source = tree.join("a.txt");
    let old = target.join("a.txt");
    fs::write(&old, "old").unwrap();
    let _held = held_here(&old);
    let mut script = Script::answering(&[Reply::once(Answer::Skip)]);

    run(
        &request(
            std::slice::from_ref(&source),
            &target,
            true,
            Existing::Overwrite,
        ),
        &mut script,
    );

    let held = asked_holders(&script);
    assert!(
        held.iter().any(|line| line.contains("current process")),
        "{held:?}"
    );
}
