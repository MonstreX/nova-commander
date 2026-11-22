//! "Set" as the task runs it: FAR's order of the changes, each failure asked about, Skip / Skip all /
//! Retry / Cancel, the selected objects reported as they are done (only in folders the tests make).

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::times::TimeFormat;
use super::*;
use crate::platform::files::{self, Flag};
use crate::platform::locale::DateOrder;

const DMY: TimeFormat = TimeFormat {
    order: DateOrder::DayMonthYear,
    date_separator: '.',
    time_separator: ':',
    decimal_separator: '.',
};

/// An owner no computer has: setting it always fails.
const NOBODY: &str = "no-such-account-6f1c";

/// Answers from a script, and what was asked. "Access denied" is offered only when `elevate` is
/// given: then the helper's part is `elevate`'s answer to each request it is handed.
#[derive(Default)]
pub(super) struct Script {
    pub(super) replies: VecDeque<Reply>,
    pub(super) asked: Vec<Question>,
    pub(super) progress: Progress,
    pub(super) cancel_after: Option<usize>,
    pub(super) elevate: Option<fn(&crate::vfs::local::elevated::Request) -> std::io::Result<()>>,
    pub(super) elevated: Vec<crate::vfs::local::elevated::Request>,
}

impl Host for Script {
    fn elevate_available(&self) -> bool {
        self.elevate.is_some()
    }

    fn run_elevated(
        &mut self,
        request: &crate::vfs::local::elevated::Request,
    ) -> std::io::Result<()> {
        self.elevated.push(request.clone());
        let elevate = self
            .elevate
            .ok_or_else(|| std::io::Error::other("no helper"))?;
        elevate(request)
    }

    fn ask(&mut self, question: Question) -> Reply {
        self.asked.push(question);
        self.replies.pop_front().unwrap_or(Reply::Cancel)
    }

    fn cancelled(&mut self) -> bool {
        self.cancel_after
            .is_some_and(|after| self.progress.finished.len() >= after)
    }

    fn progress(&mut self, change: &mut dyn FnMut(&mut Progress)) {
        change(&mut self.progress);
    }
}

fn at(flag: Flag) -> usize {
    Flag::ALL.iter().position(|f| *f == flag).unwrap()
}

/// Hidden on, for `paths`, with `owner`.
pub(super) fn hide(paths: &[&Path], owner: &str) -> Request {
    let mut wanted = [None; 20];
    wanted[at(Flag::Hidden)] = Some(true);
    Request {
        items: paths.iter().map(|path| path.to_path_buf()).collect(),
        wanted,
        owner: owner.to_string(),
        times: std::array::from_fn(|_| TimeEdit::default()),
        utc: false,
        format: DMY,
        subfolders: false,
        link_target: None,
    }
}

fn two_files() -> (tempfile::TempDir, [std::path::PathBuf; 2]) {
    let temp = tempfile::tempdir().unwrap();
    let paths = ["a.txt", "b.txt"].map(|name| temp.path().join(name));
    for path in &paths {
        std::fs::write(path, "x").unwrap();
    }
    (temp, paths)
}

pub(super) fn hidden(path: &Path) -> bool {
    files::file_properties(path)
        .unwrap()
        .attributes
        .has(Flag::Hidden)
}

#[test]
fn the_changes_go_in_fars_order() {
    let (_temp, [path, _]) = two_files();
    let mut properties = files::file_properties(&path).unwrap();
    properties.attributes = properties.attributes.with(Flag::ReparsePoint, true);
    let mut wanted = [None; 20];
    for flag in [
        Flag::Hidden,
        Flag::Compressed,
        Flag::Encrypted,
        Flag::Sparse,
    ] {
        wanted[at(flag)] = Some(true);
    }
    wanted[at(Flag::ReparsePoint)] = Some(false);
    let later = SystemTime::now() + Duration::from_secs(60);
    let planned = changes(&properties, &wanted, "SOMEONE\\else", |_, _| Some(later));
    let kinds: Vec<&str> = planned.iter().map(failure).collect();
    assert_eq!(
        kinds,
        [
            "Cannot set owner for",
            "Cannot set attribute COMPRESSED for",
            "Cannot set attribute ENCRYPTED for",
            "Cannot set attribute SPARSE for",
            "Cannot remove reparse point for",
            "Cannot set attributes for",
            "Cannot set file time for",
        ]
    );
}

