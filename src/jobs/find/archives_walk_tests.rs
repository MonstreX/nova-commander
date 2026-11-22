//! "Search in archives" past a disk's own files: the status naming the place inside an archive and the
//! archive while it is opened, a plugin's file with a real name searched in, two files naming each other
//! each opened inside the other once, a stop during a file's read opening no archive after it.

use super::{Disk, Opener, disk};
use crate::jobs::find::tests::request;
use crate::jobs::find::{Archives, Found, Request, Search, text};
use crate::vfs::test_tree as tree;
use crate::vfs::{Entry, FileOpener, OpenedFile, VfsPath};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A wait held until let go: whether it was entered.
#[derive(Default)]
struct Held {
    entered: AtomicBool,
    released: AtomicBool,
}

impl Held {
    /// Entered, then waiting until let go (ten seconds at most).
    fn hold(&self) {
        self.entered.store(true, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.released.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Waits until it is entered.
    fn wait_entered(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.entered.load(Ordering::Relaxed) {
            assert!(Instant::now() < deadline, "never entered");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn release(&self) {
        self.released.store(true, Ordering::Relaxed);
    }
}

/// A session of `reals://`: the disk's files it lists, and the hold of its reads.
type Session = (String, Vec<PathBuf>, Option<Arc<Held>>);

static SESSIONS: Mutex<Vec<Session>> = Mutex::new(Vec::new());

/// `reals://<session>/`: a plugin's folder whose files have real names (a list's, FAR's
/// `OPIF_REALNAMES`), listing the disk's files it was given; a read waits while the session is held.
struct Reals;

fn session_of(path: &VfsPath) -> io::Result<(Vec<PathBuf>, Option<Arc<Held>>)> {
    let (session, _) = path.session_path().ok_or(io::ErrorKind::NotFound)?;
    let sessions = SESSIONS.lock().unwrap();
    (sessions.iter())
        .find(|(name, ..)| name == session)
        .map(|(_, files, held)| (files.clone(), held.clone()))
        .ok_or_else(|| io::ErrorKind::NotFound.into())
}

fn name_of(file: &Path) -> String {
    file.file_name().unwrap().to_string_lossy().into_owned()
}

impl crate::vfs::Provider for Reals {
    fn id(&self) -> &str {
        "reals"
    }

    fn list(
        &self,
        dir: &VfsPath,
        _cancel: &AtomicBool,
        found: &mut dyn FnMut(Entry),
    ) -> io::Result<()> {
        for file in session_of(dir)?.0 {
            found(Entry {
                name: name_of(&file),
                size: std::fs::metadata(&file)?.len(),
                real_path: Some(file),
                ..Entry::default()
            });
        }
        Ok(())
    }

    fn open_read(&self, path: &VfsPath) -> io::Result<Box<dyn io::Read + Send>> {
        let (files, held) = session_of(path)?;
        if let Some(held) = held {
            held.hold();
        }
        let name = path.file_name().unwrap_or_default();
        let file = (files.into_iter())
            .find(|file| name_of(file) == name)
            .ok_or(io::ErrorKind::NotFound)?;
        Ok(Box::new(std::fs::File::open(file)?))
    }
}

/// The session `name` of `reals://` listing `files`, its reads held by `held`: its root.
fn reals(name: &str, files: Vec<PathBuf>, held: Option<Arc<Held>>) -> VfsPath {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| crate::vfs::register(Arc::new(Reals)));
    SESSIONS
        .lock()
        .unwrap()
        .push((name.to_string(), files, held));
    VfsPath::in_session("reals", name, "/")
}

/// The search of `request` to its end: what it found, and its status then.
fn run_to_end(request: Request) -> (Vec<Found>, String) {
    let search = Search::start(request);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut found = Vec::new();
    loop {
        let over = search.over();
        found.extend(search.take_found());
        if over {
            return (found, search.status());
        }
        assert!(Instant::now() < deadline, "the search did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn opener_of(archives: HashMap<PathBuf, VfsPath>) -> Arc<Opener> {
    Arc::new(Opener {
        archives,
        ..Opener::default()
    })
}

fn archives_of(opener: &Arc<Opener>) -> Option<Archives> {
    Some(Archives(Arc::clone(opener) as Arc<dyn FileOpener>))
}

#[test]
fn the_status_names_the_place_inside_an_archive() {
    let disk = disk("arc-status");
    let temp = disk.temp.path().display().to_string();
    let (_, status) = run_to_end(disk.request("*"));
    assert_eq!(
        status.replace(&temp, "T"),
        r"T\sub\b.zip:\",
        "the last place walked: the last archive's root"
    );
    let mut request = disk.request("*");
    request.pattern = Some(text::Pattern::Bytes(b"hello".to_vec()));
    let (_, status) = run_to_end(request);
    assert_eq!(
        status.replace(&temp, "T"),
        r"T\sub\b.zip:\z.txt",
        "the last file read"
    );
}

#[test]
fn the_status_names_a_folder_inside_an_archive_as_it_is_walked_into() {
    let temp = tempfile::tempdir().unwrap();
    let zip = temp.path().join("a.zip");
    std::fs::write(&zip, "zip").unwrap();
    let inside = tree::root("arc-status-in");
    tree::write(&inside.join("x.txt"), b"x");
    tree::dir(&inside.join("in"));
    tree::write(&inside.join("in").join("y.txt"), b"y");
    let opener = opener_of(HashMap::from([(zip.clone(), inside)]));
    let mut request = request(&VfsPath::local(temp.path()), "*");
    request.archives = archives_of(&opener);
    let (found, status) = run_to_end(request);
    assert_eq!(found.len(), 4, "{found:?}");
    assert_eq!(
        status,
        format!(r"{}:\in", zip.display()),
        "the last folder walked into"
    );
}

/// Opens nothing; holds the search in `open` until let go.
#[derive(Default)]
struct Holding(Held);

impl FileOpener for Holding {
    fn open(&self, _file: &Path, _stop: &AtomicBool) -> Option<OpenedFile> {
        self.0.hold();
        None
    }
}

#[test]
fn the_status_names_the_archive_while_it_is_opened() {
    let temp = tempfile::tempdir().unwrap();
    let zip = temp.path().join("a.zip");
    std::fs::write(&zip, "zip").unwrap();
    let holding = Arc::new(Holding::default());
    let mut request = request(&VfsPath::local(temp.path()), "*");
    request.archives = Some(Archives(Arc::clone(&holding) as Arc<dyn FileOpener>));
    let search = Search::start(request);
    holding.0.wait_entered();
    let status = search.status();
    holding.0.release();
    assert_eq!(status, zip.display().to_string());
    wait_over(&search);
}

#[test]
fn a_plugins_file_with_a_real_name_is_searched_in() {
    let disk = disk("arc-reals");
    let list = reals("arc-reals-list", vec![disk.temp.path().join("a.zip")], None);
    let mut request = disk.request("*.txt");
    request.roots = vec![list];
    let found = disk.run(request);
    assert_eq!(found, [r"T\a.zip:\|x.txt", r"T\a.zip:\in|y.txt"]);
    assert_eq!(disk.offered(), ["a.zip"]);
}

#[test]
fn two_files_naming_each_other_are_each_opened_inside_the_other_once() {
    let temp = tempfile::tempdir().unwrap();
    let (a, b) = (temp.path().join("a.lst"), temp.path().join("b.lst"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    let opener = opener_of(HashMap::from([
        (a.clone(), reals("arc-each-a", vec![b.clone()], None)),
        (b.clone(), reals("arc-each-b", vec![a.clone()], None)),
    ]));
    let mut request = request(&VfsPath::local(temp.path()), "*");
    request.archives = archives_of(&opener);
    let disk = Disk { temp, opener };
    let found = disk.run(request);
    assert_eq!(
        disk.offered(),
        ["a.lst", "b.lst", "b.lst", "a.lst"],
        "never inside itself, however deep: {found:?}"
    );
    assert_eq!(found.len(), 6, "{found:?}");
    assert_eq!(disk.closed(), 4);
}

/// Waits until the search is over.
fn wait_over(search: &Search) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !search.over() {
        assert!(Instant::now() < deadline, "the search did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_stop_during_a_files_read_takes_it_not_and_opens_no_archive_after_it() {
    let temp = tempfile::tempdir().unwrap();
    let zip = temp.path().join("a.zip");
    std::fs::write(&zip, "zip").unwrap();
    let held = Arc::new(Held::default());
    let list = reals("arc-held", vec![zip.clone()], Some(Arc::clone(&held)));
    let opener = opener_of(HashMap::from([(zip, tree::root("arc-held-a"))]));
    let mut request = request(&list, "*");
    // "Not containing" would take a file whose read the stop broke.
    request.pattern = Some(text::Pattern::Bytes(b"absent".to_vec()));
    request.not_containing = true;
    request.archives = archives_of(&opener);
    let search = Search::start(request);
    held.wait_entered();
    search.stop();
    held.release();
    wait_over(&search);
    assert!(search.take_found().is_empty(), "a file read when stopped");
    assert!(
        opener.offered.lock().unwrap().is_empty(),
        "stopped: not opened"
    );
}

#[test]
fn a_search_stopped_in_an_archive_walks_no_root_after_it() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.zip"), "zip").unwrap();
    let holding = Arc::new(super::Holding::default());
    let mut request = request(&VfsPath::local(temp.path()), "*");
    request.roots.push(tree::root("arc-stop-second"));
    request.archives = Some(Archives(Arc::clone(&holding) as Arc<dyn FileOpener>));
    let search = Search::start(request);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !holding.entered.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline, "the file was not offered");
        std::thread::sleep(Duration::from_millis(2));
    }
    search.stop();
    wait_over(&search);
    assert!(
        tree::log("arc-stop-second").is_empty(),
        "the next root not walked"
    );
}
