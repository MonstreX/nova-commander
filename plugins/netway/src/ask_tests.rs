use super::*;
use crate::runtime::Runtime;
use crate::ssh::Known;

fn key() -> HostKey {
    HostKey {
        host: "h".to_string(),
        port: 22,
        algorithm: "ssh-ed25519".to_string(),
        fingerprint: "SHA256:x".to_string(),
        known: Known::New,
    }
}

/// The work's questions are answered on the waiting thread, in their order; statuses come with no answer
/// waited for.
#[test]
fn questions_are_answered_while_the_work_waits() {
    let runtime = Runtime::new().unwrap();
    let (asker, mut questions) = Asker::new();
    let work = async move {
        asker.tell("Connecting to host...");
        let name = asker.text("&Username:", false).await;
        let trusted = (asker.trust())(key()).await;
        (name, trusted)
    };
    let mut seen = Vec::new();
    let mut answer = |question: Question| {
        seen.push(question.clone());
        match question {
            Question::Text { .. } => Reply::Text(Some("me".to_string())),
            Question::HostKey(_) => Reply::HostKey(Answer::Once),
            _ => Reply::Nothing,
        }
    };
    let done = runtime
        .wait_answering(work, &|| false, &mut questions, &mut answer)
        .unwrap();
    assert_eq!(done, (Some("me".to_string()), Answer::Once));
    assert_eq!(seen.len(), 3);
    assert_eq!(
        seen[0],
        Question::Status("Connecting to host...".to_string())
    );
    assert_eq!(seen[2], Question::HostKey(key()));
}

/// No one to answer: a line is none, a key refused.
#[test]
fn no_answer_refuses() {
    let runtime = Runtime::new().unwrap();
    let (asker, questions) = Asker::new();
    drop(questions);
    let work = async move {
        let text = asker.text("&Password:", true).await;
        let trusted = (asker.trust())(key()).await;
        let questions = crate::ssh::Questions {
            name: String::new(),
            instructions: String::new(),
            prompts: vec![("Password: ".to_string(), false)],
        };
        let answers = asker.answers(&questions).await;
        (text, trusted, answers)
    };
    let done = runtime.wait(work, &|| false).unwrap();
    assert_eq!(done, (None, Answer::Refuse, None));
}

#[test]
fn the_passphrase_prompt_names_the_key() {
    let prompt = passphrase_prompt(std::path::Path::new("id_ed25519"));
    assert_eq!(prompt, "&Passphrase for key 'id_ed25519':");
}
