//! The task list: the per-volume queue, ending, pausing and questions.

use super::*;
use crate::jobs::transfer::{Answer, Existing};
use std::fs;
use std::path::Path;

fn copy_request(source: &Path, target: &Path) -> Request {
    Request {
        sources: vec![VfsPath::local(source)],
        target: VfsPath::local(target),
        move_: false,
        existing: Existing::Ask,
        ask_read_only: true,
        keep: crate::jobs::transfer::Keep::default(),
        link: None,
        retrying: Vec::new(),
        filter: None,
    }
}

/// Pumps until `done` or 10 s.
fn pump_until(tasks: &mut TaskList, mut done: impl FnMut(&mut TaskList) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(tasks) {
        assert!(Instant::now() < deadline, "waited too long");
        tasks.pump(Instant::now());
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn status(tasks: &TaskList, id: TaskId) -> Option<Status> {
    tasks.get(id).map(Task::status)
}

#[test]
fn a_task_that_left_something_out_can_give_a_task_for_it_with_its_settings() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("gone.txt");
    let mut request = copy_request(&missing, temp.path());
    request.keep.verify = true;
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let id = tasks.add(request.clone());
    pump_until(&mut tasks, |tasks| {
        status(tasks, id) == Some(Status::Waiting)
    });
    assert!(tasks.question(id).is_some());

    tasks.answer(id, Reply::once(Answer::Skip));
    pump_until(&mut tasks, |tasks| {
        status(tasks, id) == Some(Status::Failed)
    });

    let retry = tasks.get(id).unwrap().retry_request().unwrap();
    assert_eq!(retry.sources, request.sources);
    assert_eq!(retry.retrying.len(), 1);
    assert_eq!(retry.retrying[0].source, request.sources[0]);
    assert!(retry.keep.verify, "the settings come along");
}

#[cfg(windows)]
#[test]
fn a_link_task_is_a_link_and_counts_its_items_without_walking_them() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("dir");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("x.txt"), "x").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let request = Request {
        link: Some(crate::platform::files::NewLink::Junction),
        ..copy_request(&folder, &target)
    };
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let id = tasks.add(request);
    assert_eq!(tasks.get(id).unwrap().kind, Kind::Link);

    pump_until(&mut tasks, |tasks| status(tasks, id) == Some(Status::Done));

    let progress = tasks.get(id).unwrap().progress();
    assert!(progress.totals_known);
    assert_eq!((progress.files_total, progress.files_done), (1, 1));
    assert!(
        fs::symlink_metadata(target.join("dir"))
            .unwrap()
            .is_symlink()
    );
}

#[cfg(windows)]
#[test]
fn retry_skipped_on_a_link_task_makes_links_again() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("dir");
    fs::create_dir(&folder).unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(target.join("dir")).unwrap();
    let request = Request {
        link: Some(crate::platform::files::NewLink::Junction),
        ..copy_request(&folder, &target)
    };
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let id = tasks.add(request);
    pump_until(&mut tasks, |tasks| {
        status(tasks, id) == Some(Status::Waiting)
    });
    tasks.answer(id, Reply::once(Answer::Skip));
    pump_until(&mut tasks, |tasks| {
        status(tasks, id) == Some(Status::Failed)
    });
    fs::remove_dir(target.join("dir")).unwrap();

    let retry = tasks.get(id).unwrap().retry_request().unwrap();
    let again = tasks.add(retry);
    assert_eq!(tasks.get(again).unwrap().kind, Kind::Link);
    pump_until(&mut tasks, |tasks| {
        status(tasks, again) == Some(Status::Done)
    });

    assert!(
        fs::symlink_metadata(target.join("dir"))
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn a_task_with_nothing_left_out_has_nothing_to_retry() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let id = tasks.add(copy_request(&source, &target));
    assert_eq!(
        tasks.get(id).unwrap().retry_request(),
        None,
        "not ended yet"
    );

    pump_until(&mut tasks, |tasks| status(tasks, id) == Some(Status::Done));

    assert_eq!(tasks.get(id).unwrap().retry_request(), None);
}

