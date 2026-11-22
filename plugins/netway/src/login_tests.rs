//! Sessions started against the tests' server inside the process, the user's answers scripted.

use super::*;
use crate::ask::{Question, Reply};
use crate::runtime::Runtime;
use crate::ssh::Answer;
use crate::test_server::{PASSWORD, QUESTION, QUESTIONED_USER, TestServer, USER};

fn plan(port: u16, known: &Path) -> Plan {
    Plan {
        host: "127.0.0.1".to_string(),
        port,
        user: USER.to_string(),
        password: String::new(),
        key: None,
        known_hosts: known.to_path_buf(),
        folder: String::new(),
        protocol: crate::session::Protocol::Sftp,
        encryption: crate::session::Encryption::None,
        scp_fallback: false,
        options: Default::default(),
    }
}

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

/// `plan` started with `typed` given to each line asked (in turn; none left: Esc); the questions seen.
fn started(plan: &Plan, typed: &[&str]) -> (Result<Started, Failure>, Vec<Question>) {
    let runtime = Runtime::new().unwrap();
    let (asker, mut questions) = Asker::new();
    let mut typed = typed.iter().map(|text| text.to_string());
    let mut seen = Vec::new();
    let mut answer = |question: Question| {
        seen.push(question.clone());
        match question {
            Question::HostKey(_) => Reply::HostKey(Answer::Keep),
            Question::Certificate(_) => Reply::HostKey(Answer::Once),
            Question::Text { .. } => Reply::Text(typed.next()),
            Question::Questions(_) => Reply::Answers(typed.next().map(|text| vec![text])),
            Question::Status(_) | Question::Transfer(_) => Reply::Nothing,
        }
    };
    let work = async { start(plan, &asker).await };
    let done = runtime
        .wait_answering(work, &|| false, &mut questions, &mut answer)
        .unwrap();
    (done, seen)
}

fn texts(seen: &[Question]) -> Vec<String> {
    (seen.iter())
        .filter_map(|question| match question {
            Question::Text { prompt, .. } => Some(prompt.clone()),
            Question::Questions(questions) => Some(questions.prompts[0].0.clone()),
            _ => None,
        })
        .collect()
}

/// The session's password logs in with no question but the host key's; the statuses in WinSCP's order;
/// the home folder opened.
#[test]
fn a_kept_password_logs_in_without_asking() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.password = PASSWORD.to_string();
    let (done, seen) = started(&plan, &[]);
    let started = done.unwrap();
    assert_eq!(
        (started.folder.as_str(), started.user.as_str()),
        ("/", USER)
    );
    let statuses: Vec<&Question> = (seen.iter())
        .filter(|question| matches!(question, Question::Status(_)))
        .collect();
    assert_eq!(
        statuses,
        [CONNECTING, AUTHENTICATING, STARTING]
            .map(|text| Question::Status(text.to_string()))
            .iter()
            .collect::<Vec<_>>()
    );
    assert!(
        seen.iter()
            .any(|question| matches!(question, Question::HostKey(_)))
    );
    assert!(texts(&seen).is_empty());
}

/// No user and no password: both asked; a wrong password asked again after the server's question.
#[test]
fn the_user_and_the_password_are_asked() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.user = String::new();
    let (done, seen) = started(&plan, &[USER, "wrong", "still wrong", PASSWORD]);
    assert!(done.is_ok());
    assert_eq!(texts(&seen)[..2], [USER_PROMPT, PASSWORD_PROMPT]);
    assert_eq!(
        texts(&seen).last().map(String::as_str),
        Some(PASSWORD_PROMPT)
    );
}

/// Esc on the password stops; three wrong passwords are refused.
#[test]
fn esc_and_wrong_passwords_end_the_login() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let plan = plan(server.port, &temp.path().join("known_hosts"));
    let (done, _) = started(&plan, &[]);
    assert!(matches!(done, Err(Failure::Cancelled)), "{done:?}");
    let wrong = ["a", "b", "c", "d", "e", "f"];
    let (done, _) = started(&plan, &wrong);
    assert!(matches!(done, Err(Failure::Refused)), "{done:?}");
}

/// A user the server only asks its questions: the password typed answers the first one.
#[test]
fn the_servers_password_question_takes_the_password() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.user = QUESTIONED_USER.to_string();
    plan.password = PASSWORD.to_string();
    let (done, seen) = started(&plan, &[]);
    assert!(done.is_ok(), "{done:?}");
    assert!(
        texts(&seen).is_empty(),
        "answered by the password: {seen:?}"
    );
    // A wrong password: the password asked again, each typed one answering the server's question.
    plan.password = "wrong".to_string();
    let (done, seen) = started(&plan, &["wrong too", PASSWORD]);
    assert!(done.is_ok(), "{done:?}");
    assert_eq!(texts(&seen), [PASSWORD_PROMPT, PASSWORD_PROMPT]);
}

