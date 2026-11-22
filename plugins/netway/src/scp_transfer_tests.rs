//! The transfers and operations over SCP: the tests' server's user without SFTP, its shell's commands.

use super::*;
use crate::session_options::Options;
use crate::test_server::SHELL_USER;

impl Folders {
    /// A connection over SCP (the server's shell user).
    fn scp_session(&self) -> Session {
        self.session_with(SHELL_USER, &Options::default())
    }

    /// A connection as `user` with the session's `options`: SCP for the shell user, else SFTP.
    fn session_with(&self, user: &str, options: &Options) -> Session {
        let server = TestServer::start(&self.root());
        let runtime = Runtime::new().unwrap();
        let login = Login {
            host: "127.0.0.1".to_string(),
            port: server.port,
            user: user.to_string(),
            auth: Auth::Password(PASSWORD.to_string()),
        };
        let trust = trust_by(|_| Answer::Keep);
        let known = self.temp.path().join("known_hosts");
        let connection = (runtime.wait(connect(&login, &known, trust), &|| false))
            .unwrap()
            .unwrap();
        let commands = connection.commands().with_shell(&options.shell);
        let fs = match user {
            SHELL_USER => crate::fs::Fs::scp(commands, options),
            _ => {
                let sftp = runtime.wait(connection.sftp(""), &|| false);
                crate::fs::Fs::sftp(sftp.unwrap().unwrap(), Some(commands), options)
            }
        };
        Session {
            sftp: fs,
            _connection: Some(connection),
            runtime,
            _server: Box::new(server),
        }
    }
}

/// The server's folder `folder` listed by the session.
fn listed(session: &Session, folder: &str) -> Vec<crate::sftp::RemoteEntry> {
    let list = session.sftp.list(folder);
    session.runtime.wait(list, &|| false).unwrap().unwrap()
}

/// The names in the server's folder `folder` of the disk.
fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = (fs::read_dir(folder).unwrap())
        .map(|item| item.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The SCP/Shell tab's shell runs the commands; without the full timestamp the listing's wall clock is
/// read in the client's zone and shifted by the time zone offset.
#[test]
fn the_shell_listing_and_time_offset_apply() {
    let folders = Folders::new();
    write(&folders.root().join("srv/a.txt"), "alpha", 1_546_646_400);
    let options = Options {
        shell: "/bin/sh".to_string(),
        ls_full_time: false,
        time_difference: 60,
        ..Options::default()
    };
    let session = folders.session_with(SHELL_USER, &options);
    let read = listed(&session, "/srv");
    let a = read.iter().find(|item| item.name == "a.txt").unwrap();
    // 2019-01-05 00:00 UTC listed as "Jan  5  2019": midnight there, in the client's zone, an hour on.
    let expected = crate::scp::local_to_utc(1_546_646_400) + 3600;
    assert_eq!(a.modified, Some(u32::try_from(expected).unwrap()));
    let full = folders.session_with(SHELL_USER, &Options::default());
    let read = listed(&full, "/srv");
    let a = read.iter().find(|item| item.name == "a.txt").unwrap();
    assert_eq!(
        a.modified,
        Some(1_546_646_400),
        "a zoned time is not shifted"
    );
}

/// "Preserve deleted remote files to recycle bin": F8 moves into the bin as `name-yyyymmdd-hhnnss.ext`;
/// in the bin itself it deletes.
#[test]
fn deleting_moves_into_the_recycle_bin() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    fs::create_dir_all(srv.join("bin")).unwrap();
    let options = Options {
        delete_to_recycle_bin: true,
        recycle_bin_path: "/srv/bin".to_string(),
        ..Options::default()
    };
    let session = folders.session_with(SHELL_USER, &options);
    let delete = Work::Delete("/srv", names(&["a.txt"]));
    assert_eq!(
        run(&session, Setup::default(), delete, Vec::new()).0,
        Ok(())
    );
    assert!(!srv.join("a.txt").exists());
    let binned = names_in(&srv.join("bin"));
    assert_eq!(binned.len(), 1);
    assert!(
        binned[0].starts_with("a-") && binned[0].ends_with(".txt"),
        "{binned:?}"
    );
    let again = Work::Delete("/srv/bin", binned.clone());
    assert_eq!(run(&session, Setup::default(), again, Vec::new()).0, Ok(()));
    assert!(names_in(&srv.join("bin")).is_empty(), "deleted for real");
}

