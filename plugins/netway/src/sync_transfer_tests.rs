//! Synchronizing against the tests' SFTP server: the trees compared, the checklist done.

use super::*;
use crate::sync_collect::{Action, Mode, Settings, collect};
use crate::sync_work::{local_tree, remote_tree};

fn checklist(
    session: &Session,
    local: &Path,
    settings: &Settings,
) -> Vec<crate::sync_collect::Item> {
    let disk = local_tree(local).unwrap();
    let server = (session.runtime)
        .wait(remote_tree(&session.sftp, "/srv", &|| false), &|| false)
        .unwrap()
        .unwrap();
    collect(&disk, &server, settings, "")
}

/// To the server: a newer local file and a new folder up, a remote-only file deleted ("Delete files").
#[test]
fn files_go_up_and_the_extra_one_goes() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let out = folders.out();
    write(&srv.join("old.txt"), "old", 1_000_000_000);
    write(&out.join("old.txt"), "new!", 1_100_000_000);
    fs::create_dir_all(out.join("dir")).unwrap();
    write(&out.join("dir/inner.txt"), "in", 1_100_000_000);
    write(&srv.join("extra.txt"), "x", 1_000_000_000);
    let session = folders.session();
    let settings = Settings {
        mode: Mode::Remote,
        delete: true,
        ..Settings::default()
    };
    let items = checklist(&session, &out, &settings);
    let actions: Vec<(&str, Action)> = (items.iter())
        .map(|item| (item.path.as_str(), item.action))
        .collect();
    assert_eq!(
        actions,
        [
            ("dir", Action::UploadNew),
            ("docs", Action::DeleteRemote),
            ("extra.txt", Action::DeleteRemote),
            ("old.txt", Action::UploadUpdate),
        ]
    );
    let work = Work::Sync(items, out.clone(), "/srv", false);
    let (done, seen) = run(&session, Setup::default(), work, Vec::new());
    assert_eq!(done, Ok(()), "{seen:?}");
    assert!(seen.is_empty(), "no questions: {seen:?}");
    assert_eq!(fs::read_to_string(srv.join("old.txt")).unwrap(), "new!");
    assert_eq!(fs::read_to_string(srv.join("dir/inner.txt")).unwrap(), "in");
    assert!(!srv.join("extra.txt").exists());
    assert!(!srv.join("docs").exists(), "a remote-only folder deleted");
}

/// "Keep Remote Directory Up To Date": a file made on the disk while watching goes up; Esc (a stop) ends it.
#[test]
fn a_local_change_is_reflected_until_stopped() {
    use crate::keep_up_to_date::Watch;
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let out = folders.out();
    let session = folders.session();
    let (asker, mut questions) = Asker::new();
    let progress = Progress::default();
    let job = Job {
        fs: &session.sftp,
        asker: &asker,
        progress: &progress,
        params: Setup::default().params,
        moving: false,
        temporary: false,
        overwrite_all: true,
    };
    let watch = Watch {
        local: out.display().to_string(),
        remote: "/srv".to_string(),
        delete: false,
        existing_only: false,
        recursive: true,
        selected_only: false,
        on_start: false,
        save: false,
    };
    let fresh = srv.join("fresh.txt");
    let work = async {
        let changing = async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            fs::write(out.join("fresh.txt"), "fresh").unwrap();
            for _ in 0..100 {
                if fs::read_to_string(&fresh).is_ok_and(|text| text == "fresh") {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            progress.stop();
        };
        let (done, ()) = tokio::join!(job.keep_up_to_date(&watch, &[]), changing);
        done
    };
    let mut answer = |_question: Question| Reply::Nothing;
    let done = (session.runtime)
        .wait_answering(work, &|| false, &mut questions, &mut answer)
        .unwrap();
    assert_eq!(done, Ok(()));
    assert_eq!(fs::read_to_string(&fresh).unwrap(), "fresh");
}

/// A disk's folder that cannot be read is never compared: with "Delete files" and "Synchronize on start"
/// the server's files stay (an empty tree would have deleted them all).
#[test]
fn an_unreadable_local_folder_deletes_nothing() {
    use crate::keep_up_to_date::Watch;
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("kept.txt"), "kept", 1_000_000_000);
    let session = folders.session();
    let (asker, mut questions) = Asker::new();
    let progress = Progress::default();
    let job = Job {
        fs: &session.sftp,
        asker: &asker,
        progress: &progress,
        params: Setup::default().params,
        moving: false,
        temporary: false,
        overwrite_all: true,
    };
    let watch = Watch {
        local: folders.out().join("gone").display().to_string(),
        remote: "/srv".to_string(),
        delete: true,
        existing_only: false,
        recursive: true,
        selected_only: false,
        on_start: true,
        save: false,
    };
    let work = async {
        let stopping = async {
            tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            progress.stop();
        };
        let (done, ()) = tokio::join!(job.keep_up_to_date(&watch, &[]), stopping);
        done
    };
    let mut seen = Vec::new();
    let mut answer = |question: Question| {
        seen.push(format!("{question:?}"));
        Reply::Nothing
    };
    let done = (session.runtime)
        .wait_answering(work, &|| false, &mut questions, &mut answer)
        .unwrap();
    assert_eq!(done, Ok(()));
    assert!(seen.is_empty(), "a few failed rounds ask nothing: {seen:?}");
    assert_eq!(fs::read_to_string(srv.join("kept.txt")).unwrap(), "kept");
    assert!(srv.join("docs").exists(), "the server's folder kept");
}

