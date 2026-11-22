//! "Access denied" in a deletion (ADR-0007) on a real file the user may not delete: rights set by the
//! test on its own temporary folder only, and taken back before it is removed.

use std::fs;
use std::path::Path;
use std::process::Command;

use super::tests::{Script, once, request};
use super::*;
use crate::jobs::elevation::Verb;
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::local::elevated_session::not_started;
use crate::vfs::{VfsPath, provider_for};

/// A file that cannot be deleted while the guard lives: deleting needs DELETE on it or
/// DELETE_CHILD on its folder, and both are denied.
struct Undeletable {
    _temp: tempfile::TempDir,
    folder: std::path::PathBuf,
    file: std::path::PathBuf,
}

fn icacls(path: &Path, arguments: &[&str]) {
    let done = Command::new("icacls")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(done.status.success(), "{done:?}");
}

/// Everyone, by its SID: the name differs by the system's language.
const DENY_DELETE: &str = "*S-1-1-0:(D)";
const DENY_DELETE_CHILD: &str = "*S-1-1-0:(DC)";

impl Undeletable {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("protected");
        fs::create_dir(&folder).unwrap();
        let file = folder.join("a.txt");
        fs::write(&file, "a").unwrap();
        icacls(&file, &["/deny", DENY_DELETE]);
        icacls(&folder, &["/deny", DENY_DELETE_CHILD]);
        Self {
            _temp: temp,
            folder,
            file,
        }
    }
}

impl Drop for Undeletable {
    fn drop(&mut self) {
        icacls(&self.folder, &["/remove:d", "*S-1-1-0"]);
        icacls(&self.file, &["/remove:d", "*S-1-1-0"]);
    }
}

fn plan(file: &VfsPath) -> Request {
    Request {
        items: vec![Item {
            path: file.clone(),
            entry: provider_for(file).unwrap().stat(file).unwrap(),
        }],
        ..request(&[], Mode::Remove)
    }
}

fn helping(replies: &[Reply]) -> Script {
    let mut script = Script::answering(replies);
    script.elevate = true;
    script
}

#[test]
fn a_denied_deletion_asks_and_goes_through_the_helper() {
    let denied = Undeletable::new();
    let file = VfsPath::local(&denied.file);
    let mut script = helping(&[once(Answer::Elevate)]);

    let report = run(&plan(&file), &mut script);

    assert!(report.failures.is_empty(), "{report:?}");
    assert!(matches!(
        script.asked[..],
        [Question::Elevate {
            verb: Verb::Delete,
            ..
        }]
    ));
    assert_eq!(script.elevated, [Elevated::DeleteFile(denied.file.clone())]);
}

#[test]
fn skip_goes_on_to_the_usual_error_question() {
    let denied = Undeletable::new();
    let file = VfsPath::local(&denied.file);
    let mut script = helping(&[once(Answer::Skip), once(Answer::Skip)]);

    let report = run(&plan(&file), &mut script);

    assert!(matches!(script.asked[0], Question::Elevate { .. }));
    assert!(matches!(script.asked[1], Question::Error { .. }));
    assert!(script.elevated.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert!(denied.file.exists());
}

#[test]
fn esc_stops_the_deletion() {
    let denied = Undeletable::new();
    let file = VfsPath::local(&denied.file);
    let mut script = helping(&[once(Answer::Cancel)]);

    let report = run(&plan(&file), &mut script);

    assert!(report.cancelled);
    assert_eq!(script.asked.len(), 1);
    assert!(denied.file.exists());
}

#[test]
fn a_helper_that_could_not_start_leaves_the_error() {
    let denied = Undeletable::new();
    let file = VfsPath::local(&denied.file);
    let mut script = helping(&[once(Answer::Elevate), once(Answer::Skip)]);
    script.helper_fails = Some(not_started(io::Error::from_raw_os_error(1223)));

    let report = run(&plan(&file), &mut script);

    let Question::Error { message, .. } = &script.asked[1] else {
        panic!("{:?}", script.asked[1]);
    };
    assert!(message.contains("os error 5"), "its own error: {message}");
    assert_eq!(report.failures.len(), 1);
}

#[test]
fn a_deletion_cancelled_while_the_helper_is_busy_stops() {
    let denied = Undeletable::new();
    let file = VfsPath::local(&denied.file);
    let mut script = helping(&[once(Answer::Elevate)]);
    script.helper_fails = Some(io::ErrorKind::Interrupted.into());

    assert!(run(&plan(&file), &mut script).cancelled);
    assert_eq!(script.asked.len(), 1);
}

#[cfg(windows)]
#[test]
fn the_error_question_names_who_holds_the_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("held.txt");
    fs::write(&path, "x").unwrap();
    let _held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let file = VfsPath::local(&path);
    let mut script = Script::answering(&[once(Answer::Skip)]);

    run(&plan(&file), &mut script);

    let Question::Error { held, .. } = &script.asked[0] else {
        panic!("{:?}", script.asked[0]);
    };
    assert_eq!(held[0], "Object is being opened in:", "{held:?}");
    assert!(
        held.iter().any(|line| line.contains("current process")),
        "{held:?}"
    );
}
