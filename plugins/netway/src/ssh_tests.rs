//! Connections to the tests' server inside the process: the logins, the host key's questions, a listing.

use super::*;
use crate::runtime::Runtime;
use crate::test_server::{PASSWORD, QUESTION, QUESTIONED_USER, TestServer, USER};
use std::sync::atomic::{AtomicUsize, Ordering};

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

fn login(port: u16, auth: Auth) -> Login {
    Login {
        host: "127.0.0.1".to_string(),
        port,
        user: USER.to_string(),
        auth,
    }
}

/// A trust answering `answer`, counting the questions and keeping the last key asked about.
fn trust(answer: Answer) -> (Trust, Arc<AtomicUsize>, Arc<Mutex<Option<HostKey>>>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let last = Arc::new(Mutex::new(None));
    let (count, keep) = (Arc::clone(&asked), Arc::clone(&last));
    let trust = trust_by(move |key: &HostKey| {
        count.fetch_add(1, Ordering::SeqCst);
        *keep.lock().unwrap() = Some(key.clone());
        answer
    });
    (trust, asked, last)
}

/// A connection by `auth` to a fresh server over `root`, the host key kept in `known`.
fn connected(root: &Path, known: &Path, auth: Auth) -> Result<(Runtime, Connection), Failure> {
    let server = TestServer::start(root);
    let runtime = Runtime::new().unwrap();
    let (trust, _, _) = trust(Answer::Keep);
    let connection = runtime
        .wait(connect(&login(server.port, auth), known, trust), &|| false)
        .unwrap()?;
    std::mem::forget(server);
    Ok((runtime, connection))
}

/// A password logs in; the root's items are listed with their kinds and sizes, "." and ".." left out.
#[test]
fn a_password_logs_in_and_the_root_is_listed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("a.txt"), b"hello").unwrap();
    let known = temp.path().join("known_hosts");
    let (runtime, connection) =
        connected(&root, &known, Auth::Password(PASSWORD.to_string())).unwrap();
    let listed = runtime
        .wait(
            async {
                let sftp = connection.sftp("").await?;
                let home = crate::sftp::home(&sftp)
                    .await
                    .map_err(Failure::Connection)?;
                let items = crate::sftp::list(&sftp, &home)
                    .await
                    .map_err(Failure::Connection)?;
                Ok::<_, Failure>((home, items))
            },
            &|| false,
        )
        .unwrap()
        .unwrap();
    assert_eq!(listed.0, "/");
    let mut items = listed.1;
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let seen: Vec<_> = items
        .iter()
        .map(|i| (i.name.as_str(), i.is_dir, i.size))
        .collect();
    assert_eq!(seen[0], ("a.txt", false, 5));
    assert_eq!((seen[1].0, seen[1].1), ("docs", true));
    assert_eq!(seen.len(), 2, "{seen:?}");
    runtime.wait(connection.close(), &|| false).unwrap();
}

#[test]
fn a_wrong_password_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let failed = connected(temp.path(), &known, Auth::Password("wrong".to_string())).unwrap_err();
    assert!(matches!(failed, Failure::Refused), "{failed:?}");
}

/// The user's keys: OpenSSH's, an encrypted one with its passphrase, PuTTY's `.ppk` (version 3).
#[test]
fn private_keys_log_in_openssh_encrypted_and_putty() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let keys = [
        ("user_ed25519", None),
        ("user_ed25519_encrypted", Some("secret phrase")),
        ("user_ed25519.ppk", None),
    ];
    for (name, passphrase) in keys {
        let auth = Auth::Key {
            path: testdata(name),
            passphrase: passphrase.map(str::to_string),
        };
        let done = connected(temp.path(), &known, auth);
        assert!(done.is_ok(), "{name}: {:?}", done.err());
    }
}

/// An encrypted key without its passphrase cannot be read: the key's failure, not the server's refusal.
#[test]
fn an_encrypted_key_without_its_passphrase_is_the_keys_failure() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let auth = Auth::Key {
        path: testdata("user_ed25519_encrypted"),
        passphrase: None,
    };
    let failed = connected(temp.path(), &known, auth).unwrap_err();
    assert!(matches!(failed, Failure::Key(_)), "{failed:?}");
}

/// A new host's key is asked about with its fingerprint; kept, the next connection asks nothing.
#[test]
fn a_new_host_key_is_asked_once_when_kept() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let server = TestServer::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let password = || Auth::Password(PASSWORD.to_string());
    let (keep, asked, last) = trust(Answer::Keep);
    let first = runtime.wait(
        connect(&login(server.port, password()), &known, keep.clone()),
        &|| false,
    );
    assert!(first.unwrap().is_ok());
    let key = last.lock().unwrap().clone().unwrap();
    assert_eq!(key.known, Known::New);
    assert_eq!(key.fingerprint, TestServer::fingerprint());
    assert!(
        key.fingerprint.starts_with("SHA256:"),
        "{}",
        key.fingerprint
    );
    assert_eq!(key.algorithm, "ssh-ed25519");
    let again = runtime.wait(
        connect(&login(server.port, password()), &known, keep),
        &|| false,
    );
    assert!(again.unwrap().is_ok());
    assert_eq!(
        asked.load(Ordering::SeqCst),
        1,
        "a kept key is not asked again"
    );
}

