//! "Access denied" in a copy or move (ADR-0007): the user's answers and what reaches the helper. The
//! helper itself is `vfs::local::elevated`'s; here it is a script.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use super::tests::{Script, request};
use super::*;
use crate::jobs::elevation::Verb;
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::local::elevated_session::{self, Policy, not_started};

fn denied_move(replies: &[Reply]) -> Script {
    Script {
        deny_moves: true,
        elevate: true,
        ..Script::answering(replies)
    }
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "a").unwrap();
    let target = temp.path().join("to");
    fs::create_dir(&target).unwrap();
    (temp, source, target)
}

#[test]
fn a_denied_move_asks_and_goes_through_the_helper() {
    let (_temp, source, target) = setup();
    let plan = request(std::slice::from_ref(&source), &target, true, Existing::Ask);
    let mut script = denied_move(&[Reply::once(Answer::Elevate)]);

    let report = run(&plan, &mut script);

    assert!(report.failures.is_empty(), "{report:?}");
    assert!(matches!(
        script.asked[..],
        [Question::Elevate {
            verb: Verb::Move,
            ..
        }]
    ));
    assert!(matches!(script.elevated[..], [Elevated::MoveFile { .. }]));
    assert_eq!(fs::read(target.join("a.txt")).unwrap(), b"a");
    assert!(!source.exists());
}

