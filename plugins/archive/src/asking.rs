//! The windows an extraction shows (arclite's ui.cpp): "Overwrite file", "Enter password", a failure with
//! Retry / Ignore / Ignore All / Cancel, its progress (`progress`) kept up while it works, Esc's question
//! "Do you want to interrupt current operation?" (when the program's confirmation asks it), and once it
//! is done with failures the "Operation log" question and the log in the viewer.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use nc_plugin_sdk::{ButtonSet, DialogItem, Host, ItemKind, Message};

use crate::extract::{Asks, Choice, Fault, FileInfo};
use crate::extract_options::Overwrite;
use crate::layout::{Layout, fit};
use crate::options::TITLE;
use crate::progress::Progress;
use crate::update::{UpdateAsks, UpdateAt};

/// The overwrite dialog's buttons, in their order, and the answers they give.
const OVERWRITE_BUTTONS: [(&str, Option<Overwrite>); 5] = [
    ("&Overwrite", Some(Overwrite::Overwrite)),
    ("&Skip", Some(Overwrite::Skip)),
    ("R&ename", Some(Overwrite::Rename)),
    ("A&ppend", Some(Overwrite::Append)),
    ("&Cancel", None),
];

/// Its buttons as an update asks (arclite's `odkUpdate`): no Rename, no Append.
const UPDATE_BUTTONS: [(&str, Option<Overwrite>); 3] = [
    ("&Overwrite", Some(Overwrite::Overwrite)),
    ("&Skip", Some(Overwrite::Skip)),
    ("&Cancel", None),
];

/// The buttons of the overwrite dialog of an extraction or (`update`) an update.
fn overwrite_buttons(update: bool) -> &'static [(&'static str, Option<Overwrite>)] {
    if update {
        &UPDATE_BUTTONS
    } else {
        &OVERWRITE_BUTTONS
    }
}

/// A time as arclite's `format_file_time` writes it: the user's short date and time, local.
pub fn time_text(time: Option<SystemTime>) -> String {
    let ticks = time.and_then(crate::item::to_ticks);
    let local = ticks.and_then(nc_plugin_sdk::system::local_time);
    local.map_or_else(String::new, |local| {
        format!("{} {}", local.date, local.time)
    })
}

/// arclite's `OverwriteDialog` of an extraction: the file, the question, the source's and the
/// destination's sizes and times (the newer marked), "Remember choice", the answers.
pub fn overwrite_dialog(
    path: &Path,
    source: &FileInfo,
    destination: &FileInfo,
) -> nc_plugin_sdk::Dialog {
    overwrite_dialog_of(path, (source, destination), false)
}

/// arclite's `OverwriteDialog` as an extraction or (`update`) an update shows it.
pub fn overwrite_dialog_of(
    path: &Path,
    (source, destination): (&FileInfo, &FileInfo),
    update: bool,
) -> nc_plugin_sdk::Dialog {
    let mut layout = Layout::new("Overwrite file", 60);
    layout.label(&fit(&path.display().to_string(), 60).replace('&', "&&"));
    layout.new_line();
    layout.label("Destination file already exists. Overwrite?");
    layout.new_line();
    layout.separator();
    layout.new_line();
    let (source_label, destination_label) = ("Source:", "Destination:");
    let label_pad = 1 + i32::try_from(source_label.len().max(destination_label.len())).unwrap_or(0);
    let (source_size, destination_size) = (source.size.to_string(), destination.size.to_string());
    let widest = source_size.len().max(destination_size.len());
    let size_pad = label_pad + 1 + i32::try_from(widest).unwrap_or(0);
    let newer = |one: Option<SystemTime>, other: Option<SystemTime>| {
        one.zip(other).is_some_and(|(a, b)| a > b)
    };
    let rows = [
        (
            source_label,
            source,
            &source_size,
            newer(source.modified, destination.modified),
        ),
        (
            destination_label,
            destination,
            &destination_size,
            newer(destination.modified, source.modified),
        ),
    ];
    for (label, info, size, is_newer) in rows {
        layout.label(label);
        layout.pad(label_pad);
        if !info.is_dir {
            layout.label(size);
            layout.pad(size_pad);
        }
        layout.label(&time_text(info.modified));
        if is_newer {
            layout.spacer(1);
            layout.label("(newer)");
        }
        layout.new_line();
    }
    layout.separator();
    layout.new_line();
    layout.boxed(DialogItem::check("&Remember choice", false));
    layout.new_line();
    layout.separator();
    layout.new_line();
    let buttons = (overwrite_buttons(update).iter().enumerate())
        .map(|(index, (label, _))| {
            let button = DialogItem::button(label);
            if index == 0 {
                button.default_button()
            } else {
                button
            }
        })
        .collect();
    layout.buttons(buttons);
    layout.new_line();
    layout.dialog().warning()
}

