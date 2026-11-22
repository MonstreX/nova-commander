//! The engine against temporary trees, with a scripted host answering its questions (ADR-0009).

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::*;

type OnProgress = Box<dyn FnMut(&Progress)>;

/// Answers questions from a list, cancels at the n-th look at the cancel flag, may claim little room
/// or that the target is on another volume.
#[derive(Default)]
pub(super) struct Script {
    pub(super) replies: VecDeque<Reply>,
    pub(super) asked: Vec<Question>,
    pub(super) cancel_at: Option<usize>,
    pub(super) looks: usize,
    pub(super) free: Option<u64>,
    /// Renames fail as across volumes: a move copies and deletes.
    pub(super) cross_volume: bool,
    /// Renames fail for want of rights.
    pub(super) deny_moves: bool,
    /// "Access denied" can be offered, and the helper is this script.
    pub(super) elevate: bool,
    /// What reached the helper; a move is carried out.
    pub(super) elevated: Vec<crate::vfs::local::elevated::Request>,
    /// What the helper fails with, by request.
    pub(super) helper_fails:
        Option<fn(&crate::vfs::local::elevated::Request) -> Option<std::io::Error>>,
    /// Runs before each answer (another program lets a file go).
    pub(super) on_ask: Option<Box<dyn FnMut()>>,
    /// Runs at each change of the progress (something happens to a file between two steps).
    pub(super) on_progress: Option<OnProgress>,
    pub(super) progress: Progress,
}

impl Script {
    pub(super) fn answering(replies: &[Reply]) -> Self {
        Self {
            replies: replies.iter().cloned().collect(),
            ..Self::default()
        }
    }
}

impl Host for Script {
    fn ask(&mut self, question: Question) -> Reply {
        self.asked.push(question.clone());
        if let Some(on_ask) = &mut self.on_ask {
            on_ask();
        }
        self.replies
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected question {question:?}"))
    }

    fn cancelled(&mut self) -> bool {
        self.looks += 1;
        self.cancel_at.is_some_and(|at| self.looks >= at)
    }

    fn progress(&mut self, change: &mut dyn FnMut(&mut Progress)) {
        change(&mut self.progress);
        if let Some(on_progress) = &mut self.on_progress {
            on_progress(&self.progress);
        }
    }

    fn free_space(&mut self, dir: &Path) -> Option<u64> {
        self.free
            .or_else(|| crate::platform::volumes::free_space(dir))
    }

    fn move_item(&mut self, from: &Path, to: &Path, replace: bool) -> std::io::Result<()> {
        // The temporary copy lies next to its target: putting it in place stays on one volume.
        let temporary = from.to_string_lossy().ends_with(".nc-tmp");
        if self.cross_volume && !temporary {
            return Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices));
        }
        if self.deny_moves {
            return Err(std::io::Error::from_raw_os_error(5));
        }
        crate::platform::files::move_file(from, to, replace)
    }

    fn elevate_available(&self) -> bool {
        self.elevate
    }

    fn run_elevated(
        &mut self,
        request: &crate::vfs::local::elevated::Request,
        _base: Option<u64>,
    ) -> std::io::Result<()> {
        self.elevated.push(request.clone());
        if let Some(error) = self.helper_fails.and_then(|fails| fails(request)) {
            return Err(error);
        }
        if let crate::vfs::local::elevated::Request::MoveFile { from, to, replace } = request {
            return crate::platform::files::move_file(from, to, *replace);
        }
        Ok(())
    }
}

pub(super) fn request(
    sources: &[PathBuf],
    target: &Path,
    move_: bool,
    existing: Existing,
) -> Request {
    Request {
        sources: sources.iter().map(crate::vfs::VfsPath::local).collect(),
        target: crate::vfs::VfsPath::local(target),
        move_,
        existing,
        ask_read_only: true,
        keep: Keep::default(),
        link: None,
        retrying: Vec::new(),
        filter: None,
    }
}

pub(super) fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