/// "Preserve overwritten remote files to bin" (SFTP): the file replaced goes into the bin first.
#[test]
fn an_overwritten_file_goes_into_the_bin_over_sftp() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "old", 1_000_000_000);
    fs::create_dir_all(srv.join("bin")).unwrap();
    write(&folders.out().join("a.txt"), "new", 1_100_000_000);
    let options = Options {
        overwritten_to_recycle_bin: true,
        recycle_bin_path: "/srv/bin".to_string(),
        ..Options::default()
    };
    let session = folders.session_with(USER, &options);
    let work = Work::Up(vec![folders.out().join("a.txt")], "/srv");
    let replies = vec![TransferReply::Overwrite(Overwrite::Yes)];
    assert_eq!(run(&session, Setup::default(), work, replies).0, Ok(()));
    assert_eq!(fs::read_to_string(srv.join("a.txt")).unwrap(), "new");
    let binned = names_in(&srv.join("bin"));
    assert_eq!(binned.len(), 1, "{binned:?}");
    assert_eq!(
        fs::read_to_string(srv.join("bin").join(&binned[0])).unwrap(),
        "old"
    );
}

/// "New name" with the overwritten files' bin: what is under the new name goes into the bin under its own
/// name, the file asked about stays; a new name with nothing under it bins nothing and asks nothing more.
#[test]
fn a_new_name_bins_what_it_replaces() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "server a", 1_000_000_000);
    write(&srv.join("other.txt"), "server other", 1_000_000_000);
    fs::create_dir_all(srv.join("bin")).unwrap();
    write(&folders.out().join("a.txt"), "new", 1_100_000_000);
    let options = Options {
        overwritten_to_recycle_bin: true,
        recycle_bin_path: "/srv/bin".to_string(),
        ..Options::default()
    };
    let session = folders.session_with(USER, &options);
    let up = || Work::Up(vec![folders.out().join("a.txt")], "/srv");
    let renamed = |name: &str| {
        vec![
            TransferReply::Overwrite(Overwrite::NewName),
            TransferReply::NewName(Some(name.to_string())),
        ]
    };
    let (done, seen) = run(&session, Setup::default(), up(), renamed("other.txt"));
    assert_eq!((done, seen.len()), (Ok(()), 2), "{seen:?}");
    assert_eq!(fs::read_to_string(srv.join("a.txt")).unwrap(), "server a");
    assert_eq!(fs::read_to_string(srv.join("other.txt")).unwrap(), "new");
    let binned = names_in(&srv.join("bin"));
    assert_eq!(binned.len(), 1, "{binned:?}");
    assert!(binned[0].starts_with("other-"), "{binned:?}");
    assert_eq!(
        fs::read_to_string(srv.join("bin").join(&binned[0])).unwrap(),
        "server other"
    );
    let (done, seen) = run(&session, Setup::default(), up(), renamed("fresh.txt"));
    assert_eq!((done, seen.len()), (Ok(()), 2), "{seen:?}");
    assert_eq!(fs::read_to_string(srv.join("fresh.txt")).unwrap(), "new");
    assert_eq!(fs::read_to_string(srv.join("a.txt")).unwrap(), "server a");
    assert_eq!(names_in(&srv.join("bin")), binned, "nothing more binned");
}

