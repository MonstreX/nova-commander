//! The program's file calls on a session (S6.8b) against the tests' servers: reads by offset going on or
//! opened again, a file written anew streamed (SFTP) or kept and sent whole at its end (S3), a copy
//! abandoned (API 9's `WRITE_ABORT`) never sent.

use super::*;
use crate::background_files::{Files, stat};
use nc_plugin_sdk::{ErrorKind, WriteMode};

/// `path` read through `files` in pieces of `piece` bytes from `offset` to its end.
fn read_all(session: &Session, files: &mut Files, path: &str, piece: usize) -> Vec<u8> {
    let mut whole = Vec::new();
    loop {
        let mut buffer = vec![0; piece];
        let offset = whole.len() as u64;
        let work = files.read(&session.sftp, path, offset, &mut buffer);
        let count = session.runtime.wait(work, &|| false).unwrap().unwrap();
        whole.extend_from_slice(&buffer[..count]);
        if count < piece {
            return whole;
        }
    }
}

/// `work` done on the session's runtime, its success.
fn done<T>(
    session: &Session,
    work: impl std::future::Future<Output = Result<T, nc_plugin_sdk::Error>>,
) -> T {
    session.runtime.wait(work, &|| false).unwrap().unwrap()
}

/// `pieces` written as the program's writer does: one `New` with no bytes, `Append`s, the end.
fn write_all(session: &Session, files: &mut Files, path: &str, pieces: &[&[u8]]) {
    let fs = &session.sftp;
    done(session, files.write(fs, path, WriteMode::New, b""));
    for piece in pieces {
        done(session, files.write(fs, path, WriteMode::Append, piece));
    }
    done(session, files.end(fs, path));
}

/// SFTP: read in pieces (each going on), from a place again, written anew and ended; a file there is
/// never written over; its entry stat'ed.
#[test]
fn a_session_is_read_by_offset_and_written_as_it_comes() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let content: Vec<u8> = (0..70_000u32).map(|at| (at % 251) as u8).collect();
    fs::write(srv.join("big.bin"), &content).unwrap();
    let session = folders.session();
    let mut files = Files::default();
    assert_eq!(
        read_all(&session, &mut files, "/srv/big.bin", 4096),
        content
    );
    let mut buffer = vec![0; 10];
    let again = files.read(&session.sftp, "/srv/big.bin", 1000, &mut buffer);
    assert_eq!(session.runtime.wait(again, &|| false).unwrap().unwrap(), 10);
    assert_eq!(buffer, content[1000..1010]);
    // Gone from its folder now: the read going on reads on in the file kept open, one from another
    // place opens it again and fails.
    fs::remove_file(srv.join("big.bin")).unwrap();
    let on = files.read(&session.sftp, "/srv/big.bin", 1010, &mut buffer);
    assert_eq!(session.runtime.wait(on, &|| false).unwrap().unwrap(), 10);
    assert_eq!(buffer, content[1010..1020], "read on, never opened again");
    let elsewhere = files.read(&session.sftp, "/srv/big.bin", 5, &mut buffer);
    assert!(session.runtime.wait(elsewhere, &|| false).unwrap().is_err());
    write_all(&session, &mut files, "/srv/up.bin", &[b"one ", b"two"]);
    assert_eq!(fs::read(srv.join("up.bin")).unwrap(), b"one two");
    let over = files.write(&session.sftp, "/srv/up.bin", WriteMode::New, b"");
    let refused = session.runtime.wait(over, &|| false).unwrap().unwrap_err();
    assert_eq!(refused.kind, ErrorKind::Exists);
    let entry = session
        .runtime
        .wait(stat(&session.sftp, "/srv/up.bin"), &|| false);
    let entry = entry.unwrap().unwrap();
    assert_eq!(
        (entry.name.as_str(), entry.size, entry.is_dir),
        ("up.bin", 7, false)
    );
    let none = session
        .runtime
        .wait(stat(&session.sftp, "/srv/none"), &|| false);
    assert_eq!(none.unwrap().unwrap_err().kind, ErrorKind::NotFound);
}

