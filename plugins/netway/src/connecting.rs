//! Connecting (NetBox's `Connect`, WinSCP's `TSecureShell` prompts): a stored session (Enter, the session
//! dialog's Connect), a session the command line names, a session file of a disk (Enter on `*.netway`).
//! The login runs on the plugin's runtime while the plugin's thread shows its statuses and asks the user:
//! the host key's question (WinSCP's "Continue connecting to an unknown server..." and "WARNING -
//! POTENTIAL SECURITY BREACH!"), the user's name, the password, a key's passphrase, the server's questions.
//! Esc stops it. A session connected is a panel of its own (`remote`), its folder the session's or the
//! user's home folder.

use std::path::PathBuf;

use nc_plugin_sdk::{Host, InputOptions, Message};

use crate::NetWay;
use crate::address::Address;
use crate::ask::{Asker, Question, Reply};
use crate::login::{self, Plan};
use crate::remote::Remote;
use crate::runtime::Runtime;
use crate::ssh::{Answer, Failure, HostKey, Known, Questions};

/// The plugin's own known hosts, in its data folder.
pub const KNOWN_HOSTS: &str = "known_hosts";
/// The width the long questions are wrapped at.
const WIDTH: usize = 66;

/// WinSCP's `MSG_UNKNOWN_KEY4`, its buttons Yes / No / Cancel.
pub fn unknown_key_text(key: &HostKey) -> Vec<String> {
    let text = format!(
        "Continue connecting to an unknown server and add its host key to a cache?\n\n\
         The server's host key was not found in the cache. You have no guarantee that the server is \
         the computer you think it is.\n\n\
         The server's {} key details are:\n\n{}\n\n\
         If you trust this host, press Yes. To connect without adding host key to the cache, press \
         No. To abandon the connection press Cancel.",
        key.algorithm, key.fingerprint
    );
    wrapped(&text)
}

/// WinSCP's `MSG_DIFFERENT_KEY5` (the plugin's name for WinSCP's; no "Add": a key of the kind kept
/// beside the old one would still be "changed"), its buttons Update / Skip / Cancel.
pub fn changed_key_text(key: &HostKey) -> Vec<String> {
    let text = format!(
        "WARNING - POTENTIAL SECURITY BREACH!\n\n\
         The server's host key does not match the one NetWay has in cache. This means that either the \
         server administrator has changed the host key, the server presents different key under \
         certain circumstance, or you have actually connected to another computer pretending to be \
         the server.\n\n\
         The new {} key details are:\n\n{}\n\n\
         If you were expecting this change, trust the new key and want to continue connecting to the \
         server, press Update to update cache. If you want to carry on connecting but without updating \
         the cache, press Skip. If you want to abandon the connection completely, press Cancel. \
         Pressing Cancel is the ONLY guaranteed safe choice.",
        key.algorithm, key.fingerprint
    );
    wrapped(&text)
}