#[test]
fn nothing_asked_for_changes_nothing() {
    let (_temp, [path, _]) = two_files();
    let properties = files::file_properties(&path).unwrap();
    let owner = properties.owner.clone().unwrap_or_default();
    assert_eq!(changes(&properties, &[None; 20], &owner, |_, own| own), []);
}

#[test]
fn skip_leaves_out_that_change_and_goes_on_with_the_next() {
    let (_temp, [a, b]) = two_files();
    let mut host = Script {
        replies: VecDeque::from([Reply::Skip, Reply::Skip]),
        ..Script::default()
    };
    let report = run(&hide(&[&a, &b], NOBODY), &mut host);

    assert_eq!(host.asked.len(), 2, "each object's owner is asked about");
    assert!(
        matches!(&host.asked[0], Question::Failed { failure: "Cannot set owner for", path, .. } if *path == a),
        "{:?}",
        host.asked[0]
    );
    assert!(hidden(&a) && hidden(&b), "the next change was made");
    assert_eq!(report.failures.len(), 2);
    assert!(report.failures[0].starts_with("Cannot set owner for"));
    assert!(!report.cancelled);
    let done: Vec<VfsPath> = [&a, &b].map(VfsPath::local).to_vec();
    assert_eq!(host.progress.finished, done);
}

#[test]
fn skip_all_asks_no_more() {
    let (_temp, [a, b]) = two_files();
    let mut host = Script {
        replies: VecDeque::from([Reply::SkipAll]),
        ..Script::default()
    };
    let report = run(&hide(&[&a, &b], NOBODY), &mut host);
    assert_eq!(host.asked.len(), 1);
    assert_eq!(report.failures.len(), 2);
    assert!(hidden(&b));
}

#[test]
fn retry_tries_the_change_again() {
    let (_temp, [a, _]) = two_files();
    let mut host = Script {
        replies: VecDeque::from([Reply::Retry, Reply::Skip]),
        ..Script::default()
    };
    let report = run(&hide(&[&a], NOBODY), &mut host);
    assert_eq!(host.asked.len(), 2, "the same change failed twice");
    assert_eq!(host.asked[0], host.asked[1]);
    assert_eq!(report.failures.len(), 1);
}

#[test]
fn cancel_in_the_question_stops_the_task() {
    let (_temp, [a, b]) = two_files();
    let mut host = Script::default();
    let report = run(&hide(&[&a, &b], NOBODY), &mut host);
    assert!(report.cancelled);
    assert!(
        !hidden(&a) && !hidden(&b),
        "nothing after the owner was made"
    );
    assert!(host.progress.finished.is_empty());
}

#[test]
fn a_cancelled_task_leaves_the_rest() {
    let (_temp, [a, b]) = two_files();
    let mut host = Script {
        cancel_after: Some(1),
        ..Script::default()
    };
    let report = run(&hide(&[&a, &b], ""), &mut host);
    assert!(report.cancelled);
    assert!(hidden(&a) && !hidden(&b));
    assert_eq!(
        host.progress.finished,
        [VfsPath::local(&a)],
        "only its mark goes"
    );
}

/// `link` made a junction to `target`.
fn junction(target: &Path, link: &Path) {
    files::make_link(files::NewLink::Junction, target, link).unwrap();
}