/// A round of the server's that is not one hidden question is shown to the user.
#[test]
fn other_questions_of_the_server_are_shown() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.user = QUESTIONED_USER.to_string();
    plan.password = PASSWORD.to_string();
    let runtime = Runtime::new().unwrap();
    let (asker, mut questions) = Asker::new();
    let mut shown = Vec::new();
    let mut answer = |question: Question| match question {
        Question::HostKey(_) => Reply::HostKey(Answer::Keep),
        Question::Questions(questions) => {
            shown.push(questions.prompts.clone());
            Reply::Answers(Some(vec![PASSWORD.to_string()]))
        }
        _ => Reply::Nothing,
    };
    let work = async {
        let mut opened = ssh::open(&plan.host, plan.port, &plan.known_hosts, asker.trust()).await?;
        let mut given = None::<String>;
        opened
            .keyboard_interactive(QUESTIONED_USER, async |questions: &Questions| {
                match given.take() {
                    Some(password) => Some(vec![password]),
                    None => asker.answers(questions).await,
                }
            })
            .await
    };
    let taken = runtime
        .wait_answering(work, &|| false, &mut questions, &mut answer)
        .unwrap()
        .unwrap();
    assert!(taken);
    assert_eq!(shown, [vec![(QUESTION.to_string(), false)]]);
}

/// A private key logs in; an encrypted one asks its passphrase.
#[test]
fn keys_log_in_and_an_encrypted_one_asks_its_passphrase() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.key = Some(testdata("user_ed25519"));
    let (done, seen) = started(&plan, &[]);
    assert!(done.is_ok(), "{done:?}");
    assert!(texts(&seen).is_empty());
    let encrypted = testdata("user_ed25519_encrypted");
    plan.key = Some(encrypted.clone());
    let (done, seen) = started(&plan, &["secret phrase"]);
    assert!(done.is_ok(), "{done:?}");
    assert_eq!(texts(&seen), [passphrase_prompt(&encrypted)]);
}

/// A folder given opens there, as the panel's path.
#[test]
fn the_folder_given_is_opened() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.password = PASSWORD.to_string();
    plan.folder = "/docs/".to_string();
    let (done, _) = started(&plan, &[]);
    assert_eq!(done.unwrap().folder, "/docs");
}

/// A server without SFTP: the session falls back to SCP when it allows it (WinSCP's `fsSFTP`), else
/// fails with WinSCP's text; an SCP session never asks for SFTP.
#[test]
fn without_sftp_the_session_falls_back_to_scp_when_allowed() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("docs")).unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.user = crate::test_server::SHELL_USER.to_string();
    plan.password = PASSWORD.to_string();
    plan.folder = "docs".to_string();
    let (refused, _) = started(&plan, &[]);
    let refused = refused.map(|started| started.folder).unwrap_err();
    assert!(matches!(refused, Failure::Connection(text) if text == crate::ssh::SFTP_REFUSED));
    plan.scp_fallback = true;
    let fallen = started(&plan, &[]).0.unwrap();
    assert_eq!(
        (fallen.fs.protocol(), fallen.folder.as_str()),
        ("SCP", "/docs")
    );
    plan.scp_fallback = false;
    plan.protocol = crate::session::Protocol::Scp;
    plan.user = USER.to_string();
    assert_eq!(started(&plan, &[]).0.unwrap().fs.protocol(), "SCP");
}

/// The session's "SFTP server" runs SFTP as that command where the server has no `sftp` subsystem.
#[test]
fn an_sftp_server_command_starts_sftp() {
    let temp = tempfile::tempdir().unwrap();
    let server = TestServer::start(temp.path());
    let mut plan = plan(server.port, &temp.path().join("known_hosts"));
    plan.user = crate::test_server::SHELL_USER.to_string();
    plan.password = PASSWORD.to_string();
    plan.options.sftp_server = "/usr/lib/openssh/sftp-server".to_string();
    let started = started(&plan, &[]).0.unwrap();
    assert_eq!(
        (started.fs.protocol(), started.folder.as_str()),
        ("SFTP", "/")
    );
}

#[test]
fn a_folder_as_the_panels_path() {
    assert_eq!(panel_folder("/home/me/"), "/home/me");
    assert_eq!(panel_folder("/home/me"), "/home/me");
    assert_eq!(panel_folder("/"), "/");
    assert_eq!(panel_folder("//"), "/");
}

#[test]
fn a_password_question_is_one_hidden_prompt() {
    let questions = |prompts: Vec<(String, bool)>| Questions {
        name: String::new(),
        instructions: String::new(),
        prompts,
    };
    assert!(is_password_question(&questions(vec![(
        "Password: ".into(),
        false
    )])));
    assert!(!is_password_question(&questions(vec![(
        "Code: ".into(),
        true
    )])));
    let two = vec![("a".to_string(), false), ("b".to_string(), false)];
    assert!(!is_password_question(&questions(two)));
}

#[path = "link_login_tests.rs"]
mod link;