#[test]
fn skip_goes_on_to_the_usual_error_question() {
    let (_temp, source, target) = setup();
    let plan = request(std::slice::from_ref(&source), &target, true, Existing::Ask);
    let mut script = denied_move(&[Reply::once(Answer::Skip), Reply::once(Answer::Skip)]);

    let report = run(&plan, &mut script);

    assert_eq!(script.asked.len(), 2, "{:?}", script.asked);
    assert!(matches!(script.asked[0], Question::Elevate { .. }));
    assert!(matches!(script.asked[1], Question::Error { .. }));
    assert!(script.elevated.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert!(source.exists(), "nothing was moved");
}

#[test]
fn esc_stops_the_operation_with_no_other_question() {
    let (_temp, source, target) = setup();
    let plan = request(std::slice::from_ref(&source), &target, true, Existing::Ask);
    let mut script = denied_move(&[Reply::once(Answer::Cancel)]);

    let report = run(&plan, &mut script);

    assert!(report.cancelled);
    assert_eq!(script.asked.len(), 1, "{:?}", script.asked);
    assert!(script.elevated.is_empty());
    assert!(source.exists());
}

#[test]
fn for_all_asks_once() {
    let (temp, source, target) = setup();
    let second = temp.path().join("b.txt");
    fs::write(&second, "b").unwrap();
    let plan = request(&[source, second], &target, true, Existing::Ask);
    let mut script = denied_move(&[Reply::always(Answer::Elevate)]);

    let report = run(&plan, &mut script);

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(script.asked.len(), 1, "{:?}", script.asked);
    assert_eq!(script.elevated.len(), 2);
}

#[test]
fn without_a_helper_to_offer_the_failure_is_the_usual_one() {
    let (_temp, source, target) = setup();
    let plan = request(std::slice::from_ref(&source), &target, true, Existing::Ask);
    let mut script = Script {
        deny_moves: true,
        ..Script::answering(&[Reply::once(Answer::Skip)])
    };

    let report = run(&plan, &mut script);

    assert!(matches!(script.asked[..], [Question::Error { .. }]));
    assert_eq!(report.failures.len(), 1);
}

/// Denies Everyone `rights` on `path` until dropped; only on the test's own temporary files.
struct Denied(std::path::PathBuf);

impl Denied {
    fn new(path: &Path, rights: &str) -> Self {
        icacls(path, &["/deny", &format!("*S-1-1-0:{rights}")]);
        Self(path.to_path_buf())
    }
}

impl Drop for Denied {
    fn drop(&mut self) {
        icacls(&self.0, &["/reset", "/T"]);
    }
}

fn icacls(path: &Path, arguments: &[&str]) {
    let done = Command::new("icacls")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(done.status.success(), "{done:?}");
}

fn helping(replies: &[Reply]) -> Script {
    Script {
        elevate: true,
        ..Script::answering(replies)
    }
}

fn exact(path: &Path) -> std::path::PathBuf {
    crate::platform::files::exact_path(path)
}

#[test]
fn a_denied_copy_asks_about_the_source_and_goes_through_the_helper() {
    let (_temp, source, target) = setup();
    let _denied = Denied::new(&target, "(WD)");
    let plan = request(std::slice::from_ref(&source), &target, false, Existing::Ask);
    let mut script = helping(&[Reply::once(Answer::Elevate)]);

    run(&plan, &mut script);

    assert_eq!(
        script.asked[0],
        Question::Elevate {
            path: VfsPath::local(&source),
            verb: Verb::Copy
        },
        "FAR names what is copied"
    );
    assert_eq!(
        script.elevated,
        [Elevated::CopyFile {
            from: exact(&source),
            to: exact(&target.join("a.txt")),
        }]
    );
}

#[test]
fn esc_at_a_denied_copy_stops_and_skip_leaves_the_usual_question() {
    let (_temp, source, target) = setup();
    let _denied = Denied::new(&target, "(WD)");
    let plan = request(std::slice::from_ref(&source), &target, false, Existing::Ask);

    let mut stopped = helping(&[Reply::once(Answer::Cancel)]);
    assert!(run(&plan, &mut stopped).cancelled);
    assert_eq!(stopped.asked.len(), 1);

    let mut skipped = helping(&[Reply::once(Answer::Skip), Reply::once(Answer::Skip)]);
    let report = run(&plan, &mut skipped);
    assert!(matches!(skipped.asked[1], Question::Error { .. }));
    assert!(skipped.elevated.is_empty());
    assert_eq!(report.failures.len(), 1);
}

#[test]
fn a_copy_cancelled_inside_the_helper_cancels_the_operation() {
    let (_temp, source, target) = setup();
    let _denied = Denied::new(&target, "(WD)");
    let plan = request(std::slice::from_ref(&source), &target, false, Existing::Ask);
    let mut script = helping(&[Reply::once(Answer::Elevate)]);
    script.helper_fails = Some(|_| Some(std::io::ErrorKind::Interrupted.into()));

    assert!(run(&plan, &mut script).cancelled);
    assert_eq!(script.asked.len(), 1, "no error question after a cancel");
}

#[test]
fn a_helper_that_could_not_start_leaves_the_error_and_is_not_offered_again() {
    let (temp, source, target) = setup();
    let second = temp.path().join("b.txt");
    fs::write(&second, "b").unwrap();
    let plan = request(&[source, second], &target, true, Existing::Ask);
    let mut script = denied_move(&[
        Reply::always(Answer::Elevate),
        Reply::once(Answer::Skip),
        Reply::once(Answer::Skip),
    ]);
    script.helper_fails = Some(|_| Some(not_started(std::io::Error::from_raw_os_error(1223))));

    let report = run(&plan, &mut script);

    assert_eq!(script.elevated.len(), 1, "one prompt, not one per file");
    assert!(matches!(script.asked[0], Question::Elevate { .. }));
    let Question::Error { message, .. } = &script.asked[1] else {
        panic!("{:?}", script.asked[1]);
    };
    assert!(
        message.contains("os error 5"),
        "the object's own error: {message}"
    );
    assert!(matches!(script.asked[2], Question::Error { .. }));
    assert_eq!(report.failures.len(), 2);
}

#[test]
fn retry_after_a_skipped_question_asks_again() {
    let (_temp, source, target) = setup();
    let plan = request(std::slice::from_ref(&source), &target, true, Existing::Ask);
    let mut script = denied_move(&[
        Reply::once(Answer::Skip),
        Reply::once(Answer::Retry),
        Reply::once(Answer::Elevate),
    ]);

    let report = run(&plan, &mut script);

    assert!(report.failures.is_empty(), "{report:?}");
    assert!(matches!(script.asked[2], Question::Elevate { .. }));
    assert_eq!(script.elevated.len(), 1);
}

#[test]
fn the_sessions_yes_asks_nothing_and_its_no_only_the_usual_question() {
    let (_temp, source, target) = setup();
    let plan = request(std::slice::from_ref(&source), &target, true, Existing::Ask);

    elevated_session::set_policy(Policy::Never);
    let mut refused = denied_move(&[Reply::once(Answer::Skip)]);
    run(&plan, &mut refused);
    elevated_session::set_policy(Policy::Ask);
    assert!(matches!(refused.asked[..], [Question::Error { .. }]));
    assert!(refused.elevated.is_empty());

    elevated_session::set_policy(Policy::Always);
    let mut allowed = denied_move(&[]);
    let report = run(&plan, &mut allowed);
    elevated_session::set_policy(Policy::Ask);
    assert!(report.failures.is_empty(), "{report:?}");
    assert!(allowed.asked.is_empty());
    assert_eq!(allowed.elevated.len(), 1);
}

#[test]
fn the_moved_sources_removal_goes_through_the_helper() {
    let temp = tempfile::tempdir().unwrap();
    let protected = temp.path().join("protected");
    fs::create_dir(&protected).unwrap();
    let source = protected.join("a.txt");
    fs::write(&source, "a").unwrap();
    let target = temp.path().join("to");
    fs::create_dir(&target).unwrap();
    let _denied = (Denied::new(&source, "(D)"), Denied::new(&protected, "(DC)"));
    let plan = request(std::slice::from_ref(&source), &target, true, Existing::Ask);
    // A file nobody may delete cannot be opened for the copy either: the first answer is for all.
    let mut script = helping(&[Reply::always(Answer::Elevate)]);
    script.cross_volume = true;

    let report = run(&plan, &mut script);

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(script.asked.len(), 1, "{:?}", script.asked);
    assert_eq!(
        script.elevated.last(),
        Some(&Elevated::DeleteFile(exact(&source)))
    );
}

#[test]
fn a_denied_folder_is_made_through_the_helper() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("empty");
    fs::create_dir(&source).unwrap();
    let target = temp.path().join("to");
    fs::create_dir(&target).unwrap();
    let _denied = Denied::new(&target, "(AD)");
    let plan = request(std::slice::from_ref(&source), &target, false, Existing::Ask);
    let mut script = helping(&[Reply::once(Answer::Elevate)]);

    run(&plan, &mut script);

    assert!(matches!(
        script.asked[0],
        Question::Elevate {
            verb: Verb::Create,
            ..
        }
    ));
    assert_eq!(
        script.elevated,
        [Elevated::CreateDirectory(exact(&target.join("empty")))]
    );
}