/// What the overwrite dialog closed with: the button's answer and "Remember choice"; none for Cancel
/// and Esc.
pub fn overwrite_answer(
    dialog: &nc_plugin_sdk::Dialog,
    closed: &nc_plugin_sdk::Closed,
) -> Option<(Overwrite, bool)> {
    overwrite_answer_of(dialog, closed, false)
}

/// What the overwrite dialog of an extraction or (`update`) an update closed with.
pub fn overwrite_answer_of(
    dialog: &nc_plugin_sdk::Dialog,
    closed: &nc_plugin_sdk::Closed,
    update: bool,
) -> Option<(Overwrite, bool)> {
    let buttons = overwrite_buttons(update);
    let first_button = dialog.items.len().checked_sub(buttons.len())?;
    let pressed = closed.button?.checked_sub(first_button)?;
    let answer = buttons.get(pressed)?.1?;
    let remember = (dialog.items.iter()).position(|item| item.kind == ItemKind::Check);
    let all = remember
        .and_then(|item| closed.values.get(item))
        .is_some_and(nc_plugin_sdk::Value::checked);
    Some((answer, all))
}

/// arclite's `PasswordDialog`: the archive's path, "Password:" and its line, OK and Cancel.
pub fn password_dialog(archive: &Path) -> nc_plugin_sdk::Dialog {
    let mut layout = Layout::new("Enter password", 60);
    layout.label(&fit(&archive.display().to_string(), 60).replace('&', "&&"));
    layout.new_line();
    layout.label("Password:");
    layout.edit(DialogItem::password(""), 0);
    layout.new_line();
    layout.separator();
    layout.new_line();
    layout.buttons(vec![
        DialogItem::button("OK").default_button(),
        DialogItem::button("Cancel"),
    ]);
    layout.new_line();
    layout.dialog()
}

/// arclite's `MSG_KPID_ERRORFLAGS`: the heading of a failure's lines after what it is about.
const ERRORS: &str = "Errors:";

/// arclite's `MSG_KPID_WARNINGFLAGS` heading the warnings.
const WARNINGS: &str = "Warnings:";

/// A failure's lines as arclite's `error_dlg` writes them, its own lines after `indent`: the system's
/// words, what it is about, "Errors:" when it is about something, its lines.
fn lines_of(fault: &Fault, indent: &str) -> Vec<String> {
    let mut lines: Vec<String> = fault.system.iter().chain(&fault.objects).cloned().collect();
    let headed =
        !fault.messages.is_empty() && (!fault.objects.is_empty() || !fault.warnings.is_empty());
    if headed {
        lines.push(ERRORS.to_string());
    }
    let errors_indent = if headed { indent } else { "" };
    lines.extend(
        fault
            .messages
            .iter()
            .map(|line| format!("{errors_indent}{line}")),
    );
    if !fault.warnings.is_empty() {
        lines.push(WARNINGS.to_string());
        lines.extend(fault.warnings.iter().map(|line| format!("{indent}{line}")));
    }
    lines
}