pub(super) fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// `root/tree` with `a.txt`, `b.txt`, `inner/c.txt`, and an empty `target` folder.
pub(super) fn setup(root: &Path) -> (PathBuf, PathBuf) {
    let tree = root.join("tree");
    fs::create_dir_all(tree.join("inner")).unwrap();
    fs::write(tree.join("a.txt"), "a-new").unwrap();
    fs::write(tree.join("b.txt"), "b-new").unwrap();
    fs::write(tree.join("inner").join("c.txt"), "c-new").unwrap();
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    (tree, target)
}

#[test]
fn a_tree_is_copied_and_every_file_counted() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let mut host = Script::default();
    let report = run(
        &request(std::slice::from_ref(&tree), &target, false, Existing::Ask),
        &mut host,
    );
    assert_eq!(report, Report::default());
    assert_eq!(
        read(&target.join("tree").join("inner").join("c.txt")),
        "c-new"
    );
    assert_eq!(host.progress.files_done, 3);
    assert_eq!(host.progress.bytes_done, 15);
    assert!(tree.join("a.txt").exists(), "the source stays");
}

#[test]
fn ask_overwrite_replaces_through_a_temporary_file_and_merges_folders() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::create_dir_all(target.join("tree")).unwrap();
    fs::write(target.join("tree").join("a.txt"), "a-old").unwrap();
    fs::write(target.join("tree").join("kept.txt"), "kept").unwrap();
    let mut host = Script::answering(&[Reply::once(Answer::Overwrite)]);
    let report = run(&request(&[tree], &target, false, Existing::Ask), &mut host);
    assert!(report.failures.is_empty(), "{report:?}");
    let merged = target.join("tree");
    assert_eq!(read(&merged.join("a.txt")), "a-new");
    assert_eq!(names(&merged), ["a.txt", "b.txt", "inner", "kept.txt"]);
    let Question::Exists { source, target, .. } = &host.asked[0] else {
        panic!("{:?}", host.asked);
    };
    assert_eq!((source.size, target.size), (5, 5));
}

#[test]
fn remember_choice_answers_the_rest() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    for name in ["a.txt", "b.txt"] {
        fs::write(target.join(name), "old").unwrap();
    }
    let sources = [tree.join("a.txt"), tree.join("b.txt")];
    let mut host = Script::answering(&[Reply::always(Answer::Skip)]);
    run(&request(&sources, &target, false, Existing::Ask), &mut host);
    assert_eq!(host.asked.len(), 1);
    assert_eq!(read(&target.join("b.txt")), "old");
    assert_eq!(host.progress.files_done, 2, "skipped files count as done");
}

#[test]
fn rename_takes_the_first_free_number() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    fs::write(target.join("a (2).txt"), "old 2").unwrap();
    let mut host = Script::default();
    run(
        &request(&[tree.join("a.txt")], &target, false, Existing::Rename),
        &mut host,
    );
    assert_eq!(read(&target.join("a (3).txt")), "a-new");
    assert_eq!(read(&target.join("a.txt")), "old");
}

#[test]
fn only_newer_overwrites_older_files_and_skips_newer_ones() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let set_time = |path: &Path, time: SystemTime| {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    };
    let now = SystemTime::now();
    let hour = Duration::from_secs(3600);
    fs::write(target.join("a.txt"), "older").unwrap();
    set_time(&target.join("a.txt"), now - hour);
    fs::write(target.join("b.txt"), "newer").unwrap();
    set_time(&target.join("b.txt"), now + hour);
    let sources = [tree.join("a.txt"), tree.join("b.txt")];
    run(
        &request(&sources, &target, false, Existing::OnlyNewer),
        &mut Script::default(),
    );
    assert_eq!(read(&target.join("a.txt")), "a-new");
    assert_eq!(read(&target.join("b.txt")), "newer");
}

#[test]
fn append_adds_to_the_end() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old+").unwrap();
    run(
        &request(&[tree.join("a.txt")], &target, false, Existing::Append),
        &mut Script::default(),
    );
    assert_eq!(read(&target.join("a.txt")), "old+a-new");
}

