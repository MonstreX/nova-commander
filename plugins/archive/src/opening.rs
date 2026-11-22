//! When an archive opens (arclite's `analyse_open`, `parse_command`, `parse_open_command`): Enter on a
//! file of the include masks and not of the exclude ones, Ctrl+PgDn on any file unless "Mask settings
//! apply to Ctrl-PgDn"; `arc:[-d] [-t:<type>] [-p:<password>] <archive>` from the command line.

use std::path::{Path, PathBuf};

use crate::options::Options;

/// Whether Enter (`alternative` false) or Ctrl+PgDn on a file named `name` is the plugin's to try;
/// `matches` the program's masks (none for masks with errors: no match).
pub fn takes(
    options: &Options,
    name: &str,
    alternative: bool,
    matches: impl Fn(&str, &str) -> Option<bool>,
) -> bool {
    if !options.handle_commands {
        return false;
    }
    if alternative && !options.pgdn_masks {
        return true;
    }
    if options.use_include_masks && matches(&options.include_masks, name) != Some(true) {
        return false;
    }
    !(options.use_exclude_masks && matches(&options.exclude_masks, name) == Some(true))
}

/// `arc:`'s open command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCommand {
    /// `-d`: the format detected from the contents (one format here: the same).
    pub detect: bool,
    pub password: Option<String>,
    pub path: String,
}

/// arclite's help topic "Prefix" in short: what the command line takes.
pub const USAGE: &str = "arc:[-d] [-t:<type>] [-p:<password>] <archive>";

/// `arc:`'s open command read as arclite reads it (`parse_open_command`): its tokens (quotes kept inside
/// a token), parameters, then the archive's path last (its quotes off, `%NAME%` the environment's); a
/// first token of `c`, `u`, `x`, `e`, `d` or `t` is another command (`command_line`).
pub fn parse(text: &str) -> Result<OpenCommand, String> {
    let tokens = tokens(text);
    let first = tokens.first().map(|token| token.to_lowercase());
    if matches!(first.as_deref(), Some("c" | "u" | "x" | "e" | "d" | "t")) {
        return Err("Not an open command".to_string());
    }
    let bad = || format!("The command is not understood: {USAGE}");
    let (path, params) = tokens.split_last().ok_or_else(bad)?;
    if is_param(path) {
        return Err(bad());
    }
    let mut command = OpenCommand {
        detect: false,
        password: None,
        path: expand_env(&unquote(path)),
    };
    for param in params {
        if !is_param(param) {
            return Err(bad());
        }
        let (name, value) = match param[1..].split_once(':') {
            Some((name, value)) => (name.to_lowercase(), unquote(value)),
            None => (param[1..].to_lowercase(), String::new()),
        };
        match name.as_str() {
            "d" => command.detect = true,
            "t" if value.eq_ignore_ascii_case("zip") => {}
            "p" if !value.is_empty() => command.password = Some(value),
            _ => return Err(bad()),
        }
    }
    Ok(command)
}

/// arclite's tokens: split at spaces outside quotes, the quotes kept.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                token.push(character);
            }
            ' ' if !quoted => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            _ => token.push(character),
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

pub(crate) fn is_param(token: &str) -> bool {
    token.starts_with(['-', '/'])
}

/// arclite's `unquote`: the quotes around the whole text off, others kept.
pub(crate) fn unquote(text: &str) -> String {
    match text
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
    {
        Some(inner) => inner.to_string(),
        None => text.to_string(),
    }
}

/// `%NAME%` as the environment has it (Windows' `ExpandEnvironmentStrings`: a name it has not stays).
pub fn expand_env(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            break;
        };
        out.push_str(&rest[..start]);
        match std::env::var(&after[..end]) {
            Ok(value) if end > 0 => out.push_str(&value),
            _ => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A path given made whole as FAR's `get_absolute_path` makes it: in `folder` (FAR's current folder)
/// when relative, `.` and `..` folded and `/` turned to `\` (`GetFullPathName`).
pub fn full_path(path: &str, folder: Option<&Path>) -> PathBuf {
    let given = PathBuf::from(path);
    let joined = match folder {
        Some(folder) if given.is_relative() => folder.join(given),
        _ => given,
    };
    std::path::absolute(&joined).unwrap_or(joined)
}

#[cfg(test)]
#[path = "opening_tests.rs"]
mod tests;
