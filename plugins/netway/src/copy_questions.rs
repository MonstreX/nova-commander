//! A transfer's questions in WinSCP's words (`TTerminal::ConfirmFileOverwrite`,
//! `TSFTPFileSystem::SFTPConfirmOverwrite`, `TTerminal::FileOperationLoopQuery`; NetBox's
//! `MoreMessageDialog` shows their buttons): a file there already - overwritten, skipped, appended to or
//! resumed, sent under a new name, the answer for all of them; a failure - aborted, retried, skipped. The
//! work asks them through [`crate::ask::Asker`]; the plugin's thread shows them.

use nc_plugin_sdk::{Host, InputOptions, Message};

/// A transfer's question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferQuestion {
    /// The target is there: its text, and whether it can be appended to.
    Overwrite { lines: Vec<String>, append: bool },
    /// Appended or resumed (WinSCP's `APPEND_OR_RESUME2`), the source's path.
    AppendOrResume(String),
    /// The name to send it under instead (WinSCP's `RENAME_PROMPT2`), the target's name first.
    NewName(String),
    /// A failure's text.
    Failed(Vec<String>),
    /// A partly sent file under its temporary name, by the target's name (WinSCP's
    /// `SFTPConfirmResume`): resumed or sent anew; bigger than the source: said and deleted.
    Resume { name: String, bigger: bool },
}

/// The answer to "Do you want to resume file transfer?" (`RESUME_TRANSFER2`); for the bigger partial file
/// (`PARTIAL_BIGGER_THAN_SOURCE`, OK / Abort) `No` is OK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    Yes,
    No,
    Cancel,
}

/// WinSCP's `RESUME_TRANSFER2` and `PARTIAL_BIGGER_THAN_SOURCE`.
pub fn resume_text(name: &str, bigger: bool) -> String {
    match bigger {
        true => format!(
            "Target directory contains partially transferred file '{name}', which is bigger than a source file. The file will be deleted."
        ),
        false => format!(
            "Do you want to resume file transfer?\n \nTarget directory contains partially transferred file '{name}'.\n \nNote: Answering 'No' would delete partially transferred file and restart transfer."
        ),
    }
}

/// The answer to an overwrite question (WinSCP's `qa*` with NetBox's aliases).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    Yes,
    No,
    Cancel,
    Append,
    NewName,
    NewerOnly,
    NoToAll,
    YesToAll,
}

/// The answer to "append or resume".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOrResume {
    Append,
    Resume,
    ResumeAll,
    Cancel,
}

/// The answer to a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failed {
    Abort,
    Retry,
    Skip,
    SkipAll,
}

/// A transfer's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferReply {
    Overwrite(Overwrite),
    AppendOrResume(AppendOrResume),
    /// The name typed; none: Esc.
    NewName(Option<String>),
    Failed(Failed),
    Resume(Resume),
}

/// The overwrite question's buttons in NetBox's order (`ADD_BUTTON`'s), Append only when it can.
pub fn overwrite_buttons(append: bool) -> Vec<(&'static str, Overwrite)> {
    let all = [
        ("&Yes", Overwrite::Yes),
        ("&No", Overwrite::No),
        ("&Cancel", Overwrite::Cancel),
        ("A&ppend", Overwrite::Append),
        ("New na&me", Overwrite::NewName),
        ("Ne&wer only", Overwrite::NewerOnly),
        ("N&o to All", Overwrite::NoToAll),
        ("Yes to A&ll", Overwrite::YesToAll),
    ];
    (all.into_iter())
        .filter(|(_, answer)| append || *answer != Overwrite::Append)
        .collect()
}

/// One side of the overwrite question's details: its size and last write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    pub size: u64,
    /// Seconds since 1970 (UTC), when known.
    pub modified: Option<u64>,
}