#[test]
fn a_read_only_file_is_replaced_only_after_its_question() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let old = target.join("a.txt");
    fs::write(&old, "old").unwrap();
    let mut permissions = fs::metadata(&old).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&old, permissions).unwrap();
    let sources = [tree.join("a.txt")];

    let mut host = Script::answering(&[Reply::once(Answer::Skip)]);
    run(
        &request(&sources, &target, false, Existing::Overwrite),
        &mut host,
    );
    assert!(matches!(host.asked[..], [Question::ReadOnly { .. }]));
    assert_eq!(read(&old), "old");

    let mut host = Script::answering(&[Reply::once(Answer::Overwrite)]);
    let report = run(
        &request(&sources, &target, false, Existing::Overwrite),
        &mut host,
    );
    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(read(&old), "a-new");
}

#[test]
fn a_cancelled_overwrite_keeps_the_old_file_and_leaves_no_temporary_one() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("big.bin");
    fs::write(&source, vec![5u8; 4_000_000]).unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("big.bin"), "old").unwrap();
    // The first look is before the item, the second the copy's first progress call.
    let mut host = Script {
        cancel_at: Some(2),
        ..Script::default()
    };
    let report = run(
        &request(&[source], &target, false, Existing::Overwrite),
        &mut host,
    );
    assert!(report.cancelled);
    assert_eq!(read(&target.join("big.bin")), "old");
    assert_eq!(names(&target), ["big.bin"]);
}

#[test]
fn without_room_for_both_in_place_is_asked_and_without_room_at_all_nothing_is_deleted() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let old = target.join("a.txt");
    fs::write(&old, "old-old").unwrap();
    let sources = [tree.join("a.txt")];

    // 5 bytes needed, 1 free, 7 in the old file: in place is offered.
    let mut host = Script {
        free: Some(1),
        ..Script::answering(&[Reply::once(Answer::InPlace)])
    };
    run(
        &request(&sources, &target, false, Existing::Overwrite),
        &mut host,
    );
    let [
        Question::LowSpace {
            new_size: 5,
            old_size: 7,
            free: 1,
            ..
        },
    ] = host.asked[..]
    else {
        panic!("{:?}", host.asked);
    };
    assert_eq!(read(&old), "a-new");

    // 5 needed, 1 free, 2 in the old file: an error, the old file stays.
    fs::write(&old, "ol").unwrap();
    let mut host = Script {
        free: Some(1),
        ..Script::answering(&[Reply::once(Answer::Skip)])
    };
    let report = run(
        &request(&sources, &target, false, Existing::Overwrite),
        &mut host,
    );
    assert!(matches!(
        host.asked[..],
        [Question::Error { retry: false, .. }]
    ));
    assert_eq!(read(&old), "ol");
    assert_eq!(report.failures.len(), 1);
}

#[test]
fn a_move_into_a_folder_merges_and_every_file_stays_in_one_place() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::create_dir_all(target.join("tree")).unwrap();
    fs::write(target.join("tree").join("b.txt"), "b-old").unwrap();
    let report = run(
        &request(std::slice::from_ref(&tree), &target, true, Existing::Skip),
        &mut Script::default(),
    );
    assert!(report.failures.is_empty(), "{report:?}");
    let moved = target.join("tree");
    assert_eq!(read(&moved.join("a.txt")), "a-new");
    assert_eq!(read(&moved.join("inner").join("c.txt")), "c-new");
    assert_eq!(read(&moved.join("b.txt")), "b-old");
    assert_eq!(
        names(&tree),
        ["b.txt"],
        "the skipped file stays, the rest left"
    );
}

#[test]
fn a_move_with_overwrite_replaces_the_target_with_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    run(
        &request(&[tree.join("a.txt")], &target, true, Existing::Overwrite),
        &mut Script::default(),
    );
    assert_eq!(read(&target.join("a.txt")), "a-new");
    assert!(!tree.join("a.txt").exists());
}

#[test]
fn a_case_only_rename_is_done_and_copy_onto_itself_or_into_itself_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, _) = setup(temp.path());
    let file = tree.join("a.txt");
    let report = run(
        &request(
            std::slice::from_ref(&file),
            &file,
            false,
            Existing::Overwrite,
        ),
        &mut Script::default(),
    );
    assert_eq!(report.failures.len(), 1);
    assert_eq!(read(&file), "a-new");

    let report = run(
        &request(
            std::slice::from_ref(&tree),
            &tree.join("inner"),
            false,
            Existing::Ask,
        ),
        &mut Script::default(),
    );
    assert_eq!(report.failures.len(), 1);
    assert!(!tree.join("inner").join("tree").exists());

    run(
        &request(&[file], &tree.join("A.TXT"), true, Existing::Ask),
        &mut Script::default(),
    );
    assert!(names(&tree).contains(&"A.TXT".to_string()));
}

