//! The transfers and operations over FTP against the tests' FTP server.

use super::*;
use crate::test_ftp::TestFtp;

impl Folders {
    /// A session over FTP on the folders' server root.
    fn ftp_session(&self) -> Session {
        let server = TestFtp::start(&self.root());
        let runtime = Runtime::new().unwrap();
        let link = crate::link_options::Link::default();
        let options = crate::protocol_options::Ftp::default();
        let connect = crate::ftp::Connect {
            host: "127.0.0.1",
            port: server.port,
            encryption: crate::session::Encryption::None,
            link: &link,
            options: &options,
            tls: None,
        };
        let work = async {
            let stream = crate::ftp::open(&connect).await?;
            crate::ftp::Ftp::log_in(stream, ("user", "pass"), 0, &options).await
        };
        let ftp = runtime.wait(work, &|| false).unwrap().unwrap();
        Session {
            sftp: crate::fs::Fs::ftp(ftp, &Default::default()),
            _connection: None,
            runtime,
            _server: Box::new(server),
        }
    }
}

/// Files and folders down and up over FTP; a file there asked over with Append; deleting a folder.
#[test]
fn files_and_folders_go_both_ways_and_are_deleted() {
    let folders = Folders::new();
    let srv = folders.root().join("srv");
    write(&srv.join("a.txt"), "alpha", 1_000_000_000);
    write(&srv.join("docs/deep/b.txt"), "beta", 1_100_000_000);
    let session = folders.ftp_session();
    assert_eq!(session.sftp.protocol(), "FTP");
    let work = Work::Down("/srv", names(&["a.txt", "docs"]), folders.out());
    let (done, seen) = run(&session, Setup::default(), work, Vec::new());
    assert_eq!(done, Ok(()), "{seen:?}");
    let out = folders.out();
    assert_eq!(fs::read_to_string(out.join("a.txt")).unwrap(), "alpha");
    assert_eq!(
        fs::read_to_string(out.join("docs/deep/b.txt")).unwrap(),
        "beta"
    );
    write(&out.join("a.txt"), "alpha and more", 1_200_000_000);
    let up = Work::Up(vec![out.join("a.txt")], "/srv");
    let replies = vec![TransferReply::Overwrite(Overwrite::Yes)];
    let (done, seen) = run(&session, Setup::default(), up, replies);
    assert_eq!(done, Ok(()));
    let TransferQuestion::Overwrite { append, .. } = &seen[0] else {
        panic!("asked {seen:?}");
    };
    assert!(append, "FTP appends (APPE)");
    assert_eq!(
        fs::read_to_string(srv.join("a.txt")).unwrap(),
        "alpha and more"
    );
    let delete = Work::Delete("/srv", names(&["docs"]));
    assert_eq!(
        run(&session, Setup::default(), delete, Vec::new()).0,
        Ok(())
    );
    assert!(!srv.join("docs").exists());
}