/// Files and folders come down and go up over SCP, their times kept both ways.
#[test]
fn files_and_folders_go_both_ways_with_their_times() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    write(&srv.join("docs/deep/b.txt"), "beta", 1_100_000_000);
    let session = folders.scp_session();
    assert_eq!(session.sftp.protocol(), "SCP");
    let work = Work::Down("/srv", names(&["a.txt", "docs"]), folders.out());
    let (done, seen) = run(&session, Setup::default(), work, Vec::new());
    assert_eq!((done, seen), (Ok(()), Vec::new()));
    let out = folders.out();
    assert_eq!(fs::read_to_string(out.join("a.txt")).unwrap(), "alpha");
    let deep = out.join("docs/deep/b.txt");
    assert_eq!(fs::read_to_string(&deep).unwrap(), "beta");
    assert_eq!(modified(&out.join("a.txt")), 1_000_000_000);
    let up = out.join("up");
    fs::create_dir_all(up.join("inner")).unwrap();
    write(&up.join("inner/c.txt"), "gamma", 1_200_000_000);
    let work = Work::Up(vec![up], "/srv");
    let (done, _) = run(&session, Setup::default(), work, Vec::new());
    assert_eq!(done, Ok(()));
    let sent = srv.join("up/inner/c.txt");
    assert_eq!(fs::read_to_string(&sent).unwrap(), "gamma");
    assert_eq!(modified(&sent), 1_200_000_000);
}

/// A file there is asked over without Append (SCP writes no file from an offset).
#[test]
fn the_overwrite_question_has_no_append() {
    let folders = Folders::new();
    write(&folders.root().join("srv/a.txt"), "old", 1_000_000_000);
    write(&folders.out().join("a.txt"), "newer", 1_100_000_000);
    let session = folders.scp_session();
    let work = Work::Up(vec![folders.out().join("a.txt")], "/srv");
    let replies = vec![TransferReply::Overwrite(Overwrite::Yes)];
    let (done, seen) = run(&session, Setup::default(), work, replies);
    assert_eq!(done, Ok(()));
    let TransferQuestion::Overwrite { append, .. } = &seen[0] else {
        panic!("asked {seen:?}");
    };
    assert!(!append);
    let written = fs::read_to_string(folders.root().join("srv/a.txt")).unwrap();
    assert_eq!(written, "newer");
}

/// A folder made with its rights, items copied by `cp`, renamed by `mv` and deleted by `rm` / `rmdir`.
#[test]
fn the_servers_commands_make_copy_move_and_delete() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("docs/deep/b.txt"), "beta", 1_100_000_000);
    let session = folders.scp_session();
    let (done, _) = run(
        &session,
        Setup::default(),
        Work::Folder("/srv/new", Some(0o700)),
        Vec::new(),
    );
    assert_eq!(done, Ok(()));
    let made = srv.join("new");
    assert!(made.is_dir());
    assert_eq!(crate::test_server::rights_of(&made), Some(0o700));
    let copy = Work::Move("/srv", names(&["docs"]), ("/srv/new", "*"), true);
    assert_eq!(run(&session, Setup::default(), copy, Vec::new()).0, Ok(()));
    assert_eq!(
        fs::read_to_string(srv.join("new/docs/deep/b.txt")).unwrap(),
        "beta"
    );
    let rename = Work::Move("/srv", names(&["new"]), ("/srv", "renamed"), false);
    assert_eq!(
        run(&session, Setup::default(), rename, Vec::new()).0,
        Ok(())
    );
    assert!(srv.join("renamed/docs").is_dir() && !srv.join("new").exists());
    let delete = Work::Delete("/srv", names(&["renamed", "docs"]));
    assert_eq!(
        run(&session, Setup::default(), delete, Vec::new()).0,
        Ok(())
    );
    assert!(!srv.join("renamed").exists() && !srv.join("docs").exists());
}

/// Duplicating over SFTP goes through the server's `cp` too (WinSCP's secondary shell).
#[test]
fn sftp_duplicates_through_cp() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    let session = folders.session();
    let copy = Work::Move("/srv", names(&["a.txt"]), ("/srv/docs", "b.*"), true);
    assert_eq!(run(&session, Setup::default(), copy, Vec::new()).0, Ok(()));
    let copied = srv.join("docs/b.txt");
    assert_eq!(fs::read_to_string(&copied).unwrap(), "alpha");
    assert_eq!(modified(&copied), 1_000_000_000);
}