/// The lines of a failure as arclite's `retry_or_ignore_error` and `error_dlg` show them.
pub fn fault_lines(fault: &Fault) -> Vec<String> {
    lines_of(fault, "")
}

/// The log's text as arclite's `show_error_log` writes it: each failure's lines (indented under
/// "Errors:"), an empty line after.
pub fn log_text(log: &[Fault]) -> String {
    let mut text = String::new();
    for fault in log {
        for line in lines_of(fault, "  ") {
            text.push_str(&line);
            text.push('\n');
        }
        text.push('\n');
    }
    text
}

/// The extraction's windows through the program.
pub struct HostAsks {
    host: Host,
    archive: PathBuf,
    progress: Progress,
    /// Items deleted: arclite's `ArchiveFileDeleterProgress` for the window.
    deleting: bool,
    /// A new archive written: "Creating archive..." for the window.
    creating: bool,
}

impl HostAsks {
    pub fn new(host: Host, archive: &Path) -> Self {
        Self {
            host,
            archive: archive.to_path_buf(),
            progress: Progress::new(&archive.display().to_string()),
            deleting: false,
            creating: false,
        }
    }

    /// The windows of items deleted from the archive.
    pub fn deleting(mut self) -> Self {
        self.deleting = true;
        self
    }

    /// The windows of a new archive written (arclite's `ArchiveUpdateProgress` with `new_arc`).
    pub fn creating(mut self) -> Self {
        self.creating = true;
        self
    }