/// The read-only attribute of a protected target comes off through the helper and goes back after a
/// failure the same way; the temporary file is removed by it with no question.
#[test]
fn a_protected_read_only_target_and_the_temporary_file_are_left_as_they_were() {
    let (_temp, source, target) = setup();
    let old = target.join("a.txt");
    fs::write(&old, "old").unwrap();
    crate::platform::files::set_read_only(&old, true).unwrap();
    let _denied = Denied::new(&old, "(WA)");
    let plan = request(
        std::slice::from_ref(&source),
        &target,
        false,
        Existing::Overwrite,
    );
    let mut script = helping(&[
        Reply::once(Answer::Overwrite),
        Reply::always(Answer::Elevate),
        Reply::once(Answer::Cancel),
    ]);
    script.helper_fails = Some(|request| {
        matches!(request, Elevated::MoveFile { .. }).then(|| std::io::Error::from_raw_os_error(32))
    });

    let report = run(&plan, &mut script);

    assert!(report.cancelled);
    assert!(matches!(script.asked[0], Question::ReadOnly { .. }));
    assert!(matches!(
        script.asked[1],
        Question::Elevate {
            verb: Verb::SetAttributes,
            ..
        }
    ));
    let exact_old = exact(&old);
    assert_eq!(
        script.elevated[0],
        Elevated::SetReadOnly {
            path: exact_old.clone(),
            read_only: false
        }
    );
    assert!(matches!(script.elevated[1], Elevated::MoveFile { .. }));
    assert_eq!(
        script.elevated[2],
        Elevated::SetReadOnly {
            path: exact_old,
            read_only: true
        },
        "its read-only comes back"
    );
    assert_eq!(fs::read(&old).unwrap(), b"old");
    assert!(
        !target.join(".a.txt.nc-tmp").exists(),
        "the temporary file is gone"
    );
}