#[test]
fn a_task_on_a_busy_volume_waits_and_one_on_another_runs_at_once() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("a.txt"), "old").unwrap();
    let other = temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let x = || vec![VolumeId::Root(PathBuf::from("X:\\"))];
    let first = tasks.add_on(copy_request(&source, &target), x());
    let second = tasks.add_on(copy_request(&source, &other), x());
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let third = tasks.add_on(
        copy_request(&source, &elsewhere),
        vec![VolumeId::Root(PathBuf::from("Y:\\"))],
    );

    // The first asks about a.txt and waits: the second stays queued, the third ran.
    pump_until(&mut tasks, |tasks| {
        status(tasks, first) == Some(Status::Waiting)
    });
    pump_until(&mut tasks, |tasks| {
        status(tasks, third) == Some(Status::Done)
    });
    assert_eq!(status(&tasks, second), Some(Status::Queued));
    assert!(!other.join("a.txt").exists());

    assert!(tasks.question(first).is_some());
    tasks.answer(first, Reply::once(Answer::Overwrite));
    pump_until(&mut tasks, |tasks| {
        status(tasks, second) == Some(Status::Done)
    });
    assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "new");
    assert_eq!(fs::read_to_string(other.join("a.txt")).unwrap(), "new");
    assert_eq!(tasks.active(), 0);
}

#[test]
fn tasks_not_stopping_long_after_cancel_all_are_told_and_forgotten_when_none_runs() {
    // Review R12: the question at F10 comes for tasks that still run long after "Cancel all".
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("a.txt"), "old").unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let id = tasks.add(copy_request(&source, &target));
    pump_until(&mut tasks, |tasks| {
        status(tasks, id) == Some(Status::Waiting)
    });
    assert!(!tasks.not_stopping(), "no Cancel all given");
    tasks.cancelled_long_ago();
    assert!(tasks.not_stopping(), "running long after it");
    tasks.forget_stop();
    assert!(!tasks.not_stopping(), "Back: let be");
    tasks.cancelled_long_ago();
    tasks.answer(id, Reply::once(Answer::Skip));
    pump_until(&mut tasks, |tasks| tasks.active() == 0);
    assert!(!tasks.not_stopping(), "none runs: over");
    tasks.cancelled_long_ago();
    tasks.abandon_all();
}

#[test]
fn clean_tasks_leave_after_the_timeout_and_cancelled_ones_stay() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "x").unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let done = tasks.add(copy_request(&source, &temp.path().join("b.txt")));
    let queued = tasks.add(copy_request(&source, &temp.path().join("c.txt")));
    tasks.cancel(queued);
    pump_until(&mut tasks, |tasks| {
        status(tasks, done) == Some(Status::Done)
    });
    assert_eq!(status(&tasks, queued), Some(Status::Cancelled));
    assert!(!temp.path().join("c.txt").exists());

    tasks.pump(Instant::now() + Duration::from_secs(29));
    assert!(tasks.get(done).is_some(), "still within the timeout");
    tasks.pump(Instant::now() + Duration::from_secs(31));
    assert!(tasks.get(done).is_none(), "a clean task left");
    assert!(tasks.get(queued).is_some(), "a cancelled one stays");
    tasks.remove(queued);
    assert!(tasks.is_empty());
}

#[test]
fn a_paused_task_says_so_and_a_question_shows_as_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    fs::write(temp.path().join("b.txt"), "old").unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let id = tasks.add(copy_request(&source, &temp.path().join("b.txt")));
    pump_until(&mut tasks, |tasks| {
        status(tasks, id) == Some(Status::Waiting)
    });
    tasks.set_paused(id, true);
    tasks.answer(id, Reply::once(Answer::Overwrite));
    pump_until(&mut tasks, |tasks| {
        status(tasks, id) == Some(Status::Paused)
    });
    tasks.set_paused(id, false);
    pump_until(&mut tasks, |tasks| status(tasks, id) == Some(Status::Done));
    assert_eq!(
        fs::read_to_string(temp.path().join("b.txt")).unwrap(),
        "new"
    );
}

