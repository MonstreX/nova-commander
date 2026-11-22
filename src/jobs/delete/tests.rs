//! The deletion engine against FAR's `delete.cpp`: an in-memory tree (`tree://`) for the questions,
//! failures and the Recycle Bin, temporary folders for the real disk.

use std::collections::VecDeque;
use std::io;

use super::*;
use crate::vfs::test_tree::{self as tree, dir, exists, file, link, read_only};

/// Answers the engine's questions from a script and keeps what it asked.
#[derive(Default)]
pub(super) struct Script {
    replies: VecDeque<Reply>,
    pub(super) asked: Vec<Question>,
    progress: Progress,
    /// Cancelled from the start (the user pressed Cancel before the worker began).
    cancelled: bool,
    /// "Access denied" can be offered, and the helper is this script.
    pub(super) elevate: bool,
    /// What reached the helper.
    pub(super) elevated: Vec<crate::vfs::local::elevated::Request>,
    /// What the helper fails with.
    pub(super) helper_fails: Option<io::Error>,
    /// The task's cancel flag, when a test sets it.
    pub(super) stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Done as a question is asked (the tree changed meanwhile).
    on_ask: Option<Box<dyn FnMut()>>,
}

impl Script {
    pub(super) fn answering(replies: &[Reply]) -> Self {
        Self {
            replies: replies.iter().copied().collect(),
            ..Self::default()
        }
    }
}

impl Host for Script {
    fn ask(&mut self, question: Question) -> Reply {
        if let Some(on_ask) = &mut self.on_ask {
            on_ask();
        }
        let reply = self
            .replies
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected question: {question:?}"));
        self.asked.push(question);
        reply
    }

    fn cancelled(&mut self) -> bool {
        self.cancelled
    }

    fn progress(&mut self, change: &mut dyn FnMut(&mut Progress)) {
        change(&mut self.progress);
    }

    fn elevate_available(&self) -> bool {
        self.elevate
    }

    fn stop_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.stop
            .clone()
            .unwrap_or_else(crate::jobs::work::never_stopped)
    }

    fn run_elevated(&mut self, request: &crate::vfs::local::elevated::Request) -> io::Result<()> {
        self.elevated.push(request.clone());
        self.helper_fails.take().map_or(Ok(()), Err)
    }
}

pub(super) fn request(paths: &[&VfsPath], mode: Mode) -> Request {
    Request {
        items: paths
            .iter()
            .map(|path| Item {
                path: (*path).clone(),
                entry: tree::entry_of(path),
            })
            .collect(),
        mode,
        ask_folders: true,
        ask_read_only: true,
        count_totals: false,
    }
}

pub(super) fn once(answer: Answer) -> Reply {
    Reply::once(answer)
}

fn always(answer: Answer) -> Reply {
    Reply::always(answer)
}

/// `tree://<session>/folder` with `a.txt` and `inner/deep.txt`.
fn folder_tree(session: &str) -> (VfsPath, VfsPath, VfsPath) {
    let root = tree::root(session);
    let folder = dir(&root.join("folder"));
    file(&folder.join("a.txt"));
    let inner = dir(&folder.join("inner"));
    let deep = file(&inner.join("deep.txt"));
    (folder, inner, deep)
}

#[test]
fn past_the_bin_a_tree_goes_depth_first_files_before_their_folder() {
    let (folder, inner, deep) = folder_tree("depth");
    let mut request = request(&[&folder], Mode::Remove);
    request.ask_folders = false;
    let mut host = Script::default();

    let report = run(&request, &mut host);

    assert_eq!(report, Report::default());
    assert!(!exists(&folder) && !exists(&inner) && !exists(&deep));
    let removals: Vec<String> = tree::log("depth")
        .into_iter()
        .filter(|line| line.starts_with("remove"))
        .collect();
    assert_eq!(
        removals,
        [
            "remove-file tree://depth/folder/a.txt",
            "remove-file tree://depth/folder/inner/deep.txt",
            "remove-dir tree://depth/folder/inner",
            "remove-dir tree://depth/folder",
        ]
    );
    assert_eq!(host.progress.done, 4);
}

#[test]
fn a_folder_that_is_not_empty_is_asked_about_and_skip_keeps_it_whole() {
    let (folder, _, deep) = folder_tree("skip-folder");
    let mut host = Script::answering(&[once(Answer::Skip)]);

    let report = run(&request(&[&folder], Mode::Remove), &mut host);

    assert_eq!(
        host.asked,
        [Question::NotEmpty {
            path: folder.clone(),
            mode: Mode::Remove
        }]
    );
    assert_eq!(report, Report::default(), "the user's choice is no failure");
    assert!(exists(&deep));
}

