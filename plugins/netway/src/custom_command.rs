//! WinSCP's custom commands (`TRemoteCustomCommand`, `TFileCustomCommand`): the patterns of "Apply
//! command" - `!!` an exclamation mark, `!` the file's name, `!&` the selected files quoted and
//! space-delimited, `!/` the current remote path, `!@` the host, `!U` the user, `!P` the password,
//! `!?prompt?default!` a value asked for (`\?` a question mark in the prompt) - and the command's runs: once
//! with `!&`, else once per file.

use crate::shell::quoted;

/// What the patterns stand for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Data {
    pub folder: String,
    pub host: String,
    pub user: String,
    pub password: String,
}

/// A `!?prompt?default!` of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub prompt: String,
    pub default: String,
}

/// The prompts of `command`, in their order.
pub fn prompts(command: &str) -> Vec<Prompt> {
    let mut found = Vec::new();
    let mut rest = command;
    while let Some(at) = rest.find('!') {
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('?')
            && let Some((prompt, default, left)) = prompt_of(after)
        {
            found.push(Prompt { prompt, default });
            rest = left;
        } else if rest.starts_with('!') {
            rest = &rest[1..];
        }
    }
    found
}

/// `prompt?default!` read; the rest after it.
fn prompt_of(text: &str) -> Option<(String, String, &str)> {
    let mut prompt = String::new();
    let mut chars = text.char_indices();
    let default_at = loop {
        let (at, ch) = chars.next()?;
        match ch {
            '\\' if text[at + 1..].starts_with('?') => {
                prompt.push('?');
                chars.next();
            }
            '?' => break at + 1,
            other => prompt.push(other),
        }
    };
    let end = text[default_at..].find('!')? + default_at;
    Some((prompt, text[default_at..end].to_string(), &text[end + 1..]))
}

/// Whether the command takes all files at once (`!&`).
pub fn takes_list(command: &str) -> bool {
    let mut rest = command;
    while let Some(at) = rest.find('!') {
        rest = &rest[at + 1..];
        match rest.chars().next() {
            Some('&') => return true,
            Some('!') => rest = &rest[1..],
            _ => {}
        }
    }
    false
}

/// The command with its patterns put in: `file` for `!`, `files` for `!&`, `answers` for the prompts in
/// their order.
pub fn expanded(
    command: &str,
    data: &Data,
    (file, files): (&str, &[String]),
    answers: &[String],
) -> String {
    let mut text = String::new();
    let mut rest = command;
    let mut answers = answers.iter();
    while let Some(at) = rest.find('!') {
        text.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('?')
            && let Some((_, default, left)) = prompt_of(after)
        {
            text.push_str(answers.next().map_or(default.as_str(), String::as_str));
            rest = left;
            continue;
        }
        let (put, taken) = match rest.chars().next() {
            Some('!') => ("!".to_string(), 1),
            Some('&') => {
                let list: Vec<String> = files.iter().map(|name| quoted(name)).collect();
                (list.join(" "), 1)
            }
            Some('/') => (format!("{}/", data.folder.trim_end_matches('/')), 1),
            Some('@') => (data.host.clone(), 1),
            Some('U') => (data.user.clone(), 1),
            Some('P') => (data.password.clone(), 1),
            _ => (file.to_string(), 0),
        };
        text.push_str(&put);
        rest = &rest[taken..];
    }
    text.push_str(rest);
    text
}

#[cfg(test)]
#[path = "custom_command_tests.rs"]
mod tests;
