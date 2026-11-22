//! Verify (ADR-0009, owner's): each copy read back; one that differs never stays under the real name, and
//! a move deletes its source only after the check passed.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::tests::{Script, names, read, request};
use super::*;
use crate::vfs::test_tree as tree;

fn verifying(mut request: Request) -> Request {
    request.keep.verify = true;
    request
}

/// `root/a.txt` holding `content`, an empty `root/target`.
fn source_and_target(root: &Path, content: &[u8]) -> (PathBuf, PathBuf) {
    let source = root.join("a.txt");
    fs::write(&source, content).unwrap();
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    (source, target)
}

/// A host that damages `victim` as the check starts, the first `times` times.
fn damaging(victim: PathBuf, times: u32, replies: &[Reply]) -> (Script, Rc<Cell<u32>>) {
    let damaged = Rc::new(Cell::new(0));
    let count = Rc::clone(&damaged);
    let mut host = Script::answering(replies);
    host.on_progress = Some(Box::new(move |progress: &Progress| {
        if progress.verified == Some(0) && count.get() < times {
            count.set(count.get() + 1);
            fs::write(&victim, "damaged").unwrap();
        }
    }));
    (host, damaged)
}

fn clean(report: &Report) {
    assert_eq!(report, &Report::default());
}

fn a_differing_copy_was_asked_about(host: &Script) {
    assert!(
        matches!(
            &host.asked[..],
            [Question::Error { message, retry: true, .. }] if message.contains("differs")
        ),
        "{:?}",
        host.asked
    );
}

#[test]
fn files_of_any_length_pass_the_check() {
    for length in [
        0,
        1,
        4095,
        4096,
        4097,
        1024 * 1024,
        1024 * 1024 + 1,
        3 * 1024 * 1024,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let content: Vec<u8> = (0..length).map(|at| (at % 251) as u8).collect();
        let (source, target) = source_and_target(temp.path(), &content);

        let plan = verifying(request(&[source], &target, false, Existing::Ask));
        clean(&run(&plan, &mut Script::default()));

        assert_eq!(fs::read(target.join("a.txt")).unwrap(), content, "{length}");
    }
}

#[test]
fn nothing_is_read_back_unless_asked() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"text");
    let (mut host, damaged) = damaging(target.join("a.txt"), 1, &[]);

    clean(&run(
        &request(&[source], &target, false, Existing::Ask),
        &mut host,
    ));

    assert_eq!(damaged.get(), 0);
}

#[test]
fn a_copy_that_differs_is_asked_about_and_skip_leaves_no_copy() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"text");
    let (mut host, _) = damaging(target.join("a.txt"), 1, &[Reply::once(Answer::Skip)]);
    let plan = verifying(request(
        std::slice::from_ref(&source),
        &target,
        false,
        Existing::Ask,
    ));

    let report = run(&plan, &mut host);

    a_differing_copy_was_asked_about(&host);
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert_eq!(
        names(&target),
        Vec::<String>::new(),
        "no copy under the real name"
    );
    assert_eq!(read(&source), "text");
    assert!(host.progress.finished.is_empty(), "it keeps its mark");
    assert_eq!(
        host.progress.verified, None,
        "the dialog is back to copying"
    );
}

#[test]
fn retry_copies_again_and_checks_again() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"text");
    let (mut host, damaged) = damaging(target.join("a.txt"), 1, &[Reply::once(Answer::Retry)]);

    clean(&run(
        &verifying(request(&[source], &target, false, Existing::Ask)),
        &mut host,
    ));

    assert_eq!(damaged.get(), 1);
    assert_eq!(read(&target.join("a.txt")), "text");
}

#[test]
fn a_move_keeps_its_source_until_the_check_has_passed() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"text");
    let (mut host, _) = damaging(target.join("a.txt"), 1, &[Reply::once(Answer::Skip)]);
    host.cross_volume = true;
    let plan = verifying(request(
        std::slice::from_ref(&source),
        &target,
        true,
        Existing::Ask,
    ));

    run(&plan, &mut host);

    assert_eq!(read(&source), "text", "the source stays");
    assert_eq!(names(&target), Vec::<String>::new());
}

#[test]
fn a_move_deletes_its_source_after_a_retry_passed() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"text");
    let (mut host, _) = damaging(target.join("a.txt"), 1, &[Reply::once(Answer::Retry)]);
    host.cross_volume = true;
    let plan = verifying(request(
        std::slice::from_ref(&source),
        &target,
        true,
        Existing::Ask,
    ));

    clean(&run(&plan, &mut host));

    assert!(!source.exists());
    assert_eq!(read(&target.join("a.txt")), "text");
}

