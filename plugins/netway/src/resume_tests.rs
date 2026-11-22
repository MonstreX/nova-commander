//! Resume through a temporary name against the tests' server (S6.5e, WinSCP's `.filepart`).

use super::*;
use crate::copy_params::Resume as Support;
use crate::copy_questions::Resume;

fn resuming() -> Setup {
    Setup {
        params: CopyParams {
            resume: Support::On,
            ..CopyParams::default()
        },
        ..Setup::default()
    }
}

/// A file there replaced through its temporary name: none of it left after.
#[test]
fn an_upload_goes_through_its_temporary_name() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    fs::write(srv.join("a.txt"), "old").unwrap();
    let source = folders.out().join("a.txt");
    fs::write(&source, "new text").unwrap();
    let session = folders.session();
    let yes = vec![TransferReply::Overwrite(Overwrite::Yes)];
    let work = Work::Up(vec![source], "/srv");
    let (done, _) = run(&session, resuming(), work, yes);
    assert_eq!(done, Ok(()));
    assert_eq!(fs::read_to_string(srv.join("a.txt")).unwrap(), "new text");
    assert!(!srv.join("a.txt.filepart").exists());
}

/// A partial file from before asked about: Yes resumes it, No sends the file anew; one bigger than the
/// source is said and deleted.
#[test]
fn a_partial_upload_is_resumed_or_restarted() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let source = folders.out().join("b.txt");
    fs::write(&source, "0123456789").unwrap();
    let session = folders.session();
    fs::write(srv.join("b.txt.filepart"), "01234").unwrap();
    let yes = vec![TransferReply::Resume(Resume::Yes)];
    let (done, seen) = run(
        &session,
        resuming(),
        Work::Up(vec![source.clone()], "/srv"),
        yes,
    );
    assert_eq!(done, Ok(()));
    assert_eq!(
        seen,
        [TransferQuestion::Resume {
            name: "b.txt".to_string(),
            bigger: false
        }]
    );
    assert_eq!(fs::read_to_string(srv.join("b.txt")).unwrap(), "0123456789");
    assert!(!srv.join("b.txt.filepart").exists());
    fs::remove_file(srv.join("b.txt")).unwrap();
    fs::write(srv.join("b.txt.filepart"), "XXXXX").unwrap();
    let no = vec![TransferReply::Resume(Resume::No)];
    let (done, _) = run(
        &session,
        resuming(),
        Work::Up(vec![source.clone()], "/srv"),
        no,
    );
    assert_eq!(done, Ok(()));
    assert_eq!(fs::read_to_string(srv.join("b.txt")).unwrap(), "0123456789");
    fs::remove_file(srv.join("b.txt")).unwrap();
    fs::write(srv.join("b.txt.filepart"), "much longer than the source").unwrap();
    let ok = vec![TransferReply::Resume(Resume::No)];
    let (done, seen) = run(&session, resuming(), Work::Up(vec![source], "/srv"), ok);
    assert_eq!(done, Ok(()));
    assert!(matches!(
        &seen[..],
        [TransferQuestion::Resume { bigger: true, .. }]
    ));
    assert_eq!(fs::read_to_string(srv.join("b.txt")).unwrap(), "0123456789");
}

/// A download resumed from its partial file on the disk; Cancel halts and keeps it.
#[test]
fn a_partial_download_is_resumed() {
    let folders = Folders::new();
    fs::write(folders.root().join("srv/c.txt"), "abcdefghij").unwrap();
    let partial = folders.out().join("c.txt.filepart");
    fs::write(&partial, "abc").unwrap();
    let session = folders.session();
    let down = || Work::Down("/srv", names(&["c.txt"]), folders.out());
    let cancel = vec![TransferReply::Resume(Resume::Cancel)];
    let (done, _) = run(&session, resuming(), down(), cancel);
    assert_eq!(done, Err(Halted));
    assert!(partial.exists(), "kept");
    let yes = vec![TransferReply::Resume(Resume::Yes)];
    let (done, _) = run(&session, resuming(), down(), yes);
    assert_eq!(done, Ok(()));
    let got = fs::read_to_string(folders.out().join("c.txt")).unwrap();
    assert_eq!(got, "abcdefghij");
    assert!(!partial.exists());
}

