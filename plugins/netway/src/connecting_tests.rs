use super::*;

fn key(known: Known) -> HostKey {
    HostKey {
        host: "example.com".to_string(),
        port: 22,
        algorithm: "ssh-ed25519".to_string(),
        fingerprint: "SHA256:abc".to_string(),
        known,
    }
}

/// WinSCP's questions with the key's details, wrapped at the width; the changed key's without "Add".
#[test]
fn the_host_keys_questions_are_winscps() {
    let unknown = unknown_key_text(&key(Known::New));
    assert_eq!(
        unknown[0],
        "Continue connecting to an unknown server and add its host key to a"
    );
    assert!(unknown.contains(&"SHA256:abc".to_string()));
    assert!(
        unknown
            .iter()
            .any(|line| line.contains("ssh-ed25519 key details"))
    );
    assert!(unknown.iter().all(|line| line.chars().count() <= 66));
    let changed = changed_key_text(&key(Known::Changed));
    assert_eq!(changed[0], "WARNING - POTENTIAL SECURITY BREACH!");
    assert!(
        changed
            .iter()
            .any(|line| line.contains("NetWay has in cache"))
    );
    assert!(!changed.iter().any(|line| line.contains("Add")));
}

#[test]
fn paragraphs_wrap_and_stay_apart() {
    let lines = wrapped(&format!("{}\n\nend", "word ".repeat(20)));
    assert_eq!(lines.len(), 4);
    assert!(lines[0].chars().count() <= 66 && lines[0].ends_with("word"));
    assert_eq!(&lines[2..], ["", "end"]);
}

#[test]
fn the_buttons_answer_keep_once_refuse() {
    assert_eq!(key_answer(Some(0)), Answer::Keep);
    assert_eq!(key_answer(Some(1)), Answer::Once);
    assert_eq!(key_answer(Some(2)), Answer::Refuse);
    assert_eq!(key_answer(None), Answer::Refuse, "Esc");
}

#[test]
fn a_stop_says_nothing_a_refusal_access_denied() {
    assert_eq!(failure_text(&Failure::Cancelled), None);
    assert_eq!(
        failure_text(&Failure::Refused).as_deref(),
        Some("Access denied.")
    );
    let lost = Failure::Connection("refused".to_string());
    assert_eq!(
        failure_text(&lost).as_deref(),
        Some("Network error: refused")
    );
}