/// `text`'s paragraphs wrapped at the questions' width.
pub fn wrapped(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > WIDTH {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    lines
}

/// The answer of a host key's question's button.
pub fn key_answer(button: Option<usize>) -> Answer {
    match button {
        Some(0) => Answer::Keep,
        Some(1) => Answer::Once,
        _ => Answer::Refuse,
    }
}

/// What a login's failure says; none for the user's own stop.
pub fn failure_text(failure: &Failure) -> Option<String> {
    match failure {
        Failure::Cancelled => None,
        Failure::Refused => Some("Access denied.".to_string()),
        other => Some(other.to_string()),
    }
}

/// The work's question shown on the plugin's thread, titled `title`.
fn answer(host: &Host, title: &str, question: Question) -> Reply {
    match question {
        Question::Status(status) => {
            host.busy(title, &[&status]);
            Reply::Nothing
        }
        Question::HostKey(key) => Reply::HostKey(host_key(host, &key)),
        Question::Certificate(certificate) => {
            Reply::HostKey(certificate_answer(host, &certificate))
        }
        Question::Text { prompt, secret } => Reply::Text(typed(host, title, &prompt, secret)),
        Question::Questions(questions) => Reply::Answers(server_questions(host, title, &questions)),
        // A login asks none.
        Question::Transfer(_) => Reply::Nothing,
    }
}

/// WinSCP's certificate question: Yes keeps it, No takes it this time, Cancel refuses.
fn certificate_answer(host: &Host, certificate: &crate::ftp_login::Certificate) -> Answer {
    let text = crate::ftp_tls::prompt(&certificate.unknown);
    let lines: Vec<&str> = text.split('\n').collect();
    let message = (Message::new("Warning", &lines).warning().left_align())
        .buttons(&["&Yes", "&No", "&Cancel"]);
    key_answer(host.show(&message))
}

fn host_key(host: &Host, key: &HostKey) -> Answer {
    let (lines, buttons) = match key.known {
        Known::Changed => (changed_key_text(key), ["&Update", "&Skip", "&Cancel"]),
        _ => (unknown_key_text(key), ["&Yes", "&No", "&Cancel"]),
    };
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let message = (Message::new("Warning", &lines).warning().left_align()).buttons(&buttons);
    key_answer(host.show(&message))
}

fn typed(host: &Host, title: &str, prompt: &str, secret: bool) -> Option<String> {
    // WinSCP's prompts mark their hotkeys ("&Password:").
    let options = InputOptions {
        password: secret,
        enable_empty: true,
        no_ampersand: true,
        ..InputOptions::default()
    };
    host.input_with(title, prompt, "", options)
}

/// Each of the server's questions asked in turn (its name the title, its instructions before the first).
fn server_questions(host: &Host, title: &str, questions: &Questions) -> Option<Vec<String>> {
    let title = match questions.name.trim() {
        "" => title,
        name => name,
    };
    let mut answers = Vec::new();
    for (index, (prompt, echo)) in questions.prompts.iter().enumerate() {
        let instructions = questions.instructions.trim();
        let prompt = match (index, instructions) {
            (0, text) if !text.is_empty() => format!("{text} {}", prompt.trim()),
            _ => prompt.trim().to_string(),
        };
        answers.push(typed(host, title, &prompt, !echo)?);
    }
    Some(answers)
}

impl NetWay {
    /// The runtime made at the first connection.
    fn make_runtime(&mut self) -> std::io::Result<()> {
        if self.runtime.is_none() {
            self.runtime = Some(Runtime::new()?);
        }
        Ok(())
    }

    /// `address` connected to: the new panel's instance; none when it is not (the user told why, or
    /// stopped it).
    pub(crate) fn connect(&mut self, address: Address) -> Option<String> {
        let (session, name) = match &address.stored {
            Some(stored) => match self.store.load(stored) {
                Ok(loaded) => (loaded.session, stored.clone()),
                Err(error) => {
                    self.error(&format!("Cannot initialize session '{stored}'.\n{error}"));
                    return None;
                }
            },
            None => (address.session.clone(), address.session.default_name()),
        };
        if session.host.trim().is_empty() {
            self.error("No host name specified.");
            return None;
        }
        let options = session.options();
        let plan = Plan {
            host: session.host.trim().to_string(),
            port: session.port,
            user: session.user.clone(),
            password: session.password.clone(),
            key: (!session.private_key.trim().is_empty())
                .then(|| PathBuf::from(session.private_key.trim())),
            known_hosts: self.host.data_folder().join(KNOWN_HOSTS),
            // WinSCP's: the address's folder, else the session's remote directory.
            folder: match address.folder.trim() {
                "" => options.remote_directory.clone(),
                _ => address.folder.clone(),
            },
            protocol: session.protocol,
            encryption: session.encryption,
            scp_fallback: session.scp_fallback,
            options,
        };
        self.start(&plan, name, address.stored.clone())
    }

    /// The session `plan` logged into with NetBox's windows titled `title`; none when it is not (the user
    /// told why, or stopped it).
    pub(crate) fn log_in(&mut self, plan: &Plan, title: &str) -> Option<login::Started> {
        let host = self.host;
        host.busy(title, &[login::CONNECTING]);
        if let Err(error) = self.make_runtime() {
            self.error(&error.to_string());
            return None;
        }
        let runtime = self.runtime.as_ref()?;
        let (asker, mut questions) = Asker::new();
        let mut answering = |question: Question| answer(&host, title, question);
        let work = async { login::start(plan, &asker).await };
        let started =
            runtime.wait_answering(work, &|| host.cancelled(), &mut questions, &mut answering);
        match started {
            Err(_) => None,
            Ok(Err(failure)) => {
                if let Some(text) = failure_text(&failure) {
                    self.error(&text);
                }
                None
            }
            Ok(Ok(started)) => Some(started),
        }
    }

    /// The session `plan` started under its `name` (`stored`: the stored session's), a panel made of it.
    fn start(&mut self, plan: &Plan, name: String, stored: Option<String>) -> Option<String> {
        let started = self.log_in(plan, &name)?;
        self.opened += 1;
        let number = self.opened as u64;
        let instance = format!("{}{number}", crate::REMOTE_PREFIX);
        // What WinSCP's `Reopen` logs in with again: the user and the password that logged in.
        let kept = Plan {
            user: started.user.clone(),
            password: started.password.clone(),
            ..plan.clone()
        };
        let remote = Remote {
            url: crate::remote::session_url(
                &started.fs.protocol().to_ascii_lowercase(),
                &started.user,
                &plan.host,
                plan.port,
            ),
            connection: started.connection,
            fs: started.fs,
            host: plan.host.clone(),
            name,
            number,
            start: (started.folder != "/").then_some(started.folder),
            plan: kept,
            retries: 0,
            stored,
            folder: String::new(),
            history: Vec::new(),
            edits: Vec::new(),
            synchronized: false,
            seen: std::cell::RefCell::new(None),
            unfollowed: std::cell::RefCell::new(None),
            background: None,
        };
        self.remotes.insert(instance.clone(), remote);
        Some(instance)
    }
}

#[cfg(test)]
#[path = "connecting_tests.rs"]
mod tests;