/// Small files (below the threshold), the text mode and the viewer's copies are never resumable.
#[test]
fn some_transfers_never_go_through_a_temporary_name() {
    let params = CopyParams::default();
    assert!(!params.allow_resume(100 * 1024 - 1, "a"));
    assert!(params.allow_resume(100 * 1024, "a"));
    assert!(
        !params.allow_resume(1 << 30, &"n".repeat(250)),
        "a name too long"
    );
    let folders = Folders::new();
    fs::write(folders.root().join("srv/d.txt"), "d\n").unwrap();
    let partial = folders.out().join("d.txt.filepart");
    fs::write(&partial, "x").unwrap();
    let session = folders.session();
    let text = Setup {
        params: CopyParams {
            resume: Support::On,
            transfer_mode: crate::copy_params::TransferMode::Text,
            ..CopyParams::default()
        },
        ..Setup::default()
    };
    let down = Work::Down("/srv", names(&["d.txt"]), folders.out());
    let (done, seen) = run(&session, text, down, Vec::new());
    assert_eq!((done, seen), (Ok(()), Vec::new()));
    assert!(partial.exists(), "the text mode leaves it alone");
}

/// A resumed download tried again (Retry after a failure part-way) writes from where it was resumed:
/// what the failed attempt wrote past the offset is cut first, not kept before the rest.
#[test]
fn a_retried_resume_writes_its_part_once() {
    let folders = Folders::new();
    fs::write(folders.root().join("srv/c.txt"), "0123456789").unwrap();
    let target = folders.out().join("c.txt");
    let partial = folders.out().join("c.txt.filepart");
    // The first attempt resumed at 5 and wrote "567" plus some before it failed.
    fs::write(&partial, "01234567X").unwrap();
    let session = folders.session();
    let (asker, _questions) = Asker::new();
    let progress = Progress::default();
    let job = Job {
        fs: &session.sftp,
        asker: &asker,
        progress: &progress,
        params: CopyParams::default(),
        moving: false,
        temporary: false,
        overwrite_all: false,
    };
    let write = crate::copy_work::Write::Resume(5);
    let receiving = crate::download::Receiving {
        source: "/srv/c.txt",
        target: &target,
        partial: Some(&partial),
        write: &write,
        modified: None,
        text: false,
        read_only: false,
    };
    let done = (session.runtime)
        .wait(job.receive(&receiving), &|| false)
        .unwrap();
    assert_eq!(done, Ok(()));
    assert_eq!(fs::read_to_string(&target).unwrap(), "0123456789");
    assert!(!partial.exists());
}

/// An appended download tried again writes the whole source after the file's first part only (what the
/// failed attempt appended cut); its time and read-only mark are set after, on the file in place.
#[test]
fn a_retried_append_writes_the_source_once() {
    let folders = Folders::new();
    fs::write(folders.root().join("srv/c.txt"), "0123").unwrap();
    let target = folders.out().join("c.txt");
    // The file was "abc"; the failed attempt appended "01" to it.
    fs::write(&target, "abc01").unwrap();
    let session = folders.session();
    let (asker, _questions) = Asker::new();
    let progress = Progress::default();
    let job = Job {
        fs: &session.sftp,
        asker: &asker,
        progress: &progress,
        params: CopyParams::default(),
        moving: false,
        temporary: false,
        overwrite_all: false,
    };
    let write = crate::copy_work::Write::Append(3);
    let receiving = crate::download::Receiving {
        source: "/srv/c.txt",
        target: &target,
        partial: None,
        write: &write,
        modified: Some(1_000_000_000),
        text: false,
        read_only: true,
    };
    let work = async {
        job.receive(&receiving).await?;
        let received = fs::read_to_string(&target).map_err(crate::copy_work::fault)?;
        job.receive(&receiving).await?;
        job.finish(&receiving)?;
        Ok::<_, crate::copy_work::Fault>(received)
    };
    let done = (session.runtime).wait(work, &|| false).unwrap();
    assert_eq!(done.as_deref(), Ok("abc0123"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "abc0123", "again");
    assert_eq!(modified(&target), 1_000_000_000);
    assert!(fs::metadata(&target).unwrap().permissions().readonly());
    // The temporary folder must go: the read-only bit off again.
    nc_plugin_sdk::system::set_attributes(&target, 0).unwrap();
}