fn overwriting(source: &Path, target: &Path) -> Request {
    request(&[source.to_path_buf()], target, false, Existing::Overwrite)
}

/// A copy over `a.txt` whose temporary file does not take its place (renames are denied), and from
/// the first question on nobody may delete that file. The rights come back when this is dropped,
/// before the folder is: `script` and its question hold the guards, `_temp` goes last.
struct StuckTemporary {
    guards: Rc<RefCell<Vec<Denied>>>,
    temporary: PathBuf,
    plan: Request,
    script: Script,
    _temp: tempfile::TempDir,
}

fn stuck_temporary(replies: &[Reply], elevate: bool) -> StuckTemporary {
    let (temp, source, target) = setup();
    fs::write(target.join("a.txt"), "old").unwrap();
    let temporary = target.join(".a.txt.nc-tmp");
    let plan = overwriting(&source, &target);
    let guards: Rc<RefCell<Vec<Denied>>> = Rc::default();
    let (armed, file) = (Rc::clone(&guards), temporary.clone());
    let script = Script {
        deny_moves: true,
        elevate,
        on_ask: Some(Box::new(move || {
            if armed.borrow().is_empty() {
                armed
                    .borrow_mut()
                    .extend([Denied::new(&file, "(D)"), Denied::new(&target, "(DC)")]);
            }
        })),
        ..Script::answering(replies)
    };
    StuckTemporary {
        guards,
        temporary,
        plan,
        script,
        _temp: temp,
    }
}

fn about_the_temporary(report: &Report, temporary: &Path) -> Vec<String> {
    let start = format!(
        "{}: the temporary file could not be removed: ",
        VfsPath::local(temporary)
    );
    report
        .failures
        .iter()
        .filter(|failure| failure.starts_with(&start))
        .cloned()
        .collect()
}

#[test]
fn a_temporary_file_that_cannot_be_removed_is_reported_and_the_helper_is_not_used_for_it() {
    let mut stuck = stuck_temporary(
        &[Reply::once(Answer::Skip), Reply::once(Answer::Skip)],
        true,
    );

    let report = run(&stuck.plan, &mut stuck.script);

    assert!(!stuck.guards.borrow().is_empty(), "the rights were taken");
    assert!(
        matches!(
            stuck.script.asked[..],
            [Question::Elevate { .. }, Question::Error { .. }]
        ),
        "{:?}",
        stuck.script.asked
    );
    assert!(
        stuck.script.elevated.is_empty(),
        "nobody agreed to the helper: {:?}",
        stuck.script.elevated
    );
    let about = about_the_temporary(&report, &stuck.temporary);
    assert_eq!(about.len(), 1, "{report:?}");
    assert!(about[0].contains("os error 5"), "{report:?}");
    assert_eq!(report.failures.len(), 2, "the skip and this: {report:?}");
    assert!(stuck.temporary.exists(), "it did stay");
}