/// WinSCP's `FormatSize`: the digits grouped by commas.
pub fn format_size(size: u64) -> String {
    let digits = size.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// WinSCP's `UserModificationStr`: the local date and time, "Unknown" when not known.
pub fn modification(seconds: Option<u64>) -> String {
    const TICKS_TO_1970: i64 = 116_444_736_000_000_000;
    let local = (seconds.and_then(|seconds| i64::try_from(seconds).ok()))
        .and_then(|seconds| seconds.checked_mul(10_000_000))
        .and_then(|ticks| ticks.checked_add(TICKS_TO_1970))
        .and_then(nc_plugin_sdk::system::local_time);
    match local {
        Some(local) => format!("{} {}", local.date, local.time),
        None => "Unknown".to_string(),
    }
}

/// WinSCP's `LOCAL_FILE_OVERWRITE2` / `REMOTE_FILE_OVERWRITE2` with `FILE_OVERWRITE_DETAILS`: the target
/// `name` (`local`: on the disk), the new file's and the existing one's sizes and times.
pub fn overwrite_text(name: &str, local: bool, new: Side, existing: Side) -> String {
    let side = if local { "local" } else { "remote" };
    let detail = |label: &str, one: Side| {
        format!(
            "{label}{} bytes, {}",
            format_size(one.size),
            modification(one.modified)
        )
    };
    format!(
        "Overwrite {side} file '{name}'?\n \nDestination directory already contains file '{name}'.\nChoose, if you want to overwrite the file or skip this transfer and keep existing file.\n \n{}\n{}",
        detail("New:      ", new),
        detail("Existing: ", existing)
    )
}

/// WinSCP's headline of the failed step (`TRANSFER_ERROR`, `DELETE_FILE_ERROR`, ...) with the failure's
/// own text.
pub fn failed_text(headline: &str, error: &str) -> String {
    format!("{headline}\n{error}")
}

/// `text` as the message's lines, each wrapped.
pub fn lines(text: &str) -> Vec<String> {
    crate::connecting::wrapped(text)
}

/// The question shown on the plugin's thread.
pub fn show(host: &Host, question: &TransferQuestion) -> TransferReply {
    match question {
        TransferQuestion::Overwrite { lines, append } => {
            let buttons = overwrite_buttons(*append);
            let labels: Vec<&str> = buttons.iter().map(|(label, _)| *label).collect();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let message = Message::new("Confirmation", &lines).buttons(&labels);
            let pressed = host.show(&message).and_then(|index| buttons.get(index));
            TransferReply::Overwrite(pressed.map_or(Overwrite::Cancel, |(_, answer)| *answer))
        }
        TransferQuestion::AppendOrResume(file) => {
            TransferReply::AppendOrResume(append_or_resume(host, file))
        }
        TransferQuestion::NewName(name) => {
            let options = InputOptions {
                no_ampersand: true,
                ..InputOptions::default()
            };
            let typed =
                host.input_with("Transfer under different name", "&New name:", name, options);
            TransferReply::NewName(typed)
        }
        TransferQuestion::Failed(lines) => {
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let buttons = ["A&bort", "&Retry", "&Skip", "Ski&p all"];
            let message = Message::new("Error", &lines).warning().buttons(&buttons);
            let answer = match host.show(&message) {
                Some(1) => Failed::Retry,
                Some(2) => Failed::Skip,
                Some(3) => Failed::SkipAll,
                _ => Failed::Abort,
            };
            TransferReply::Failed(answer)
        }
        TransferQuestion::Resume { name, bigger } => {
            TransferReply::Resume(resume_question(host, name, *bigger))
        }
    }
}

/// WinSCP's `SFTPConfirmResume`: Yes / No / Cancel, or the bigger file's OK / Abort.
fn resume_question(host: &Host, name: &str, bigger: bool) -> Resume {
    let lines = lines(&resume_text(name, bigger));
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    if bigger {
        let message = Message::new("Warning", &lines)
            .warning()
            .buttons(&["&Ok", "&Abort"]);
        return match host.show(&message) {
            Some(0) => Resume::No,
            _ => Resume::Cancel,
        };
    }
    let message = Message::new("Confirmation", &lines).buttons(&["&Yes", "&No", "&Cancel"]);
    match host.show(&message) {
        Some(0) => Resume::Yes,
        Some(1) => Resume::No,
        _ => Resume::Cancel,
    }
}

/// WinSCP's `APPEND_OR_RESUME2` with Yes / No / No to All / Cancel.
fn append_or_resume(host: &Host, file: &str) -> AppendOrResume {
    let text = format!(
        "Do you want to append file '{file}' at the end of existing file?\n \nPress 'No' to resume file transfer instead."
    );
    let lines = lines(&text);
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let buttons = ["&Yes", "&No", "N&o to All", "&Cancel"];
    match host.show(&Message::new("Confirmation", &lines).buttons(&buttons)) {
        Some(0) => AppendOrResume::Append,
        Some(1) => AppendOrResume::Resume,
        Some(2) => AppendOrResume::ResumeAll,
        _ => AppendOrResume::Cancel,
    }
}

#[cfg(test)]
#[path = "copy_questions_tests.rs"]
mod tests;