#[test]
fn past_the_bin_every_folder_inside_is_asked_about_until_all() {
    let root = tree::root("nested");
    let first = dir(&root.join("first"));
    let a = dir(&first.join("a"));
    file(&a.join("1.txt"));
    let b = dir(&first.join("b"));
    file(&b.join("2.txt"));
    let c = dir(&first.join("c"));
    file(&c.join("3.txt"));
    // first: Delete; a: Delete (once); b: All - c is not asked about.
    let mut host = Script::answering(&[
        once(Answer::Delete),
        once(Answer::Delete),
        always(Answer::Delete),
    ]);

    run(&request(&[&first], Mode::Remove), &mut host);

    let asked: Vec<&Question> = host.asked.iter().collect();
    assert_eq!(asked.len(), 3);
    assert_eq!(
        asked[2],
        &Question::NotEmpty {
            path: b,
            mode: Mode::Remove
        }
    );
    assert!(!exists(&first));
}

#[test]
fn a_folder_inside_that_is_skipped_stays_and_so_does_its_parent_without_an_error() {
    let root = tree::root("skip-inner");
    let top = dir(&root.join("top"));
    let kept = dir(&top.join("kept"));
    let inside = file(&kept.join("inside.txt"));
    let gone = file(&top.join("gone.txt"));
    let mut host = Script::answering(&[once(Answer::Delete), once(Answer::Skip)]);

    let report = run(&request(&[&top], Mode::Remove), &mut host);

    assert_eq!(report, Report::default());
    assert_eq!(
        host.asked.len(),
        2,
        "no 'cannot delete the folder' after the skip"
    );
    assert!(exists(&inside) && exists(&top));
    assert!(!exists(&gone));
}

#[test]
fn an_empty_folder_and_a_file_are_deleted_without_a_folder_question() {
    let root = tree::root("plain");
    let empty = dir(&root.join("empty"));
    let single = file(&root.join("single.txt"));
    let mut host = Script::default();

    run(&request(&[&empty, &single], Mode::Remove), &mut host);

    assert!(!exists(&empty) && !exists(&single));
}

#[test]
fn a_read_only_file_is_asked_about_all_deletes_the_rest_without_asking() {
    let root = tree::root("ro-all");
    let first = read_only(&root.join("1.txt"));
    let second = read_only(&root.join("2.txt"));
    let third = read_only(&root.join("3.txt"));
    let mut host = Script::answering(&[once(Answer::Skip), always(Answer::Delete)]);

    let report = run(
        &request(&[&first, &second, &third], Mode::Remove),
        &mut host,
    );

    assert_eq!(
        host.asked,
        [
            Question::ReadOnly {
                path: first.clone()
            },
            Question::ReadOnly {
                path: second.clone()
            },
        ]
    );
    assert_eq!(report, Report::default());
    assert!(exists(&first));
    assert!(!exists(&second) && !exists(&third));
}

#[test]
fn skip_all_keeps_every_read_only_file_and_the_question_turned_off_deletes_them() {
    let root = tree::root("ro-skip");
    let first = read_only(&root.join("1.txt"));
    let second = read_only(&root.join("2.txt"));
    let mut host = Script::answering(&[always(Answer::Skip)]);
    run(&request(&[&first, &second], Mode::Remove), &mut host);
    assert!(exists(&first) && exists(&second));

    let mut request = request(&[&first, &second], Mode::Remove);
    request.ask_read_only = false;
    run(&request, &mut Script::default());
    assert!(!exists(&first) && !exists(&second));
}

#[test]
fn a_link_to_a_folder_is_removed_as_a_link_and_never_looked_into() {
    let root = tree::root("links");
    let top = dir(&root.join("top"));
    let inner_link = link(&top.join("inner-link"));
    let selected_link = link(&root.join("selected-link"));
    let mut request = request(&[&top, &selected_link], Mode::Remove);
    request.ask_folders = false;

    run(&request, &mut Script::default());

    assert!(!exists(&inner_link) && !exists(&selected_link));
    let log = tree::log("links");
    assert!(log.contains(&"remove-dir tree://links/top/inner-link".to_string()));
    assert!(log.contains(&"remove-dir tree://links/selected-link".to_string()));
    assert!(
        !log.iter()
            .any(|line| line.starts_with("list") && line.ends_with("-link")),
        "{log:?}"
    );
}

#[test]
fn a_failed_removal_is_asked_about_and_retry_tries_again() {
    let root = tree::root("retry");
    let locked = file(&root.join("locked.txt"));
    tree::fail(&locked, io::ErrorKind::PermissionDenied, 1);
    let mut host = Script::answering(&[once(Answer::Retry)]);

    let report = run(&request(&[&locked], Mode::Remove), &mut host);

    assert!(matches!(
        &host.asked[..],
        [Question::Error {
            folder: false,
            kind: io::ErrorKind::PermissionDenied,
            ..
        }]
    ));
    assert_eq!(report, Report::default());
    assert!(!exists(&locked));
}

