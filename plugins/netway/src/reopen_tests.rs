use super::*;
use crate::runtime::Runtime;
use crate::ssh::Answer;
use crate::test_server::{PASSWORD, TestServer, USER};

#[test]
fn the_countdown_is_in_seconds() {
    assert_eq!(countdown(5), "Reconnecting in 5 s...");
}

/// A session whose server went away is told lost; logging in again with the kept plan at the same
/// place opens it, the password that logged in kept.
#[test]
fn a_lost_session_is_told_and_opened_again() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let server = TestServer::start(&root);
    let port = server.port;
    let runtime = Runtime::new().unwrap();
    let (asker, mut questions) = Asker::new();
    let plan = Plan {
        host: "127.0.0.1".to_string(),
        port,
        user: USER.to_string(),
        password: String::new(),
        key: None,
        known_hosts: temp.path().join("known_hosts"),
        folder: String::new(),
        protocol: crate::session::Protocol::Sftp,
        encryption: crate::session::Encryption::None,
        scp_fallback: false,
        options: Default::default(),
    };
    let mut answer = |question: crate::ask::Question| match question {
        crate::ask::Question::Text { .. } => crate::ask::Reply::Text(Some(PASSWORD.to_string())),
        crate::ask::Question::HostKey(_) => crate::ask::Reply::HostKey(Answer::Keep),
        _ => crate::ask::Reply::Nothing,
    };
    let work = async { login::start(&plan, &asker).await };
    let started = runtime
        .wait_answering(work, &|| false, &mut questions, &mut answer)
        .unwrap()
        .unwrap();
    assert_eq!(started.password, PASSWORD, "the password typed kept");
    let connection = started.connection.as_ref().unwrap();
    assert!(!connection.is_closed());
    drop(server);
    let mut closed = false;
    for _ in 0..100 {
        closed = connection.is_closed();
        if closed {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(closed, "the server went away: lost");
    let again = TestServer::start_at(&root, port);
    let kept = Plan {
        password: started.password.clone(),
        ..plan
    };
    let (asker, questions) = Asker::new();
    drop(questions);
    let work = async { login::start(&kept, &asker).await };
    let reopened = runtime.wait(work, &|| false).unwrap().unwrap();
    let reopened_connection = reopened.connection.as_ref().unwrap();
    assert!(!reopened_connection.is_closed(), "opened again unasked");
    drop(again);
}