/// A key trusted once is not kept: asked again next time.
#[test]
fn a_key_trusted_once_is_asked_again() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let server = TestServer::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let (once, asked, _) = trust(Answer::Once);
    for _ in 0..2 {
        let auth = Auth::Password(PASSWORD.to_string());
        let done = runtime.wait(
            connect(&login(server.port, auth), &known, once.clone()),
            &|| false,
        );
        assert!(done.unwrap().is_ok());
    }
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    assert!(!known.exists(), "nothing kept");
}

/// A refused key ends the connection with it named; another key kept for the host is "changed".
#[test]
fn a_refused_key_fails_and_a_different_kept_key_is_changed() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let server = TestServer::start(temp.path());
    let other = std::fs::read_to_string(testdata("user_ed25519.pub")).unwrap();
    let other_key = other
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    std::fs::write(&known, format!("[127.0.0.1]:{} {other_key}\n", server.port)).unwrap();
    let runtime = Runtime::new().unwrap();
    let (refuse, _, _) = trust(Answer::Refuse);
    let auth = Auth::Password(PASSWORD.to_string());
    let failed = runtime
        .wait(connect(&login(server.port, auth), &known, refuse), &|| {
            false
        })
        .unwrap()
        .unwrap_err();
    match failed {
        Failure::HostKey(key) => assert_eq!(key.known, Known::Changed),
        other => panic!("{other:?}"),
    }
}

/// A changed key kept replaces the one kept for the host (another host's line and a comment stay): the
/// next connection asks nothing.
#[test]
fn a_changed_key_kept_replaces_the_old_one() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let server = TestServer::start(temp.path());
    let other = std::fs::read_to_string(testdata("user_ed25519.pub")).unwrap();
    let other_key = other
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let lines = format!(
        "# kept\nelsewhere.example {other_key}\n[127.0.0.1]:{} {other_key}\n",
        server.port
    );
    std::fs::write(&known, lines).unwrap();
    let runtime = Runtime::new().unwrap();
    let user = login(server.port, Auth::Password(PASSWORD.to_string()));
    let (keep, asked, last) = trust(Answer::Keep);
    let first = connect(&user, &known, keep);
    runtime.wait(first, &|| false).unwrap().unwrap();
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(last.lock().unwrap().as_ref().unwrap().known, Known::Changed);
    let text = std::fs::read_to_string(&known).unwrap();
    assert!(text.starts_with("# kept\nelsewhere.example "), "{text}");
    assert_eq!(text.matches(&other_key).count(), 1, "{text}");
    let (again, asked_again, _) = trust(Answer::Refuse);
    let second = connect(&user, &known, again);
    runtime.wait(second, &|| false).unwrap().unwrap();
    assert_eq!(asked_again.load(Ordering::SeqCst), 0, "known now");
}

/// The server's questions answered (keyboard-interactive): the right answer logs in, a wrong one and
/// giving up do not; a password is refused to that user.
#[test]
fn the_servers_questions_log_in() {
    let temp = tempfile::tempdir().unwrap();
    let known = temp.path().join("known_hosts");
    let server = TestServer::start(temp.path());
    let runtime = Runtime::new().unwrap();
    let attempt = |answer: Option<&'static str>| {
        let (keep, _, _) = trust(Answer::Keep);
        let work = async {
            let mut opened = open("127.0.0.1", server.port, &known, keep).await?;
            let refused = opened.password(QUESTIONED_USER, PASSWORD).await?;
            let mut seen = Vec::new();
            let taken = opened
                .keyboard_interactive(QUESTIONED_USER, async |questions: &Questions| {
                    seen.push(questions.prompts.clone());
                    answer.map(|text| vec![text.to_string()])
                })
                .await?;
            Ok::<_, Failure>((refused, taken, seen))
        };
        runtime.wait(work, &|| false).unwrap().unwrap()
    };
    let (refused, taken, seen) = attempt(Some(PASSWORD));
    assert!(!refused, "no password for that user");
    assert!(taken);
    assert_eq!(seen, [vec![(QUESTION.to_string(), false)]]);
    assert!(!attempt(Some("wrong")).1);
    assert!(!attempt(None).1);
}

/// A host that drops the connection before speaking SSH is a connection's failure, not a hang. (The port
/// stays held by the test: a port freed for "nobody listens" could be taken by another test's server.)
#[test]
fn a_host_dropping_the_connection_is_a_connection_failure() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            drop(stream);
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let runtime = Runtime::new().unwrap();
    let (keep, _, _) = trust(Answer::Keep);
    let auth = Auth::Password(PASSWORD.to_string());
    let failed = runtime
        .wait(
            connect(&login(port, auth), &temp.path().join("k"), keep),
            &|| false,
        )
        .unwrap()
        .unwrap_err();
    assert!(matches!(failed, Failure::Connection(_)), "{failed:?}");
}