#[test]
fn skip_reports_the_failure_and_goes_on_skip_all_asks_no_more() {
    let root = tree::root("skip-errors");
    let paths: Vec<VfsPath> = (1..=3)
        .map(|n| file(&root.join(&format!("{n}.txt"))))
        .collect();
    for path in &paths[..2] {
        tree::fail(path, io::ErrorKind::PermissionDenied, 5);
    }
    let failing = file(&root.join("4.txt"));
    tree::fail(&failing, io::ErrorKind::Other, 5);
    let mut host = Script::answering(&[once(Answer::Skip), always(Answer::Skip)]);

    let report = run(
        &request(&[&paths[0], &paths[1], &paths[2], &failing], Mode::Remove),
        &mut host,
    );

    assert_eq!(host.asked.len(), 2, "after Skip all no more questions");
    assert_eq!(report.failures.len(), 3, "{:?}", report.failures);
    assert!(!report.cancelled);
    assert!(exists(&paths[0]) && exists(&paths[1]) && exists(&failing));
    assert!(!exists(&paths[2]));
}

#[test]
fn cancel_stops_at_once_and_leaves_the_rest() {
    let root = tree::root("cancel");
    let first = file(&root.join("1.txt"));
    let second = file(&root.join("2.txt"));
    tree::fail(&first, io::ErrorKind::PermissionDenied, 1);
    let mut host = Script::answering(&[once(Answer::Cancel)]);

    let report = run(&request(&[&first, &second], Mode::Remove), &mut host);

    assert!(report.cancelled);
    assert!(exists(&first) && exists(&second));
}

#[test]
fn a_cancelled_task_deletes_nothing() {
    let (folder, _, deep) = folder_tree("cancelled");
    let mut host = Script {
        cancelled: true,
        ..Script::default()
    };

    let report = run(&request(&[&folder], Mode::Remove), &mut host);

    assert!(report.cancelled);
    assert!(exists(&deep));
}

#[test]
fn an_object_already_gone_is_no_error() {
    let root = tree::root("gone");
    let vanished = root.join("vanished.txt");
    let item = Item {
        path: vanished,
        entry: Entry {
            name: "vanished.txt".into(),
            ..Entry::default()
        },
    };
    let mut request = request(&[], Mode::Remove);
    request.items.push(item);

    let report = run(&request, &mut Script::default());

    assert_eq!(report, Report::default());
}

#[test]
fn a_root_and_the_parent_entry_are_never_deleted() {
    let root = tree::root("roots");
    let kept = file(&root.join("kept.txt"));
    let mut request = request(&[&root], Mode::Remove);
    request.ask_folders = false;
    request.items.push(Item {
        path: root.join(".."),
        entry: Entry::parent(),
    });

    run(&request, &mut Script::default());

    assert!(exists(&root) && exists(&kept));
    assert!(
        !tree::log("roots")
            .iter()
            .any(|line| line.starts_with("remove")),
        "no removal was even tried"
    );
}

#[test]
fn to_the_bin_each_selected_object_goes_in_one_call() {
    let (folder, _, deep) = folder_tree("trash");
    let single = file(&folder.parent().unwrap().join("single.txt"));
    let mut host = Script::answering(&[once(Answer::Delete)]);

    let report = run(&request(&[&folder, &single], Mode::Trash), &mut host);

    assert_eq!(report, Report::default());
    assert_eq!(
        host.asked,
        [Question::NotEmpty {
            path: folder.clone(),
            mode: Mode::Trash
        }]
    );
    assert!(!exists(&folder) && !exists(&deep) && !exists(&single));
    let log = tree::log("trash");
    assert!(
        !log.iter().any(|line| line.starts_with("remove")),
        "{log:?}"
    );
    assert_eq!(
        log.iter().filter(|line| line.starts_with("trash")).count(),
        2
    );
}

#[test]
fn what_the_bin_refuses_is_asked_about_and_delete_removes_it_past_the_bin() {
    let (folder, inner, deep) = folder_tree("refused");
    tree::no_trash(&folder);
    let mut host = Script::answering(&[
        once(Answer::Delete), // the folder is not empty
        once(Answer::Delete), // try to delete permanently
        once(Answer::Delete), // inner is not empty
    ]);

    let report = run(&request(&[&folder], Mode::Trash), &mut host);

    assert_eq!(report, Report::default());
    assert!(matches!(
        host.asked[1],
        Question::CannotTrash { folder: true, .. }
    ));
    assert_eq!(
        host.asked[2],
        Question::NotEmpty {
            path: inner,
            mode: Mode::Remove
        }
    );
    assert!(!exists(&folder) && !exists(&deep));
    let log = tree::log("refused");
    assert_eq!(
        log.iter().filter(|line| line.starts_with("trash")).count(),
        1
    );
}

