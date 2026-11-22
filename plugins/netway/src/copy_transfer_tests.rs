//! Downloads and uploads against the tests' server inside the process, the user's answers scripted.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ask::{Asker, Question, Reply};
use crate::copy_questions::{AppendOrResume, Failed, Overwrite, TransferQuestion, TransferReply};
use crate::copy_work::{CopyParams, Halted, Job, Progress};
use crate::runtime::Runtime;
use crate::ssh::{Answer, Auth, Connection, Login, connect, trust_by};
use crate::test_server::{PASSWORD, TestServer, USER};

/// A connection to a fresh server over `root`.
struct Session {
    sftp: crate::fs::Fs,
    _connection: Option<Connection>,
    runtime: Runtime,
    _server: Box<dyn std::any::Any>,
}

fn session(root: &Path, known: &Path) -> Session {
    let server = TestServer::start(root);
    let runtime = Runtime::new().unwrap();
    let login = Login {
        host: "127.0.0.1".to_string(),
        port: server.port,
        user: USER.to_string(),
        auth: Auth::Password(PASSWORD.to_string()),
    };
    let trust = trust_by(|_| Answer::Keep);
    let connection = (runtime.wait(connect(&login, known, trust), &|| false))
        .unwrap()
        .unwrap();
    let sftp = runtime
        .wait(connection.sftp(""), &|| false)
        .unwrap()
        .unwrap();
    Session {
        sftp: crate::fs::Fs::sftp(sftp, Some(connection.commands()), &Default::default()),
        _connection: Some(connection),
        runtime,
        _server: Box::new(server),
    }
}

