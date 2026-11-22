use super::*;
use crate::runtime::Runtime;
use crate::test_ftp::TestFtp;

fn logged_in(runtime: &Runtime, port: u16) -> Ftp {
    logged_in_with(runtime, port, &Options::default())
}

fn logged_in_with(runtime: &Runtime, port: u16, options: &Options) -> Ftp {
    let link = Link::default();
    let connect = Connect {
        host: "127.0.0.1",
        port,
        encryption: Encryption::None,
        link: &link,
        options,
        tls: None,
    };
    let work = async {
        let stream = open(&connect).await?;
        Ftp::log_in(stream, ("user", "pass"), 0, options).await
    };
    runtime.wait(work, &|| false).unwrap().unwrap()
}

#[test]
fn mfmt_times_are_utc_digits() {
    let seconds = crate::scp_listing::seconds_of((2024, 3, 10), (10, 30, 45));
    assert_eq!(mfmt_time(u32::try_from(seconds).unwrap()), "20240310103045");
}

/// MLSD off: LIST read - with "-a" when on, by itself and settled when Auto, without it when off.
#[test]
fn list_follows_the_ftp_tab() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("srv")).unwrap();
    std::fs::write(temp.path().join("srv/a.txt"), "alpha").unwrap();
    let server = TestFtp::start(temp.path());
    let runtime = Runtime::new().unwrap();
    for list_all in [Switch::Auto, Switch::On, Switch::Off] {
        let options = Options {
            use_mlsd: Switch::Off,
            list_all,
            ..Options::default()
        };
        let ftp = logged_in_with(&runtime, server.port, &options);
        assert!(!ftp.mlsd);
        let listed = runtime.wait(ftp.list("/srv"), &|| false).unwrap().unwrap();
        assert!(
            listed.iter().any(|item| item.name == "a.txt"),
            "{list_all:?}: {listed:?}"
        );
        let settled = *ftp.list_all.lock().unwrap();
        assert_ne!(
            settled,
            Switch::Auto,
            "{list_all:?}: decided by the listing"
        );
        if list_all != Switch::Auto {
            assert_eq!(settled, list_all);
        }
    }
    let always = Options {
        use_mlsd: Switch::On,
        ..Options::default()
    };
    assert!(logged_in_with(&runtime, server.port, &always).mlsd);
}

/// A session idle long enough is asked by a NOOP whether it still answers; one used lately is not asked;
/// a server gone makes it lost, and lost it stays (its next look asks nothing).
#[test]
fn an_idle_session_is_asked_whether_it_still_answers() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestFtp::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let ftp = logged_in(&runtime, server.port);
    let wait = |work| runtime.wait(work, &|| false).unwrap();
    let long = Duration::from_secs(3_600);
    assert!(wait(ftp.alive(Duration::ZERO)), "it answers");
    drop(server);
    std::thread::sleep(Duration::from_millis(200));
    assert!(wait(ftp.alive(long)), "used lately: not asked");
    assert!(!wait(ftp.alive(Duration::ZERO)), "the server gone");
    assert!(!wait(ftp.alive(long)), "lost for good");
}

/// Against the tests' server: folders listed, made, renamed and removed; a file up, down and resumed.
#[test]
fn the_server_lists_makes_moves_and_transfers() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("srv")).unwrap();
    std::fs::write(temp.path().join("srv/a.txt"), "alpha").unwrap();
    let server = TestFtp::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let ftp = logged_in(&runtime, server.port);
    let work = async {
        let listed = ftp.list("/srv").await?;
        assert!(
            listed
                .iter()
                .any(|item| item.name == "a.txt" && item.size == 5)
        );
        ftp.mkdir("/srv/made").await?;
        ftp.rename("/srv/made", "/srv/moved").await?;
        let mut up = ftp.file("/srv/moved/b.txt", Way::Write);
        up.write_all(b"beta").await.map_err(|e| e.to_string())?;
        up.shutdown().await.map_err(|e| e.to_string())?;
        let mut down = ftp.file("/srv/moved/b.txt", Way::Read);
        down.seek(1).map_err(|e| e.to_string())?;
        let mut read = Vec::new();
        let mut buffer = [0u8; 64];
        loop {
            let count = down.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            read.extend_from_slice(&buffer[..count]);
        }
        assert_eq!(read, b"eta", "resumed from the offset");
        let entry = ftp.entry("/srv/moved/b.txt").await?;
        assert_eq!(entry.size, 4);
        ftp.remove("/srv/moved/b.txt").await?;
        ftp.rmdir("/srv/moved").await?;
        assert_eq!(ftp.canonicalize("/srv").await?, "/srv");
        Ok::<(), String>(())
    };
    runtime.wait(work, &|| false).unwrap().unwrap();
    assert!(!temp.path().join("srv/moved").exists());
}