#[test]
fn a_replaced_file_stays_as_it_was_when_the_new_copy_differs() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"text");
    fs::write(target.join("a.txt"), "old").unwrap();
    let temporary = target.join(".a.txt.nc-tmp");
    let (mut host, _) = damaging(temporary.clone(), 1, &[Reply::once(Answer::Skip)]);
    let plan = verifying(request(&[source], &target, false, Existing::Overwrite));

    run(&plan, &mut host);

    a_differing_copy_was_asked_about(&host);
    assert_eq!(read(&target.join("a.txt")), "old");
    assert!(!temporary.exists());
}

#[test]
fn cancel_at_the_question_leaves_no_copy() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"text");
    let (mut host, _) = damaging(target.join("a.txt"), 1, &[Reply::once(Answer::Cancel)]);

    let report = run(
        &verifying(request(&[source], &target, false, Existing::Ask)),
        &mut host,
    );

    assert!(report.cancelled);
    assert_eq!(names(&target), Vec::<String>::new());
}

#[test]
fn a_cancel_while_reading_back_leaves_no_copy() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), &vec![5u8; 3 * 1024 * 1024]);
    let looks = Rc::new(Cell::new(false));
    let seen = Rc::clone(&looks);
    let mut host = Script {
        // The copy looks at the flag too: it is told to stop only from the check on.
        on_progress: Some(Box::new(move |progress: &Progress| {
            seen.set(seen.get() || progress.verified.is_some());
        })),
        ..Script::default()
    };
    host.cancel_at = Some(usize::MAX);
    let mut host = StopWhenChecking {
        script: host,
        checking: looks,
    };

    let report = run(
        &verifying(request(&[source], &target, false, Existing::Ask)),
        &mut host,
    );

    assert!(report.cancelled, "{report:?}");
    assert_eq!(names(&target), Vec::<String>::new());
}

struct StopWhenChecking {
    script: Script,
    checking: Rc<Cell<bool>>,
}

impl Host for StopWhenChecking {
    fn ask(&mut self, question: Question) -> Reply {
        self.script.ask(question)
    }

    fn cancelled(&mut self) -> bool {
        self.checking.get()
    }

    fn progress(&mut self, change: &mut dyn FnMut(&mut Progress)) {
        self.script.progress(change);
    }
}

#[test]
fn an_append_that_differs_is_cut_back() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"new");
    let appended = target.join("a.txt");
    fs::write(&appended, "old").unwrap();
    let victim = appended.clone();
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);
    let damaged = Rc::new(Cell::new(false));
    let once = Rc::clone(&damaged);
    host.on_progress = Some(Box::new(move |progress: &Progress| {
        if progress.verified == Some(0) && !once.replace(true) {
            fs::OpenOptions::new()
                .append(true)
                .open(&victim)
                .and_then(|mut file| std::io::Write::write_all(&mut file, b"!"))
                .unwrap();
        }
    }));

    run(
        &verifying(request(&[source], &target, false, Existing::Append)),
        &mut host,
    );

    a_differing_copy_was_asked_about(&host);
    assert_eq!(read(&appended), "old");
}

#[test]
fn an_append_compares_only_what_it_added() {
    let temp = tempfile::tempdir().unwrap();
    let (source, target) = source_and_target(temp.path(), b"new");
    fs::write(target.join("a.txt"), "1234567").unwrap();

    clean(&run(
        &verifying(request(&[source], &target, false, Existing::Append)),
        &mut Script::default(),
    ));

    assert_eq!(read(&target.join("a.txt")), "1234567new");
}

#[test]
fn a_copy_behind_a_provider_is_read_back_too() {
    let root = tree::root("verify-provider");
    let target = tree::dir(&root.join("target"));
    let source = tree::dir(&root.join("from"));
    tree::write(&source.join("a.txt"), b"text");
    let written = target.join("a.txt");
    let plan = |target: &VfsPath| Request {
        sources: vec![source.join("a.txt")],
        target: target.clone(),
        move_: false,
        existing: Existing::Ask,
        ask_read_only: true,
        keep: Keep {
            verify: true,
            ..Keep::default()
        },
        link: None,
        retrying: Vec::new(),
        filter: None,
    };
    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);
    let victim = written.clone();
    let once = Rc::new(Cell::new(false));
    host.on_progress = Some(Box::new(move |progress: &Progress| {
        if progress.verified == Some(0) && !once.replace(true) {
            tree::write(&victim, b"damaged");
        }
    }));

    run(&plan(&target), &mut host);

    a_differing_copy_was_asked_about(&host);
    assert!(!tree::exists(&written));

    clean(&run(&plan(&target), &mut Script::default()));
    assert_eq!(tree::read(&written), b"text");
}