#[test]
fn what_a_dismissed_task_finished_is_still_read() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let id = tasks.add_on(
        copy_request(&source, &target),
        vec![VolumeId::Root(PathBuf::from("X:\\"))],
    );
    pump_until(&mut tasks, |tasks| {
        tasks
            .get(id)
            .is_some_and(|task| !task.progress().finished.is_empty())
    });

    tasks.dismiss(id);
    pump_until(&mut tasks, |tasks| tasks.get(id).is_none());

    assert_eq!(
        tasks.take_finished_objects(),
        [(id, VfsPath::local(&source))]
    );
}

#[test]
fn a_change_of_attributes_runs_at_once_beside_a_copy_waiting_on_its_disk() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("a.txt");
    fs::write(&source, "new").unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("a.txt"), "old").unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let copy = tasks.add(copy_request(&source, &target));
    pump_until(&mut tasks, |tasks| {
        status(tasks, copy) == Some(Status::Waiting)
    });
    let mut wanted = [None; 20];
    wanted[2] = Some(true);
    let request = attributes::Request {
        items: vec![source.clone()],
        wanted,
        owner: String::new(),
        times: std::array::from_fn(|_| attributes::TimeEdit::default()),
        utc: false,
        format: attributes::times::TimeFormat::current(),
        subfolders: false,
        link_target: None,
    };
    let id = tasks.add_attributes(request);
    assert_eq!(tasks.get(id).map(|task| task.kind), Some(Kind::Attributes));
    pump_until(&mut tasks, |tasks| status(tasks, id) == Some(Status::Done));
    let attributes = crate::platform::files::file_properties(&source)
        .unwrap()
        .attributes;
    assert!(attributes.has(crate::platform::files::Flag::Hidden));
    tasks.cancel(copy);
}

#[test]
#[cfg(windows)]
fn a_task_puts_what_it_did_into_the_trees_file() {
    use crate::jobs::tree_cache::TreeCache;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("disk");
    for folder in ["src/x", "dst"] {
        fs::create_dir_all(root.join(folder)).unwrap();
    }
    let cache = TreeCache::under(temp.path().join("TreeCache"), root.clone());
    let known = [
        root.join("src"),
        root.join("src").join("x"),
        root.join("dst"),
    ];
    cache.save(&root, &known, 1).unwrap();
    let mut tasks = TaskList::new(Duration::from_secs(30));
    tasks.tree_cache = Some(cache.clone());
    let id = tasks.add(copy_request(&root.join("src"), &root.join("dst")));
    pump_until(&mut tasks, |tasks| status(tasks, id) == Some(Status::Done));
    let read = cache.read(&root).unwrap();
    assert!(
        read.contains(&root.join("dst").join("src").join("x")),
        "{read:?}"
    );
}

/// R7 review B7: a plugin let go stops the copies and moves on its panels (a new instance would take
/// their names); the others go on.
#[test]
fn a_plugin_let_go_stops_its_tasks_only() {
    let temp = tempfile::tempdir().unwrap();
    let mut remote = copy_request(temp.path(), temp.path());
    remote.sources = vec![VfsPath::in_session("netway", "r1", "/dir")];
    remote.move_ = true;
    let local = copy_request(&temp.path().join("a.txt"), temp.path());
    let mut tasks = TaskList::new(Duration::from_secs(30));
    let (theirs, ours) = (tasks.add(remote), tasks.add(local));
    let before = status(&tasks, ours);
    tasks.cancel_on_provider("NetWay");
    assert_ne!(status(&tasks, theirs), before, "stopped");
    assert_eq!(status(&tasks, ours), before, "goes on");
}