#[test]
fn subfolders_go_through_the_tree_and_into_links_once_per_place() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(root.join("inner")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(root.join("inner").join("deep.txt"), "d").unwrap();
    std::fs::write(outside.join("far.txt"), "f").unwrap();
    junction(&outside, &root.join("to-outside"));
    junction(&outside, &root.join("inner").join("again"));
    junction(&root, &root.join("inner").join("back-up"));
    let mut request = hide(&[&root], "");
    request.subfolders = true;
    let mut host = Script::default();
    let report = run(&request, &mut host);

    assert_eq!(report, Report::default());
    for path in [
        root.clone(),
        root.join("inner").join("deep.txt"),
        root.join("to-outside"),
        outside.join("far.txt"),
    ] {
        assert!(hidden(&path), "{}", path.display());
    }
    assert!(
        !hidden(&outside),
        "a link's target folder itself is not one of the objects"
    );
    assert_eq!(host.progress.finished, [VfsPath::local(&root)]);
}

#[test]
fn without_subfolders_a_folder_alone_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("in.txt"), "i").unwrap();
    run(&hide(&[&root], ""), &mut Script::default());
    assert!(hidden(&root) && !hidden(&root.join("in.txt")));
}

#[test]
fn a_typed_time_takes_its_blank_parts_from_each_objects_own() {
    let (_temp, [a, b]) = two_files();
    let day = Duration::from_secs(86_400);
    let a_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    for (path, time) in [(&a, a_time), (&b, a_time + day)] {
        let times = files::FileTimes {
            modified: Some(time),
            ..files::FileTimes::default()
        };
        files::set_file_times(path, &times).unwrap();
    }
    let mut request = hide(&[&a, &b], "");
    request.wanted = [None; 20];
    request.utc = true;
    request.times[0] = TimeEdit {
        date: "  .  .     ".to_string(),
        time: "23:  :  .       ".to_string(),
        as_shown: false,
    };
    run(&request, &mut Script::default());

    let shown = |path: &Path| {
        let modified = files::file_properties(path).unwrap().times.modified;
        times::show(modified, true, DMY)
    };
    let (a_date, a_clock) = shown(&a);
    let (b_date, b_clock) = shown(&b);
    assert_eq!(a_date, times::show(Some(a_time), true, DMY).0);
    assert_eq!(b_date, times::show(Some(a_time + day), true, DMY).0);
    assert!(a_clock.starts_with("23:") && b_clock.starts_with("23:"));
    assert_eq!(a_clock[2..], b_clock[2..], "the minutes and seconds kept");
}

#[test]
fn a_line_as_shown_keeps_the_folders_time_and_gives_it_to_what_is_inside() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let inside = root.join("in.txt");
    std::fs::write(&inside, "i").unwrap();
    let folder_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_400_000_000);
    let times = files::FileTimes {
        modified: Some(folder_time),
        ..files::FileTimes::default()
    };
    files::set_file_times(&root, &times).unwrap();
    let (date, time) = times::show(Some(folder_time), true, DMY);
    let mut request = hide(&[&root], "");
    request.wanted = [None; 20];
    request.utc = true;
    request.subfolders = true;
    request.times[0] = TimeEdit {
        date,
        time,
        as_shown: true,
    };
    run(&request, &mut Script::default());

    let modified = |path: &Path| files::file_properties(path).unwrap().times.modified;
    assert_eq!(
        modified(&inside),
        Some(folder_time),
        "FAR gives the line to each object"
    );
    assert_eq!(modified(&root), Some(folder_time));
}

#[test]
fn a_folder_a_link_leads_back_to_is_gone_through_once() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir_all(root.join("inner")).unwrap();
    std::fs::write(root.join("inner").join("deep.txt"), "d").unwrap();
    junction(&root.join("inner"), &root.join("x"));
    let mut request = hide(&[&root], NOBODY);
    request.subfolders = true;
    let mut host = Script {
        replies: std::iter::repeat_n(Reply::Skip, 20).collect(),
        ..Script::default()
    };
    run(&request, &mut host);
    let deep = (host.asked.iter())
        .filter(|question| matches!(question, Question::Failed { path, .. } if path.ends_with("deep.txt")))
        .count();
    assert_eq!(deep, 1, "{:?}", host.asked);
}