    /// "Operation log" asked once the extraction ended with failures: "Show log" opens them in the
    /// viewer (a file of its own, UTF-16 with its mark, gone as the viewer closes).
    pub fn show_log(&self, log: &[Fault]) {
        let message = Message::new(
            "Operation log",
            &["Operation completed with errors. Do you want to review log file?"],
        )
        .buttons(&["&Close", "Show &log"])
        .warning();
        if self.host.show(&message) != Some(1) {
            return;
        }
        let path = std::env::temp_dir().join(format!(
            "vc-archive-log-{}-{}.txt",
            std::process::id(),
            nc_plugin_sdk::system::now_ticks()
        ));
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(log_text(log).encode_utf16().flat_map(u16::to_le_bytes));
        if std::fs::write(&path, bytes).is_ok() && !self.host.view(&path, true) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

impl Asks for HostAsks {
    fn overwrite(
        &mut self,
        path: &Path,
        source: &FileInfo,
        destination: &FileInfo,
    ) -> Option<(Overwrite, bool)> {
        let dialog = overwrite_dialog(path, source, destination);
        let since = Instant::now();
        let closed = self.host.dialog(&dialog);
        self.progress.shown_again(since);
        overwrite_answer(&dialog, &closed)
    }

    fn password(&mut self) -> Option<String> {
        let since = Instant::now();
        let closed = self.host.dialog(&password_dialog(&self.archive));
        self.progress.shown_again(since);
        // The items: the path, "Password:", the line, the separator, OK, Cancel.
        (closed.button == Some(4)).then(|| closed.values.get(2).map(|value| value.text.clone()))?
    }

    fn fault(&mut self, fault: &Fault, can_retry: bool) -> Choice {
        let lines = fault_lines(fault);
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut buttons = Vec::new();
        if can_retry {
            buttons.push(("&Retry", Choice::Retry));
        }
        buttons.extend([
            ("&Ignore", Choice::Ignore),
            ("Ignore &All", Choice::IgnoreAll),
            ("Cancel", Choice::Cancel),
        ]);
        let labels: Vec<&str> = buttons.iter().map(|(label, _)| *label).collect();
        let message = Message::new(TITLE, &lines).buttons(&labels).warning();
        let since = Instant::now();
        let pressed = self.host.show(&message);
        self.progress.shown_again(since);
        pressed
            .and_then(|index| buttons.get(index))
            .map_or(Choice::Cancel, |(_, choice)| *choice)
    }

    fn progress(&mut self, file: &Path, done: u64, total: u64) {
        if let Some((title, lines)) = self
            .progress
            .update(&file.display().to_string(), done, total)
        {
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let _ = self.host.show(&Message::new(&title, &lines).left_align());
        }
    }

    /// Esc on the progress window: stopped, or with the program's "Interrupt" confirmation on, as the
    /// question answers (No: the work goes on, the next Esc asks again).
    fn stopped(&mut self) -> bool {
        interrupted(&self.host, &mut self.progress)
    }
}

/// Esc on a progress window: stopped, or with the program's "Interrupt" confirmation on, as the question
/// answers (No: the work goes on, the window back at its next update, the next Esc asks again).
pub fn interrupted(host: &Host, progress: &mut Progress) -> bool {
    if !host.cancelled() {
        return false;
    }
    if host.confirms("Esc") == Some(false) {
        return true;
    }
    let question = Message::new(TITLE, &["Do you want to interrupt current operation?"])
        .button_set(ButtonSet::YesNo);
    let since = Instant::now();
    let yes = host.show(&question) == Some(0);
    if !yes {
        host.resume();
        progress.shown_again(since);
    }
    yes
}

/// arclite's `error_dlg` of a failure its handler caught (`FAR_ERROR_HANDLER_END`): its lines under the
/// plugin's title, OK.
pub fn show_fault(host: &Host, fault: &Fault) {
    let lines = fault_lines(fault);
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let _ = host.show(&Message::new(TITLE, &lines).buttons(&["OK"]).warning());
}

/// A test's windows through the program: the extraction's password and Esc, the tester's progress.
impl crate::testing::TestAsks for HostAsks {
    fn password(&mut self) -> Option<String> {
        Asks::password(self)
    }

    fn testing(&mut self, file: &str, done: u64, total: u64) {
        if let Some(elapsed) = self.progress.due() {
            let (title, lines) = crate::testing::text(file, done, total, elapsed);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let _ = self.host.show(&Message::new(&title, &lines).left_align());
        }
    }

    fn stopped(&mut self) -> bool {
        Asks::stopped(self)
    }
}

/// A conversion into an SFX's windows through the program: arclite's `AttachSfxModuleProgress`, Esc.
impl crate::sfx_module::ConvertAsks for HostAsks {
    fn progress(&mut self, file: &str, done: u64, total: u64, _elapsed: std::time::Duration) {
        if self.progress.due().is_some() {
            let (title, lines) = crate::sfx_module::text(file, done, total);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let _ = self.host.show(&Message::new(&title, &lines).left_align());
        }
    }

    fn stopped(&mut self) -> bool {
        Asks::stopped(self)
    }
}

/// An update's windows through the program: the extraction's, with the update's overwrite question
/// and progress window.
impl UpdateAsks for HostAsks {
    fn overwrite(
        &mut self,
        path: &Path,
        source: &FileInfo,
        destination: &FileInfo,
    ) -> Option<(Overwrite, bool)> {
        let dialog = overwrite_dialog_of(path, (source, destination), true);
        let since = Instant::now();
        let closed = self.host.dialog(&dialog);
        self.progress.shown_again(since);
        overwrite_answer_of(&dialog, &closed, true)
    }

    fn fault(&mut self, fault: &Fault, can_retry: bool) -> Choice {
        Asks::fault(self, fault, can_retry)
    }

    fn progress(&mut self, at: &UpdateAt<'_>) {
        if let Some(elapsed) = self.progress.due() {
            let archive = self.progress.archive().to_string();
            let (title, lines) = if self.deleting {
                crate::update_progress::delete_text(at, elapsed)
            } else {
                crate::update_progress::text((self.creating, &archive), at, elapsed)
            };
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let _ = self.host.show(&Message::new(&title, &lines).left_align());
        }
    }

    fn stopped(&mut self) -> bool {
        Asks::stopped(self)
    }
}

#[cfg(test)]
#[path = "asking_tests.rs"]
mod tests;
