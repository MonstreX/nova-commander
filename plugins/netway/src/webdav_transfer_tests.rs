//! The transfers and operations over WebDAV against the tests' WebDAV server.

use super::*;
use crate::test_dav::TestDav;

impl Folders {
    /// A session over WebDAV on the folders' server root.
    fn dav_session(&self) -> Session {
        let server = TestDav::start(&self.root());
        let link = crate::link_options::Link::default();
        let connect = crate::webdav::Connect {
            host: "127.0.0.1",
            port: server.port,
            link: &link,
            tls: None,
            user: "user",
            password: "pass",
            compression: false,
        };
        let dav = crate::webdav::Dav::new(&connect).unwrap();
        Session {
            sftp: crate::fs::Fs::webdav(dav, &Default::default()),
            _connection: None,
            runtime: Runtime::new().unwrap(),
            _server: Box::new(server),
        }
    }
}

/// Files and folders down and up over WebDAV, a folder duplicated by COPY and deleted.
#[test]
fn files_and_folders_go_both_ways_are_copied_and_deleted() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    write(&srv.join("docs/deep/b.txt"), "beta", 1_100_000_000);
    let session = folders.dav_session();
    assert_eq!(session.sftp.protocol(), "WebDAV");
    let work = Work::Down("/srv", names(&["a.txt", "docs"]), folders.out());
    let (done, seen) = run(&session, Setup::default(), work, Vec::new());
    assert_eq!(done, Ok(()), "{seen:?}");
    let out = folders.out();
    assert_eq!(fs::read_to_string(out.join("a.txt")).unwrap(), "alpha");
    assert_eq!(
        fs::read_to_string(out.join("docs/deep/b.txt")).unwrap(),
        "beta"
    );
    let up = out.join("up");
    fs::create_dir_all(up.join("inner")).unwrap();
    write(&up.join("inner/c.txt"), "gamma", 1_200_000_000);
    let (done, seen) = run(
        &session,
        Setup::default(),
        Work::Up(vec![up], "/srv"),
        Vec::new(),
    );
    assert_eq!(done, Ok(()), "{seen:?}");
    assert_eq!(
        fs::read_to_string(srv.join("up/inner/c.txt")).unwrap(),
        "gamma"
    );
    let copy = Work::Move("/srv", names(&["docs"]), ("/srv/up", "*"), true);
    assert_eq!(run(&session, Setup::default(), copy, Vec::new()).0, Ok(()));
    assert!(srv.join("up/docs/deep/b.txt").exists(), "copied by COPY");
    let delete = Work::Delete("/srv", names(&["up"]));
    assert_eq!(
        run(&session, Setup::default(), delete, Vec::new()).0,
        Ok(())
    );
    assert!(!srv.join("up").exists());
}
