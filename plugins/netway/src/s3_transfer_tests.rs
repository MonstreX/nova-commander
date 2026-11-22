//! The transfers and operations over S3 against the tests' S3 server (its buckets: the root's folders).

use super::*;
use crate::test_s3::{ACCESS_KEY, SECRET, TestS3};

impl Folders {
    /// A session over S3 on the folders' server root.
    pub(super) fn s3_session(&self) -> Session {
        let server = TestS3::start(&self.root());
        let link = crate::link_options::Link::default();
        let connect = crate::s3::Connect {
            host: "127.0.0.1",
            port: server.port,
            link: &link,
            tls: None,
            signer: crate::sigv4::Signer {
                access_key: ACCESS_KEY.to_string(),
                secret: SECRET.to_string(),
                region: crate::s3_login::DEFAULT_REGION.to_string(),
            },
            options: &Default::default(),
        };
        let s3 = crate::s3::S3::new(connect).unwrap();
        Session {
            sftp: crate::fs::Fs::s3(s3, &Default::default()),
            _connection: None,
            runtime: Runtime::new().unwrap(),
            _server: Box::new(server),
        }
    }
}

/// Files and folders down and up over S3, a file duplicated by CopyObject and a folder through the client,
/// a file renamed (copied and deleted), all deleted.
#[test]
fn files_and_folders_go_both_ways_are_copied_and_deleted() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    write(&srv.join("docs/deep/b.txt"), "beta", 1_100_000_000);
    let session = folders.s3_session();
    assert_eq!(session.sftp.protocol(), "S3");
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
    write(&up.join("empty.txt"), "", 1_200_000_000);
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
    assert_eq!(fs::read(srv.join("up/empty.txt")).unwrap(), b"");
    let copy = Work::Move("/srv", names(&["a.txt", "docs"]), ("/srv/up", "*"), true);
    assert_eq!(run(&session, Setup::default(), copy, Vec::new()).0, Ok(()));
    assert_eq!(fs::read_to_string(srv.join("up/a.txt")).unwrap(), "alpha");
    assert!(
        srv.join("up/docs/deep/b.txt").exists(),
        "a folder's objects"
    );
    let rename = Work::Move("/srv/up", names(&["a.txt"]), ("/srv/up", "z.txt"), false);
    assert_eq!(
        run(&session, Setup::default(), rename, Vec::new()).0,
        Ok(())
    );
    assert!(srv.join("up/z.txt").exists() && !srv.join("up/a.txt").exists());
    let delete = Work::Delete("/srv", names(&["up"]));
    assert_eq!(
        run(&session, Setup::default(), delete, Vec::new()).0,
        Ok(())
    );
    assert!(!srv.join("up/inner/c.txt").exists());
}