#[test]
fn what_the_bin_refuses_and_the_user_skips_stays_skip_all_asks_no_more() {
    let root = tree::root("refused-skip");
    let first = file(&root.join("1.txt"));
    let second = file(&root.join("2.txt"));
    tree::no_trash(&first);
    tree::no_trash(&second);
    let mut host = Script::answering(&[always(Answer::Skip)]);

    let report = run(&request(&[&first, &second], Mode::Trash), &mut host);

    assert_eq!(host.asked.len(), 1);
    assert_eq!(report.failures.len(), 2);
    assert!(exists(&first) && exists(&second));
}

#[test]
fn the_deletion_itself_counts_nothing_before_it_deletes() {
    let (folder, _, _) = folder_tree("no-count-first");
    let mut request = request(&[&folder], Mode::Remove);
    request.ask_folders = false;
    request.count_totals = true;
    let mut host = Script::default();

    run(&request, &mut host);

    assert!(!exists(&folder));
    assert_eq!(
        host.progress.total, 0,
        "the count runs beside it, in the job"
    );
    assert_eq!(host.progress.done, 4);
}

#[test]
fn a_selected_folder_replaced_by_a_link_meanwhile_is_removed_as_the_link() {
    let root = tree::root("replaced");
    let top = dir(&root.join("top"));
    let item = Item {
        path: top.clone(),
        entry: tree::entry_of(&top),
    };
    // The task waited in the queue; the folder became a link to somewhere else.
    link(&top);
    let mut request = request(&[], Mode::Remove);
    request.items.push(item);
    request.ask_folders = false;

    run(&request, &mut Script::default());

    assert!(!exists(&top));
    let log = tree::log("replaced");
    assert_eq!(log, ["remove-dir tree://replaced/top"], "never listed");
}

#[test]
fn to_the_bin_a_link_goes_in_one_call_unlisted_and_a_refused_link_is_removed_as_a_link() {
    let root = tree::root("trash-links");
    let taken = link(&root.join("taken"));
    let refused = link(&root.join("refused"));
    tree::no_trash(&refused);
    let mut host = Script::answering(&[once(Answer::Delete)]);

    let report = run(&request(&[&taken, &refused], Mode::Trash), &mut host);

    assert_eq!(report, Report::default());
    assert!(matches!(
        host.asked[..],
        [Question::CannotTrash { folder: true, .. }]
    ));
    assert!(!exists(&taken) && !exists(&refused));
    let log = tree::log("trash-links");
    assert!(
        log.contains(&"remove-dir tree://trash-links/refused".to_string()),
        "{log:?}"
    );
    assert!(!log.iter().any(|line| line.starts_with("list")), "{log:?}");
}

#[test]
fn to_the_bin_a_read_only_file_is_asked_about_too() {
    let root = tree::root("trash-ro");
    let locked = read_only(&root.join("locked.txt"));
    let mut host = Script::answering(&[once(Answer::Delete)]);

    run(&request(&[&locked], Mode::Trash), &mut host);

    assert_eq!(
        host.asked,
        [Question::ReadOnly {
            path: locked.clone()
        }]
    );
    assert!(!exists(&locked));
    assert!(tree::log("trash-ro").contains(&"trash tree://trash-ro/locked.txt".to_string()));
}

#[test]
fn the_bin_saying_not_found_is_an_error_not_done() {
    let root = tree::root("trash-not-found");
    let kept = file(&root.join("kept.txt"));
    tree::fail(&kept, io::ErrorKind::NotFound, 1);
    let mut host = Script::answering(&[once(Answer::Skip)]);

    let report = run(&request(&[&kept], Mode::Trash), &mut host);

    assert!(matches!(host.asked[..], [Question::CannotTrash { .. }]));
    assert_eq!(report.failures.len(), 1);
    assert!(exists(&kept));
}

#[path = "tests_disk.rs"]
mod on_disk;

#[path = "tests_count.rs"]
mod count;

#[test]
fn what_stayed_is_not_finished() {
    let root = tree::root("finished");
    let (gone, kept) = (file(&root.join("1.txt")), read_only(&root.join("2.txt")));
    let mut host = Script::answering(&[once(Answer::Skip)]);

    run(&request(&[&gone, &kept], Mode::Remove), &mut host);

    assert!(exists(&kept));
    assert_eq!(host.progress.finished, [gone]);
}

#[path = "tests_walk.rs"]
mod walk;