/// What a test does.
enum Work {
    Down(&'static str, Vec<String>, PathBuf),
    Up(Vec<PathBuf>, &'static str),
    Delete(&'static str, Vec<String>),
    Folder(&'static str, Option<u16>),
    /// From the folder, the names, into the target with the mask; duplicated or moved.
    Move(
        &'static str,
        Vec<String>,
        (&'static str, &'static str),
        bool,
    ),
    Properties(&'static str, Vec<String>, crate::remote_ops::Properties),
    Link(&'static str, &'static str, bool),
    /// A synchronization's checklist done between the disk's folder and the server's (`timestamps`).
    Sync(Vec<crate::sync_collect::Item>, PathBuf, &'static str, bool),
}

/// How a transfer is asked.
#[derive(Default)]
struct Setup {
    params: CopyParams,
    moving: bool,
    temporary: bool,
    stopped: bool,
}

/// `work` done with `replies` given in turn to the questions (none left: no answer); the questions seen.
fn run(
    session: &Session,
    setup: Setup,
    work: Work,
    replies: Vec<TransferReply>,
) -> (Result<(), Halted>, Vec<TransferQuestion>) {
    let (asker, mut questions) = Asker::new();
    let progress = Progress::default();
    if setup.stopped {
        progress.stop();
    }
    let job = Job {
        fs: &session.sftp,
        asker: &asker,
        progress: &progress,
        params: setup.params,
        moving: setup.moving,
        temporary: setup.temporary,
        overwrite_all: false,
    };
    let mut replies = replies.into_iter();
    let mut seen = Vec::new();
    let mut answer = |question: Question| match question {
        Question::Transfer(question) => {
            seen.push(question);
            replies.next().map_or(Reply::Nothing, Reply::Transfer)
        }
        _ => Reply::Nothing,
    };
    let running = async {
        match &work {
            Work::Down(folder, names, target) => job.download(folder, names, target).await,
            Work::Up(sources, target) => job.upload(sources, target).await,
            Work::Delete(folder, names) => job.delete(folder, names).await,
            Work::Folder(path, rights) => job.make_folder(path, *rights).await,
            Work::Move(folder, names, target, true) => job.duplicate(folder, names, *target).await,
            Work::Move(folder, names, target, false) => {
                job.move_files(folder, names, *target).await
            }
            Work::Properties(folder, names, properties) => {
                job.set_properties(folder, names, *properties).await
            }
            Work::Link(path, to, symbolic) => job.link(path, to, *symbolic).await,
            Work::Sync(items, local, remote, timestamps) => {
                let quiet = Job {
                    params: job.params.clone(),
                    overwrite_all: true,
                    ..job
                };
                quiet.synchronize(items, (local, remote), *timestamps).await
            }
        }
    };
    let done = (session.runtime)
        .wait_answering(running, &|| false, &mut questions, &mut answer)
        .unwrap();
    (done, seen)
}

/// Folders of the test: the server's root with `srv`, a disk's `out`.
struct Folders {
    temp: tempfile::TempDir,
}

impl Folders {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("root/srv/docs/deep")).unwrap();
        fs::create_dir_all(temp.path().join("out")).unwrap();
        Self { temp }
    }
    fn root(&self) -> PathBuf {
        self.temp.path().join("root")
    }
    fn out(&self) -> PathBuf {
        self.temp.path().join("out")
    }
    fn session(&self) -> Session {
        session(&self.root(), &self.temp.path().join("known_hosts"))
    }
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn write(path: &Path, text: &str, seconds: u64) {
    fs::write(path, text).unwrap();
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(time).unwrap();
}

fn modified(path: &Path) -> u64 {
    let time = fs::metadata(path).unwrap().modified().unwrap();
    time.duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A file and a folder with its items come down whole, their names as Windows ones, the times kept.
#[test]
fn files_and_folders_come_down_with_their_times() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    write(&srv.join("docs/deep/b.txt"), "beta", 1_100_000_000);
    let session = folders.session();
    let work = Work::Down("/srv", names(&["a.txt", "docs"]), folders.out());
    let (done, seen) = run(&session, Setup::default(), work, Vec::new());
    assert_eq!((done, seen), (Ok(()), Vec::new()));
    let out = folders.out();
    assert_eq!(fs::read_to_string(out.join("a.txt")).unwrap(), "alpha");
    assert_eq!(
        fs::read_to_string(out.join("docs/deep/b.txt")).unwrap(),
        "beta"
    );
    assert_eq!(modified(&out.join("a.txt")), 1_000_000_000);
    assert!(srv.join("a.txt").exists(), "copied, not moved");
}

/// A disk's folder goes up whole, the times kept; moved, its sources go.
#[test]
fn a_moved_folder_goes_up_and_leaves_the_disk() {
    let folders = Folders::new();
    let up = folders.out().join("up");
    fs::create_dir_all(up.join("inner")).unwrap();
    write(&up.join("inner/c.txt"), "gamma", 1_200_000_000);
    let session = folders.session();
    let setup = Setup {
        moving: true,
        ..Setup::default()
    };
    let (done, _) = run(
        &session,
        setup,
        Work::Up(vec![up.clone()], "/srv"),
        Vec::new(),
    );
    assert_eq!(done, Ok(()));
    let sent = folders.root().join("srv/up/inner/c.txt");
    assert_eq!(fs::read_to_string(&sent).unwrap(), "gamma");
    assert_eq!(modified(&sent), 1_200_000_000);
    assert!(!up.exists(), "moved");
}

/// A file there is asked over: No keeps it, Yes writes it anew; the question is WinSCP's.
#[test]
fn a_file_there_is_asked_over() {
    let folders = Folders::new();
    write(&folders.root().join("srv/a.txt"), "new", 2_000_000_000);
    write(&folders.out().join("a.txt"), "old", 1_000_000_000);
    let session = folders.session();
    let down = || Work::Down("/srv", names(&["a.txt"]), folders.out());
    let no = vec![TransferReply::Overwrite(Overwrite::No)];
    let (done, seen) = run(&session, Setup::default(), down(), no);
    assert_eq!(done, Ok(()));
    assert_eq!(
        fs::read_to_string(folders.out().join("a.txt")).unwrap(),
        "old"
    );
    let TransferQuestion::Overwrite { lines, append } = &seen[0] else {
        panic!("{seen:?}");
    };
    assert!(append);
    assert_eq!(lines[0], "Overwrite local file 'a.txt'?");
    let yes = vec![TransferReply::Overwrite(Overwrite::Yes)];
    let (done, _) = run(&session, Setup::default(), down(), yes);
    assert_eq!(done, Ok(()));
    assert_eq!(
        fs::read_to_string(folders.out().join("a.txt")).unwrap(),
        "new"
    );
}

/// Yes to All asks once for all files; Cancel halts.
#[test]
fn an_answer_for_all_asks_once_and_cancel_halts() {
    let folders = Folders::new();
    for name in ["a.txt", "b.txt"] {
        write(&folders.root().join("srv").join(name), "new", 2_000_000_000);
        write(&folders.out().join(name), "old", 1_000_000_000);
    }
    let session = folders.session();
    let down = || Work::Down("/srv", names(&["a.txt", "b.txt"]), folders.out());
    let cancel = vec![TransferReply::Overwrite(Overwrite::Cancel)];
    let (done, seen) = run(&session, Setup::default(), down(), cancel);
    assert_eq!((done, seen.len()), (Err(Halted), 1));
    let all = vec![TransferReply::Overwrite(Overwrite::YesToAll)];
    let (done, seen) = run(&session, Setup::default(), down(), all);
    assert_eq!((done, seen.len()), (Ok(()), 1));
    assert_eq!(
        fs::read_to_string(folders.out().join("b.txt")).unwrap(),
        "new"
    );
}

/// "New and updated file(s) only": no question, an older source left out.
#[test]
fn newer_only_takes_newer_sources_unasked() {
    let folders = Folders::new();
    write(&folders.root().join("srv/old.txt"), "server", 1_000_000_000);
    write(&folders.root().join("srv/new.txt"), "server", 3_000_000_000);
    write(&folders.out().join("old.txt"), "disk", 2_000_000_000);
    write(&folders.out().join("new.txt"), "disk", 2_000_000_000);
    let session = folders.session();
    let setup = Setup {
        params: CopyParams {
            newer_only: true,
            ..CopyParams::default()
        },
        ..Setup::default()
    };
    let work = Work::Down("/srv", names(&["old.txt", "new.txt"]), folders.out());
    let (done, seen) = run(&session, setup, work, Vec::new());
    assert_eq!((done, seen), (Ok(()), Vec::new()));
    assert_eq!(
        fs::read_to_string(folders.out().join("old.txt")).unwrap(),
        "disk"
    );
    assert_eq!(
        fs::read_to_string(folders.out().join("new.txt")).unwrap(),
        "server"
    );
}

/// Append over a shorter file asks append or resume; resumed, the file is the source.
#[test]
fn a_shorter_target_is_resumed_or_appended_to() {
    let folders = Folders::new();
    write(&folders.out().join("r.txt"), "abc", 1);
    write(&folders.out().join("p.txt"), "123", 1);
    let session = folders.session();
    let resume = vec![
        TransferReply::Overwrite(Overwrite::Append),
        TransferReply::AppendOrResume(AppendOrResume::Resume),
    ];
    let source = folders.out().join("r.txt");
    write(&folders.root().join("srv/r.txt"), "abcdef", 1);
    let (done, seen) = run(
        &session,
        Setup::default(),
        Work::Down("/srv", names(&["r.txt"]), folders.out()),
        resume,
    );
    assert_eq!(done, Ok(()));
    assert!(matches!(seen[1], TransferQuestion::AppendOrResume(_)));
    assert_eq!(fs::read_to_string(&source).unwrap(), "abcdef");
    let append = vec![TransferReply::Overwrite(Overwrite::Append)];
    // A target no shorter than the source: appended to, unasked.
    write(&folders.root().join("srv/p.txt"), "1234", 1);
    let up = Work::Up(vec![folders.out().join("p.txt")], "/srv");
    let (done, seen) = run(&session, Setup::default(), up, append);
    assert_eq!((done, seen.len()), (Ok(()), 1));
    assert_eq!(
        fs::read_to_string(folders.root().join("srv/p.txt")).unwrap(),
        "1234123"
    );
}

/// New name sends the file under the name typed; the mask renames the first level.
#[test]
fn a_new_name_and_the_mask_rename() {
    let folders = Folders::new();
    write(&folders.root().join("srv/a.txt"), "server", 1);
    write(&folders.out().join("a.txt"), "disk", 1);
    let session = folders.session();
    let renamed = vec![
        TransferReply::Overwrite(Overwrite::NewName),
        TransferReply::NewName(Some("other.txt".to_string())),
    ];
    let down = || Work::Down("/srv", names(&["a.txt"]), folders.out());
    let (done, _) = run(&session, Setup::default(), down(), renamed);
    assert_eq!(done, Ok(()));
    assert_eq!(
        fs::read_to_string(folders.out().join("other.txt")).unwrap(),
        "server"
    );
    let setup = Setup {
        params: CopyParams {
            file_mask: "*.bak".to_string(),
            ..CopyParams::default()
        },
        ..Setup::default()
    };
    let (done, _) = run(&session, setup, down(), Vec::new());
    assert_eq!(done, Ok(()));
    assert_eq!(
        fs::read_to_string(folders.out().join("a.bak")).unwrap(),
        "server"
    );
}

/// A source not there is a failure asked about: Skip goes on, Abort halts.
#[test]
fn a_failure_is_skipped_or_aborts() {
    let folders = Folders::new();
    write(&folders.root().join("srv/a.txt"), "alpha", 1);
    let session = folders.session();
    let down = || Work::Down("/srv", names(&["gone.txt", "a.txt"]), folders.out());
    let skip = vec![TransferReply::Failed(Failed::Skip)];
    let (done, seen) = run(&session, Setup::default(), down(), skip);
    assert_eq!(done, Ok(()));
    let TransferQuestion::Failed(lines) = &seen[0] else {
        panic!("{seen:?}");
    };
    assert_eq!(lines[0], "Error transferring file '/srv/gone.txt'.");
    assert!(folders.out().join("a.txt").exists());
    let abort = vec![TransferReply::Failed(Failed::Abort)];
    let (done, _) = run(&session, Setup::default(), down(), abort);
    assert_eq!(done, Err(Halted));
}

/// The editor's and the viewer's copies: no question, a file there overwritten; a stop halts.
#[test]
fn temporary_copies_ask_nothing_and_a_stop_halts() {
    let folders = Folders::new();
    write(&folders.root().join("srv/a.txt"), "server", 1);
    write(&folders.out().join("a.txt"), "disk", 1);
    let session = folders.session();
    let setup = Setup {
        temporary: true,
        ..Setup::default()
    };
    let down = || Work::Down("/srv", names(&["a.txt"]), folders.out());
    let (done, seen) = run(&session, setup, down(), Vec::new());
    assert_eq!((done, seen), (Ok(()), Vec::new()));
    assert_eq!(
        fs::read_to_string(folders.out().join("a.txt")).unwrap(),
        "server"
    );
    let stopped = Setup {
        stopped: true,
        ..Setup::default()
    };
    let (done, _) = run(&session, stopped, down(), Vec::new());
    assert_eq!(done, Err(Halted));
}

/// A moved file leaves the server; a moved folder too, after its items.
#[test]
fn moved_files_leave_the_server() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1);
    write(&srv.join("docs/deep/b.txt"), "beta", 1);
    let session = folders.session();
    let setup = Setup {
        moving: true,
        ..Setup::default()
    };
    let work = Work::Down("/srv", names(&["a.txt", "docs"]), folders.out());
    let (done, _) = run(&session, setup, work, Vec::new());
    assert_eq!(done, Ok(()));
    assert!(!srv.join("a.txt").exists());
    assert!(!srv.join("docs").exists());
    assert!(folders.out().join("docs/deep/b.txt").exists());
}

/// A junction at `link` to the folder `target` (a link no right is needed for).
#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
}

/// R7 review C2: a move copies what a link to a folder leads to and never removes it - up from the disk
/// (a junction), down from the server (one the server lists as a link).
#[cfg(windows)]
#[test]
fn a_move_never_removes_what_a_link_leads_to() {
    let folders = Folders::new();
    let shared = folders.temp.path().join("shared");
    fs::create_dir_all(&shared).unwrap();
    write(&shared.join("kept.txt"), "kept", 1);
    let up = folders.out().join("up");
    fs::create_dir_all(&up).unwrap();
    write(&up.join("own.txt"), "own", 1);
    junction(&up.join("libs"), &shared);
    let session = folders.session();
    let moving = || Setup {
        moving: true,
        ..Setup::default()
    };
    let work = Work::Up(vec![up.clone()], "/srv");
    assert_eq!(run(&session, moving(), work, Vec::new()).0, Ok(()));
    let sent = folders.root().join("srv/up");
    assert_eq!(
        fs::read_to_string(sent.join("libs/kept.txt")).unwrap(),
        "kept"
    );
    assert!(
        shared.join("kept.txt").exists(),
        "behind the junction: kept"
    );
    assert!(!up.join("own.txt").exists(), "the folder's own file moved");

    // Backslashes only: `mklink` takes no `/`.
    let served = folders.root().join("srv").join("docs");
    junction(&served.join("link"), &shared);
    let work = Work::Down("/srv", names(&["docs"]), folders.out());
    let (done, seen) = run(&session, moving(), work, Vec::new());
    assert_eq!(done, Ok(()), "{seen:?}");
    assert!(folders.out().join("docs/link/kept.txt").exists(), "copied");
    assert!(
        !served.join("link").exists(),
        "the server's link itself went"
    );
    assert!(
        shared.join("kept.txt").exists(),
        "behind the server's link: kept"
    );
}

#[path = "copy_settings_tests.rs"]
mod settings;

#[path = "remote_ops_tests.rs"]
mod ops;

#[path = "resume_tests.rs"]
mod resume;

#[path = "scp_transfer_tests.rs"]
mod scp;

#[path = "ftp_transfer_tests.rs"]
mod ftp;

#[path = "webdav_transfer_tests.rs"]
mod webdav;

#[path = "s3_transfer_tests.rs"]
mod s3;

#[path = "sync_transfer_tests.rs"]
mod sync;

#[path = "background_files_tests.rs"]
mod background_files;