#[cfg(windows)]
#[test]
fn a_failed_file_asks_and_skip_all_answers_the_rest() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    // Another program holding files without sharing.
    let lock = |path: &Path| {
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    };
    let locks = [lock(&tree.join("a.txt")), lock(&tree.join("b.txt"))];
    let mut host = Script::answering(&[Reply::always(Answer::Skip)]);
    let report = run(&request(&[tree], &target, false, Existing::Ask), &mut host);
    drop(locks);
    assert_eq!(host.asked.len(), 1);
    assert_eq!(report.failures.len(), 2);
    assert_eq!(
        read(&target.join("tree").join("inner").join("c.txt")),
        "c-new"
    );
    assert!(!target.join("tree").join("a.txt").exists());
}

#[test]
fn a_job_runs_on_its_thread_and_takes_answers() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    let mut job = Job::start(
        request(
            &[tree.join("a.txt"), tree.join("b.txt")],
            &target,
            false,
            Existing::Ask,
        ),
        None,
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let report = loop {
        assert!(std::time::Instant::now() < deadline, "the job did not end");
        if job.question().is_some() {
            job.answer(Reply::once(Answer::Overwrite));
        }
        if let Some(report) = job.finished() {
            break report;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(report, Report::default());
    assert_eq!(read(&target.join("a.txt")), "a-new");
    assert_eq!(job.progress().files_done, 2);
}

#[test]
fn a_cancelled_job_waiting_for_an_answer_ends() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    fs::write(target.join("a.txt"), "old").unwrap();
    let mut job = Job::start(
        request(&[tree.join("a.txt")], &target, false, Existing::Ask),
        None,
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while job.question().is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    job.cancel();
    let report = loop {
        if let Some(report) = job.finished() {
            break report;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(report.cancelled);
    assert_eq!(read(&target.join("a.txt")), "old");
}

/// Directory junctions need no special privileges, unlike symbolic links.
pub(super) fn make_junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap()
        .status;
    assert!(status.success());
}

#[test]
fn links_are_copied_as_links_as_far_does() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::write(elsewhere.join("big.bin"), "not copied").unwrap();
    make_junction(&tree.join("zz-link"), &elsewhere);

    let report = run(
        &request(std::slice::from_ref(&tree), &target, false, Existing::Ask),
        &mut Script::default(),
    );

    assert!(report.failures.is_empty(), "{report:?}");
    let link = target.join("tree").join("zz-link");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        crate::platform::files::link_target(&link).unwrap(),
        elsewhere
    );
    assert_eq!(names(&elsewhere), ["big.bin"], "nothing was copied into it");
    // A link alone, also into the folder it points to.
    run(
        &request(&[tree.join("zz-link")], &elsewhere, false, Existing::Ask),
        &mut Script::default(),
    );
    let alone = elsewhere.join("zz-link");
    assert_eq!(
        crate::platform::files::link_target(&alone).unwrap(),
        elsewhere
    );
}

#[test]
fn a_move_renames_files_and_folders_and_never_into_themselves() {
    let temp = tempfile::tempdir().unwrap();
    let (tree, target) = setup(temp.path());
    run(
        &request(
            &[tree.join("a.txt")],
            &tree.join("renamed.txt"),
            true,
            Existing::Ask,
        ),
        &mut Script::default(),
    );
    assert_eq!(read(&tree.join("renamed.txt")), "a-new");
    assert!(!tree.join("a.txt").exists());

    let report = run(
        &request(
            std::slice::from_ref(&tree),
            &tree.join("inner"),
            true,
            Existing::Ask,
        ),
        &mut Script::default(),
    );
    assert_eq!(report.failures.len(), 1);
    assert!(tree.join("b.txt").exists());

    run(
        &request(std::slice::from_ref(&tree), &target, true, Existing::Ask),
        &mut Script::default(),
    );
    assert!(!tree.exists());
    assert_eq!(
        read(&target.join("tree").join("inner").join("c.txt")),
        "c-new"
    );
}
