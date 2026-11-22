//! The tree's worker (ADR-0014): every folder read, the most wanted first, a folder read before listed
//! again only when its time changed, its file kept. Every tree here has a test's own folder as its root.

use std::fs;

use super::*;

fn temp_tree(folders: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("disk");
    fs::create_dir(&root).unwrap();
    for folder in folders {
        fs::create_dir_all(root.join(folder)).unwrap();
    }
    (temp, root)
}

fn relative(root: &Path, path: &Path) -> String {
    let rest = path.strip_prefix(root).unwrap();
    rest.to_string_lossy().replace('\\', "/")
}

fn names(root: &Path, items: &[TreeItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| relative(root, &item.path))
        .collect()
}

/// Works until nothing is left; the folders listed, in their order.
fn finish(work: &mut Work) -> Vec<String> {
    let stop = AtomicBool::new(false);
    let root = work.nodes.root().to_path_buf();
    let mut order = Vec::new();
    while let Some((folder, force)) = work.next() {
        let before = work.listed;
        work.visit(&folder, force, &stop);
        if work.listed > before {
            order.push(relative(&root, &folder));
        }
    }
    order
}

/// The worker's snapshots until it is done reading.
fn done(worker: &TreeWorker) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last = None;
    loop {
        if let Some(snapshot) = worker.latest() {
            last = Some(snapshot);
        }
        if last.as_ref().is_some_and(|snapshot| !snapshot.reading) {
            return last.unwrap();
        }
        assert!(Instant::now() < deadline, "the tree was not read");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn every_folder_under_the_root_in_the_trees_order_and_no_file() {
    let (_temp, root) = temp_tree(&["a/b/c", "a/d", "e"]);
    fs::write(root.join("a").join("file.txt"), "x").unwrap();
    let worker = TreeWorker::start(root.clone(), None);
    let snapshot = done(&worker);
    assert_eq!(
        names(&root, &snapshot.items),
        ["", "a", "a/b", "a/b/c", "a/d", "e"]
    );
    assert_eq!(snapshot.count, 5);
}

#[test]
fn the_other_panels_folder_and_its_subtree_are_read_before_the_rest() {
    let (_temp, root) = temp_tree(&["a/a1/a2", "b/b1", "x/new/deep", "z/z1"]);
    let mut work = Work::new(root.clone(), None);
    work.take(Order::Follow(root.join("x").join("new")));
    let order = finish(&mut work);
    let at = |name: &str| order.iter().position(|listed| listed == name).unwrap();
    assert_eq!(&order[..4], ["", "x", "x/new", "x/new/deep"], "{order:?}");
    assert!(at("x/new/deep") < at("a"), "{order:?}");
    assert_eq!(order.len(), 11, "then every folder: {order:?}");
}

#[test]
fn the_folder_under_the_cursor_comes_first() {
    let (_temp, root) = temp_tree(&["a/a1", "b/b1", "c/c1"]);
    let mut work = Work::new(root.clone(), None);
    let stop = AtomicBool::new(false);
    let (folder, force) = work.next().unwrap();
    work.visit(&folder, force, &stop);
    work.take(Order::Focus(root.join("c")));
    let order = finish(&mut work);
    assert_eq!(order[0], "c", "{order:?}");
}

#[test]
fn a_folder_read_in_through_another_case_joins_its_branch() {
    let (_temp, root) = temp_tree(&["Work/new/deep"]);
    let mut work = Work::new(root.clone(), None);
    work.take(Order::Follow(root.join("work").join("new")));
    finish(&mut work);
    let items = work.snapshot(false).items;
    let depths: Vec<usize> = items.iter().map(|item| item.depth).collect();
    assert_eq!(depths, [0, 1, 2, 3], "one Work, new and deep in it");
    assert!(items[1].path.ends_with("Work"), "the disk's name");
}

#[test]
fn a_folder_gone_leaves_with_what_was_under_it_and_check_lists_again() {
    let (_temp, root) = temp_tree(&["a/b/c", "d"]);
    let mut work = Work::new(root.clone(), None);
    finish(&mut work);
    fs::remove_dir_all(root.join("a")).unwrap();
    fs::create_dir(root.join("e")).unwrap();
    work.take(Order::Check(vec![root.clone()]));
    finish(&mut work);
    let items = work.snapshot(false).items;
    assert_eq!(names(&root, &items), ["", "d", "e"]);
}

#[test]
fn ctrl_r_lists_every_folder_again() {
    let (_temp, root) = temp_tree(&["a/b", "c"]);
    let mut work = Work::new(root.clone(), None);
    assert_eq!(finish(&mut work).len(), 4);
    work.take(Order::Reread);
    assert_eq!(finish(&mut work).len(), 4, "the root, a, a/b, c");
}

#[test]
fn a_stopped_worker_reads_nothing_more_and_dropping_it_stops_it() {
    let (_temp, root) = temp_tree(&["a"]);
    let mut work = Work::new(root.clone(), None);
    let stop = AtomicBool::new(true);
    let (folder, force) = work.next().unwrap();
    work.visit(&folder, force, &stop);
    assert_eq!(work.listed, 0, "a stop leaves the listing");
    let worker = TreeWorker::start(root, None);
    let stop = Arc::clone(&worker.stop);
    drop(worker);
    assert!(
        stop.load(Ordering::SeqCst),
        "the tree closed: no walk left behind"
    );
}

#[test]
#[cfg(windows)]
fn a_link_back_to_a_folder_it_is_in_is_not_walked_into() {
    let (_temp, root) = temp_tree(&["a/b"]);
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(root.join("a").join("b").join("up"))
        .arg(root.join("a"))
        .output()
        .unwrap()
        .status;
    assert!(status.success());
    let mut work = Work::new(root.clone(), None);
    finish(&mut work);
    let items = work.snapshot(false).items;
    assert_eq!(names(&root, &items), ["", "a", "a/b", "a/b/up"]);
}

/// `folders` under `root` an hour old: a folder made in them later changes their time.
#[cfg(windows)]
fn age(root: &Path, folders: &[&str]) {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_FLAG_BACKUP_SEMANTICS: a folder opened as a file.
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let old = SystemTime::now() - Duration::from_secs(3600);
    for folder in folders {
        let folder = fs::OpenOptions::new()
            .write(true)
            .custom_flags(BACKUP_SEMANTICS)
            .open(root.join(folder))
            .unwrap();
        folder.set_modified(old).unwrap();
    }
}

/// The trees' files in a folder beside the test's disk.
#[cfg(windows)]
fn keeping(temp: &tempfile::TempDir, root: &Path) -> Keeping {
    let cache = TreeCache::under(temp.path().join("TreeCache"), root.to_path_buf());
    Keeping { cache, min: 1 }
}

#[test]
#[cfg(windows)]
fn a_folder_read_before_is_not_listed_again_until_its_time_changes() {
    let (temp, root) = temp_tree(&["a", "b"]);
    age(&root, &["", "a"]);
    let keeping = keeping(&temp, &root);
    // The file says the root was read as it is now, with a in it only.
    let mut nodes = Nodes::new(&root);
    nodes.insert(&root.join("a"));
    let time = fs::metadata(&root).unwrap().modified().unwrap();
    nodes.set_read(&root, Some(time));
    keeping.cache.store(&nodes, 1).unwrap();
    let mut work = Work::new(root.clone(), Some(keeping.clone()));
    assert_eq!(
        finish(&mut work),
        ["a"],
        "the root's time is the same: not listed"
    );
    assert_eq!(names(&root, &work.snapshot(false).items), ["", "a"]);
    work.save();
    fs::create_dir(root.join("c")).unwrap();
    let mut work = Work::new(root.clone(), Some(keeping));
    assert_eq!(
        finish(&mut work),
        ["", "b", "c"],
        "the root changed; a was read"
    );
    assert_eq!(
        names(&root, &work.snapshot(false).items),
        ["", "a", "b", "c"]
    );
}

#[test]
#[cfg(windows)]
fn what_was_read_is_saved_and_a_new_worker_starts_from_it() {
    let (temp, root) = temp_tree(&["a/b", "c"]);
    age(&root, &["", "a", "a/b", "c"]);
    let keeping = keeping(&temp, &root);
    let worker = TreeWorker::start(root.clone(), Some(keeping.clone()));
    done(&worker);
    drop(worker);
    let nodes = keeping.cache.load(&root).unwrap();
    assert_eq!(nodes.len(), 4);
    assert!(
        nodes.get(&root.join("a")).unwrap().read.is_some(),
        "with its time"
    );
    let mut work = Work::new(root.clone(), Some(keeping));
    assert_eq!(
        names(&root, &work.snapshot(true).items),
        ["", "a", "a/b", "c"],
        "at once"
    );
    assert!(
        finish(&mut work).is_empty(),
        "nothing changed: nothing listed"
    );
}

#[test]
#[cfg(windows)]
fn a_file_that_cannot_be_saved_says_so() {
    let (temp, root) = temp_tree(&["a"]);
    // The folder of the files is a file.
    fs::write(temp.path().join("TreeCache"), "x").unwrap();
    let worker = TreeWorker::start(root.clone(), Some(keeping(&temp, &root)));
    let unsaved = loop {
        let snapshot = done(&worker);
        if snapshot.unsaved.is_some() {
            break snapshot.unsaved;
        }
    };
    assert!(unsaved.is_some_and(|unsaved| !unsaved.error.is_empty()));
}

#[test]
#[cfg(windows)]
fn a_reading_left_halfway_goes_on_from_there_next_time() {
    let (temp, root) = temp_tree(&["a/a1", "b"]);
    age(&root, &["", "a", "b"]);
    let keeping = keeping(&temp, &root);
    let mut work = Work::new(root.clone(), Some(keeping.clone()));
    let stop = AtomicBool::new(false);
    let (folder, force) = work.next().unwrap();
    work.visit(&folder, force, &stop);
    work.save();
    let mut work = Work::new(root.clone(), Some(keeping));
    assert_eq!(finish(&mut work), ["a", "b", "a/a1"], "not the root again");
}

#[test]
#[cfg(windows)]
fn a_folder_whose_time_is_new_is_listed_again_next_time() {
    let (temp, root) = temp_tree(&["a"]);
    let keeping = keeping(&temp, &root);
    let mut work = Work::new(root.clone(), Some(keeping.clone()));
    finish(&mut work);
    work.save();
    let nodes = keeping.cache.load(&root).unwrap();
    assert_eq!(
        nodes.get(&root).unwrap().read,
        None,
        "made a moment ago: a folder made in the same tick would not show"
    );
}

#[test]
#[cfg(windows)]
fn a_folder_named_with_a_dot_or_a_space_at_its_end_stays_with_its_folders() {
    let (_temp, root) = temp_tree(&["a"]);
    for name in ["d.", "e "] {
        let verbatim = PathBuf::from(format!(r"\\?\{}", root.join(name).display()));
        fs::create_dir_all(verbatim.join("x")).unwrap();
    }
    let mut work = Work::new(root.clone(), None);
    finish(&mut work);
    let items = work.snapshot(false).items;
    assert_eq!(names(&root, &items), ["", "a", "d.", "d./x", "e ", "e /x"]);
}

#[test]
fn a_folder_renamed_in_its_case_only_takes_its_new_name_with_those_in_it() {
    let (_temp, root) = temp_tree(&["work/in", "z"]);
    let mut work = Work::new(root.clone(), None);
    finish(&mut work);
    fs::rename(root.join("work"), root.join("Work")).unwrap();
    work.take(Order::Check(vec![root.clone()]));
    finish(&mut work);
    let items = work.snapshot(false).items;
    assert_eq!(names(&root, &items), ["", "Work", "Work/in", "z"]);
}

#[test]
fn the_other_panels_folder_in_another_case_still_comes_first() {
    let (_temp, root) = temp_tree(&["a/a1", "x/new/deep", "z"]);
    let mut work = Work::new(root.clone(), None);
    work.take(Order::Follow(root.join("X").join("NEW")));
    let order: Vec<String> = (finish(&mut work).iter())
        .map(|name| name.to_lowercase())
        .collect();
    assert_eq!(&order[..4], ["", "x", "x/new", "x/new/deep"], "{order:?}");
    let items = work.snapshot(false).items;
    assert_eq!(names(&root, &items)[4], "x/new", "the disk's spelling");
}

#[test]
fn closing_an_idle_tree_does_not_wait_for_its_worker() {
    let (_temp, root) = temp_tree(&["a"]);
    let worker = TreeWorker::start(root, None);
    done(&worker);
    let started = Instant::now();
    drop(worker);
    // Not held until the last save's limit: the idle worker woke at once.
    assert!(
        started.elapsed() < LAST_SAVES / 6,
        "{:?}",
        started.elapsed()
    );
}

#[test]
#[cfg(windows)]
fn a_tree_closed_saves_on_its_own_and_the_program_s_end_waits_for_it() {
    let (temp, root) = temp_tree(&["a/b", "c"]);
    let keeping = keeping(&temp, &root);
    let worker = TreeWorker::start(root.clone(), Some(keeping.clone()));
    done(&worker);
    // Something new read: saved when the worker idles or stops, whichever comes first.
    fs::create_dir(root.join("new")).unwrap();
    worker.send(Order::Check(vec![root.clone()]));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !worker
        .latest()
        .is_some_and(|snapshot| names(&root, &snapshot.items).contains(&"new".to_string()))
    {
        assert!(Instant::now() < deadline, "new was not read");
        std::thread::sleep(Duration::from_millis(1));
    }
    let started = Instant::now();
    drop(worker);
    assert!(started.elapsed() < LAST_SAVES / 6, "closing holds nothing");
    let waiting = Instant::now();
    wait_for_saves(LAST_SAVES);
    // Other tests' workers may be waited for too, never past the limit.
    assert!(waiting.elapsed() < LAST_SAVES + Duration::from_secs(1));
    let nodes = keeping.cache.load(&root).expect("the file saved");
    assert!(
        nodes.get(&root.join("new")).is_some(),
        "with what was read last"
    );
}

#[test]
fn a_cannot_save_sent_before_the_last_picture_is_not_lost() {
    let picture = |unsaved: Option<&str>| Snapshot {
        items: Vec::new(),
        reading: false,
        count: unsaved.map_or(2, |_| 1),
        unsaved: unsaved.map(|error| crate::jobs::tree_cache::Unsaved {
            file: PathBuf::from("tree.cache"),
            error: error.to_string(),
        }),
    };
    let latest = latest_of([picture(Some("disk full")), picture(None)].into_iter()).unwrap();
    assert_eq!(latest.count, 2, "the last picture");
    assert_eq!(
        latest.unsaved.map(|unsaved| unsaved.error).as_deref(),
        Some("disk full"),
        "with the earlier notice"
    );
    assert!(latest_of(std::iter::empty()).is_none());
}