/// To the disk with timestamps: only the times change.
#[test]
fn timestamps_set_the_times_only() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let out = folders.out();
    write(&srv.join("a.txt"), "server", 1_200_000_000);
    write(&out.join("a.txt"), "local!", 1_000_000_000);
    let session = folders.session();
    let settings = Settings {
        mode: Mode::Local,
        timestamps: true,
        existing_only: true,
        ..Settings::default()
    };
    let items = checklist(&session, &out, &settings);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].action, Action::DownloadUpdate);
    let (done, _) = run(
        &session,
        Setup::default(),
        Work::Sync(items, out.clone(), "/srv", true),
        Vec::new(),
    );
    assert_eq!(done, Ok(()));
    assert_eq!(
        fs::read_to_string(out.join("a.txt")).unwrap(),
        "local!",
        "content kept"
    );
    let time = fs::metadata(out.join("a.txt")).unwrap().modified().unwrap();
    assert_eq!(crate::copy_work::unix_seconds(time), Some(1_200_000_000));
}

/// Watching the disk's `local` folder onto "/srv", subdirectories updated.
fn watching(local: &Path, delete: bool, on_start: bool) -> crate::keep_up_to_date::Watch {
    crate::keep_up_to_date::Watch {
        local: local.display().to_string(),
        remote: "/srv".to_string(),
        delete,
        existing_only: false,
        recursive: true,
        selected_only: false,
        on_start,
        save: false,
    }
}

/// `watch` kept up to date until it ends or `during` does (then stopped), `replies` given in turn; the
/// questions asked with the time each came after the start.
fn watched(
    session: &Session,
    watch: &crate::keep_up_to_date::Watch,
    during: impl std::future::Future<Output = ()>,
    replies: Vec<TransferReply>,
) -> (Result<(), Halted>, Vec<(Question, std::time::Duration)>) {
    let (asker, mut questions) = Asker::new();
    let progress = Progress::default();
    let job = Job {
        fs: &session.sftp,
        asker: &asker,
        progress: &progress,
        params: Setup::default().params,
        moving: false,
        temporary: false,
        overwrite_all: true,
    };
    let work = async {
        let keeping = job.keep_up_to_date(watch, &[]);
        tokio::pin!(keeping);
        tokio::select! {
            done = &mut keeping => done,
            () = during => {
                progress.stop();
                keeping.await
            }
        }
    };
    let started = std::time::Instant::now();
    let mut replies = replies.into_iter();
    let mut seen = Vec::new();
    let mut answer = |question: Question| {
        seen.push((question, started.elapsed()));
        replies.next().map_or(Reply::Nothing, Reply::Transfer)
    };
    let done = (session.runtime)
        .wait_answering(work, &|| false, &mut questions, &mut answer)
        .unwrap();
    (done, seen)
}

async fn pause(millis: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
}

/// A disk's folder unread five rounds in a row (a round a second) is asked about with WinSCP's
/// `LIST_DIR_ERROR`, not before; Abort stops watching, the server's files kept.
#[test]
fn an_unreadable_local_folder_is_asked_about_after_five_rounds() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("kept.txt"), "kept", 1_000_000_000);
    let session = folders.session();
    let watch = watching(&folders.out().join("gone"), true, true);
    let abort = vec![TransferReply::Failed(Failed::Abort)];
    let (done, seen) = watched(&session, &watch, pause(15_000), abort);
    assert_eq!(done, Err(Halted));
    assert_eq!(seen.len(), 1, "{seen:?}");
    let (Question::Transfer(TransferQuestion::Failed(lines)), after) = &seen[0] else {
        panic!("{seen:?}");
    };
    // The lines wrapped: compared without their spaces.
    let headline = format!("Error listing directory '{}'.", watch.local).replace(' ', "");
    assert!(
        lines.concat().replace(' ', "").starts_with(&headline),
        "{lines:?}"
    );
    // Five reads, four periods of a second waited between them.
    assert!(
        *after >= std::time::Duration::from_millis(3_900),
        "{after:?}"
    );
    assert_eq!(fs::read_to_string(srv.join("kept.txt")).unwrap(), "kept");
}

/// A folder that does not change is not compared again: without "Synchronize on start" the server's
/// extra file stays; with it, the first round deletes it and later rounds leave a new one alone.
#[test]
fn an_unchanged_folder_is_reflected_on_start_only() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let extra = srv.join("extra.txt");
    write(&extra, "x", 1_000_000_000);
    let session = folders.session();
    let quiet = watching(&folders.out(), true, false);
    let (done, seen) = watched(&session, &quiet, pause(2_300), Vec::new());
    assert_eq!((done, seen.len()), (Ok(()), 0));
    assert!(extra.exists(), "nothing changed, nothing compared");
    let later = srv.join("later.txt");
    let during = async {
        for _ in 0..100 {
            if !extra.exists() {
                break;
            }
            pause(100).await;
        }
        fs::write(&later, "later").unwrap();
        pause(2_300).await;
    };
    let on_start = watching(&folders.out(), true, true);
    let (done, seen) = watched(&session, &on_start, during, Vec::new());
    assert_eq!((done, seen.len()), (Ok(()), 0));
    assert!(!extra.exists() && !srv.join("docs").exists(), "on start");
    assert_eq!(fs::read_to_string(&later).unwrap(), "later", "once only");
}