#[test]
fn once_the_helper_was_used_it_removes_the_temporary_file_with_no_question() {
    let mut stuck = stuck_temporary(
        &[Reply::once(Answer::Elevate), Reply::once(Answer::Skip)],
        true,
    );
    // The helper could not put it in place either: the file stays to be discarded.
    stuck.script.helper_fails = Some(|request| {
        matches!(request, Elevated::MoveFile { .. }).then(|| std::io::Error::from_raw_os_error(32))
    });

    let report = run(&stuck.plan, &mut stuck.script);

    assert_eq!(stuck.script.asked.len(), 2, "{:?}", stuck.script.asked);
    assert!(matches!(
        stuck.script.elevated[0],
        Elevated::MoveFile { .. }
    ));
    assert_eq!(
        stuck.script.elevated[1..],
        [Elevated::DeleteFile(exact(&stuck.temporary))]
    );
    assert!(about_the_temporary(&report, &stuck.temporary).is_empty());
}

#[test]
fn a_temporary_file_the_helper_could_not_remove_is_reported_too() {
    let mut stuck = stuck_temporary(
        &[Reply::once(Answer::Elevate), Reply::once(Answer::Skip)],
        true,
    );
    stuck.script.helper_fails = Some(|request| match request {
        Elevated::MoveFile { .. } => Some(std::io::Error::from_raw_os_error(32)),
        Elevated::DeleteFile(_) => Some(std::io::Error::from_raw_os_error(5)),
        _ => None,
    });

    let report = run(&stuck.plan, &mut stuck.script);

    assert_eq!(stuck.script.elevated.len(), 2);
    assert_eq!(
        about_the_temporary(&report, &stuck.temporary).len(),
        1,
        "{report:?}"
    );
}

#[test]
fn a_temporary_file_that_is_gone_already_is_not_reported() {
    let (_temp, source, target) = setup();
    fs::write(target.join("a.txt"), "old").unwrap();
    let plan = overwriting(&source, &target);
    let temporary = target.join(".a.txt.nc-tmp");
    let file = temporary.clone();
    let mut script = Script {
        deny_moves: true,
        on_ask: Some(Box::new(move || fs::remove_file(&file).unwrap())),
        ..Script::answering(&[Reply::once(Answer::Skip)])
    };

    let report = run(&plan, &mut script);

    assert_eq!(report.failures.len(), 1, "only the skip: {report:?}");
    assert!(about_the_temporary(&report, &temporary).is_empty());
    assert!(script.elevated.is_empty());
}

#[test]
fn a_temporary_link_that_did_not_take_its_place_goes_as_a_link() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("inside.txt"), "x").unwrap();
    let link = temp.path().join("link");
    crate::platform::files::make_link(crate::platform::files::NewLink::Junction, &real, &link)
        .unwrap();
    let target = temp.path().join("to");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("link"), "old").unwrap();
    let plan = overwriting(&link, &target);
    let mut script = Script {
        deny_moves: true,
        ..Script::answering(&[Reply::once(Answer::Skip)])
    };

    let report = run(&plan, &mut script);

    let temporary = target.join(".link.nc-tmp");
    assert!(
        fs::symlink_metadata(&temporary).is_err(),
        "the temporary link is gone: {report:?}"
    );
    assert_eq!(report.failures.len(), 1, "only the skip: {report:?}");
    assert_eq!(
        fs::read(real.join("inside.txt")).unwrap(),
        b"x",
        "the link's target stays"
    );
    assert_eq!(fs::read(target.join("link")).unwrap(), b"old");
}

#[test]
fn replacing_through_a_temporary_file_names_the_target() {
    let (_temp, source, target) = setup();
    let old = target.join("a.txt");
    fs::write(&old, "old").unwrap();
    let plan = request(
        std::slice::from_ref(&source),
        &target,
        false,
        Existing::Overwrite,
    );
    let mut script = denied_move(&[Reply::once(Answer::Elevate)]);

    run(&plan, &mut script);

    assert_eq!(
        script.asked[0],
        Question::Elevate {
            path: VfsPath::local(&old),
            verb: Verb::Replace
        }
    );
}