/// S3 must know a file's size first: the pieces are kept on the disk and sent whole at the end (nothing
/// on the server before it); the spool removed after.
#[test]
fn a_file_of_a_protocol_needing_its_size_is_sent_whole_at_its_end() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let session = folders.s3_session();
    let mut files = Files::default();
    let fs = &session.sftp;
    let path = "/srv/kept.txt";
    done(&session, files.write(fs, path, WriteMode::New, b""));
    done(&session, files.write(fs, path, WriteMode::Append, b"kept "));
    done(&session, files.write(fs, path, WriteMode::Append, b"whole"));
    assert!(
        !srv.join("kept.txt").exists(),
        "nothing sent before its end"
    );
    done(&session, files.end(fs, path));
    assert_eq!(
        fs::read_to_string(srv.join("kept.txt")).unwrap(),
        "kept whole"
    );
    assert_eq!(
        read_all(&session, &mut files, "/srv/kept.txt", 3),
        b"kept whole"
    );
    let spools = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(&format!("netway-{}-", std::process::id())) && name.ends_with(".spool")
        })
        .count();
    assert_eq!(spools, 0, "the spool removed");
}

/// `files`' spool, which must be there.
fn spool_of(files: &Files) -> std::path::PathBuf {
    let spool = files.spool().map(std::path::Path::to_path_buf);
    let spool = spool.unwrap();
    assert!(spool.exists());
    spool
}

/// A copy abandoned (API 9's `WRITE_ABORT`) while kept on the disk: nothing sent, its spool removed;
/// one made anew while another was kept lets the other go the same way; so does the session's end.
#[test]
fn a_file_abandoned_is_never_sent_and_its_spool_goes() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let session = folders.s3_session();
    let fs = &session.sftp;
    let mut files = Files::default();
    done(
        &session,
        files.write(fs, "/srv/a.txt", WriteMode::New, b"part"),
    );
    let spool = spool_of(&files);
    files.abort("/srv/other.txt");
    assert!(spool.exists(), "another file's abandonment keeps it");
    files.abort("/srv/a.txt");
    assert!(!spool.exists(), "removed");
    done(&session, files.end(fs, "/srv/a.txt"));
    assert!(!srv.join("a.txt").exists(), "never sent");
    done(
        &session,
        files.write(fs, "/srv/b.txt", WriteMode::New, b"b"),
    );
    let first = spool_of(&files);
    done(
        &session,
        files.write(fs, "/srv/c.txt", WriteMode::New, b"c"),
    );
    assert!(!first.exists(), "let go by the next file made");
    assert!(!srv.join("b.txt").exists(), "and never sent");
    let second = spool_of(&files);
    drop(files);
    assert!(!second.exists(), "removed at the session's end");
    assert!(!srv.join("c.txt").exists());
}

/// A write that failed keeps nothing open: an `Append` after it opens the file again at its end.
#[test]
fn a_failed_write_leaves_nothing_open() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    let session = folders.session();
    let fs = &session.sftp;
    let mut files = Files::default();
    let none = files.write(fs, "/srv/none/x.txt", WriteMode::New, b"x");
    assert!(session.runtime.wait(none, &|| false).unwrap().is_err());
    assert!(!files.writes(), "nothing kept");
    fs::write(srv.join("x.txt"), "one").unwrap();
    done(
        &session,
        files.write(fs, "/srv/x.txt", WriteMode::Append, b" two"),
    );
    assert!(files.writes());
    done(&session, files.end(fs, "/srv/x.txt"));
    assert!(!files.writes());
    assert_eq!(fs::read_to_string(srv.join("x.txt")).unwrap(), "one two");
}

/// An upload's sources named from the folder that holds them all; on two disks none holds them.
#[test]
fn an_uploads_sources_are_named_from_their_common_folder() {
    use crate::background_panel::common_folder;
    use std::path::PathBuf;
    let base = std::env::temp_dir().join("a");
    let one = [base.join("x.txt"), base.join("y.txt")];
    assert_eq!(
        common_folder(&one),
        Some((base.clone(), vec!["x.txt".into(), "y.txt".into()]))
    );
    let deeper = [base.join("b").join("x.txt"), base.join("c").join("y.txt")];
    let (folder, names) = common_folder(&deeper).unwrap();
    assert_eq!(folder, base);
    assert_eq!(
        names,
        [
            PathBuf::from("b").join("x.txt"),
            PathBuf::from("c").join("y.txt")
        ]
        .map(|name| name.display().to_string())
    );
    if cfg!(windows) {
        let disks = [PathBuf::from(r"C:\a\x.txt"), PathBuf::from(r"D:\a\y.txt")];
        assert_eq!(common_folder(&disks), None);
    }
    assert_eq!(common_folder(&[]), None);
}
