//! The network's questions to the user (ADR-0023 §2): the work on the runtime's threads asks, the plugin's
//! thread - waiting for that work ([`crate::runtime::Runtime::wait_answering`]) - shows the question in
//! the program's windows and sends the answer back. Statuses ("Connecting to host...") go the same way
//! with no answer waited for.

use tokio::sync::{mpsc, oneshot};

use crate::copy_questions::{TransferQuestion, TransferReply};
use crate::ssh::{Answer, HostKey, Questions, Trust};

/// What the work asks or tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Where the work is, shown until the next.
    Status(String),
    /// A host key new or changed.
    HostKey(HostKey),
    /// A line typed: the user's name, a password, a key's passphrase.
    Text { prompt: String, secret: bool },
    /// A round of the server's questions (keyboard-interactive).
    Questions(Questions),
    /// A transfer's question: a file there, a failure.
    Transfer(TransferQuestion),
    /// An FTPS certificate that does not verify (answered as a host key: kept, this time, refused).
    Certificate(crate::ftp_login::Certificate),
}

/// The user's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// For a status, or no one answered.
    Nothing,
    HostKey(Answer),
    /// The line typed; none: Esc.
    Text(Option<String>),
    /// The answers in the questions' order; none: Esc.
    Answers(Option<Vec<String>>),
    Transfer(TransferReply),
}

/// A question with where its answer goes.
pub type Asked = (Question, oneshot::Sender<Reply>);

/// The work's side of the questions.
#[derive(Debug, Clone)]
pub struct Asker {
    sender: mpsc::UnboundedSender<Asked>,
}

impl Asker {
    /// An asker and the plugin thread's side of its questions.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Asked>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    /// `question` asked: the answer, [`Reply::Nothing`] when no one is there to answer.
    pub async fn ask(&self, question: Question) -> Reply {
        let (answer, answered) = oneshot::channel();
        if self.sender.send((question, answer)).is_err() {
            return Reply::Nothing;
        }
        answered.await.unwrap_or(Reply::Nothing)
    }

    /// Where the work is, told without waiting.
    pub fn tell(&self, status: &str) {
        let (answer, _) = oneshot::channel();
        let _ = self
            .sender
            .send((Question::Status(status.to_string()), answer));
    }

    /// A line asked; none: Esc (or no one answered).
    pub async fn text(&self, prompt: &str, secret: bool) -> Option<String> {
        let question = Question::Text {
            prompt: prompt.to_string(),
            secret,
        };
        match self.ask(question).await {
            Reply::Text(text) => text,
            _ => None,
        }
    }

    /// The server's questions asked; none: Esc.
    pub async fn answers(&self, questions: &Questions) -> Option<Vec<String>> {
        match self.ask(Question::Questions(questions.clone())).await {
            Reply::Answers(answers) => answers,
            _ => None,
        }
    }

    /// The host key's questions asked of the user; no answer refuses the key.
    pub fn trust(&self) -> Trust {
        let asker = self.clone();
        std::sync::Arc::new(move |key: HostKey| {
            let asker = asker.clone();
            Box::pin(async move {
                match asker.ask(Question::HostKey(key)).await {
                    Reply::HostKey(answer) => answer,
                    _ => Answer::Refuse,
                }
            })
        })
    }
}

/// WinSCP's "&Passphrase for key '%s':".
pub fn passphrase_prompt(key: &std::path::Path) -> String {
    format!("&Passphrase for key '{}':", key.display())
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
