//! NetBox's progress window of a transfer (`TWinSCPFileSystem::ShowOperationProgress`, WinSCP's
//! `MinimizeName`, `FormatBytes`, `FormatDateTimeSpan`): its title the operation's, the file at hand and
//! its target's folder (none for the viewer's and the editor's copies), the file's bar, the time elapsed
//! and left, the bytes sent and the speed, the whole transfer's bar; shown first at once, then twice a
//! second. While the items are walked for their size: "Calculating directory size" with the folder.

use std::time::Duration;

use crate::copy_work::Snapshot;

/// NetBox's `ProgressWidth`.
pub const WIDTH: usize = 48;

/// WinSCP's `Ellipsis`.
const ELLIPSIS: &str = "...";

/// The labels (NetBox's `NB_PROGRESS_FILE_LABEL` and the others).
const FILE: &str = "File:   ";
const TARGET: &str = "Target: ";
const ELAPSED: &str = "Elapsed:";
const LEFT: &str = "Left:";
const START_TIME: &str = "Start time:";
const TRANSFERRED: &str = "Transferred:";
const CPS: &str = "CPS:";

/// NetBox's `ProgressBar`: the bar of `WIDTH - 5` columns and the percent after it.
pub fn bar(percent: u64) -> String {
    let percent = percent.min(100);
    let columns = WIDTH - 5;
    let filled = usize::try_from(percent).unwrap_or(100) * columns / 100;
    format!(
        "{}{}{percent:4}%",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(columns - filled)
    )
}

/// `done` of `total` in percents; nothing to do: none done.
pub fn percent(done: u64, total: u64) -> u64 {
    match total {
        0 => 0,
        _ => u64::try_from(u128::from(done) * 100 / u128::from(total)).unwrap_or(100),
    }
}

/// WinSCP's `FormatBytes`: bytes up to 100 KB, then KB up to 100 MB, then MiB.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    if bytes < 100 * KB {
        format!("{bytes} B")
    } else if bytes < 100 * KB * KB {
        format!("{} KB", (bytes + KB / 2) / KB)
    } else {
        format!("{} MiB", (bytes + KB * KB / 2) / (KB * KB))
    }
}

/// WinSCP's `FormatDateTimeSpan`: `h:mm:ss`, from four days on the days.
pub fn span(seconds: u64) -> String {
    let days = seconds / 86_400;
    if days >= 4 {
        return format!("{days} days");
    }
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// WinSCP's `CutFirstDirectory`: the first folder of `dir` as "...".
fn cut_first_directory(dir: &mut String, separator: char) {
    if dir.len() == 1 && dir.starts_with(separator) {
        dir.clear();
        return;
    }
    let root = dir.starts_with(separator);
    let mut rest: String = dir.chars().skip(usize::from(root)).collect();
    if rest.starts_with('.') {
        rest = rest.chars().skip(4).collect();
    }
    *dir = match rest.find(separator) {
        Some(at) => format!("{ELLIPSIS}{separator}{}", &rest[at + 1..]),
        None => String::new(),
    };
    if root {
        dir.insert(0, separator);
    }
}

/// WinSCP's `MinimizeName`: `name` within `max` characters, its first folders cut to "..." (a disk's
/// drive last), then cut at its end.
pub fn minimize(name: &str, max: usize, unix: bool) -> String {
    let separator = if unix { '/' } else { '\\' };
    let (mut dir, file) = match name.rfind(separator) {
        Some(at) => (name[..=at].to_string(), name[at + 1..].to_string()),
        None => (String::new(), name.to_string()),
    };
    let mut drive = String::new();
    if !unix && dir.chars().nth(1) == Some(':') {
        drive = dir.chars().take(2).collect();
        dir = dir.chars().skip(2).collect();
    }
    let length = |drive: &str, dir: &str| {
        (drive.chars().count()) + dir.chars().count() + file.chars().count()
    };
    let lone = format!("{separator}{ELLIPSIS}{separator}");
    while (!dir.is_empty() || !drive.is_empty()) && length(&drive, &dir) > max {
        if dir == lone {
            dir = format!("{ELLIPSIS}{separator}");
        } else if dir.is_empty() {
            drive.clear();
        } else {
            cut_first_directory(&mut dir, separator);
        }
    }
    let result = format!("{drive}{dir}{file}");
    result.chars().take(max).collect()
}

/// A label, its value at `width`'s end.
fn aligned(label: &str, value: &str, width: usize) -> String {
    let room = width.saturating_sub(label.chars().count() + value.chars().count());
    format!("{label}{}{value}", " ".repeat(room))
}

/// What the window shows of a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub moving: bool,
    /// Downloaded for the viewer or the editor: no target's line.
    pub temporary: bool,
    /// The source is the server's (its paths Unix ones).
    pub remote_source: bool,
    /// An operation on the server's files ("Deleting", "Setting properties", ...): the file and the
    /// whole's bar by items (NetBox's window of an operation that is no transfer).
    pub operation: Option<&'static str>,
}

/// The window's title and lines at `progress`, `elapsed` worked so far.
pub fn window(shown: Shown, progress: &Snapshot, elapsed: Duration) -> (String, Vec<String>) {
    // An upload from the editor's copy: the file's name alone.
    let source = match shown.temporary && !shown.remote_source {
        true => (progress.file.rsplit(['\\', '/']).next()).unwrap_or_default(),
        false => &progress.file,
    };
    let file = format!(
        "{FILE}{}",
        minimize(source, WIDTH - FILE.len(), shown.remote_source)
    );
    if progress.calculating {
        let lines = vec![file, bar(0)];
        return ("Calculating directory size".to_string(), lines);
    }
    if let Some(title) = shown.operation {
        let whole = bar(percent(progress.files_done, progress.files));
        return (title.to_string(), vec![file, whole]);
    }
    let title = if shown.moving { "Moving" } else { "Copying" };
    let mut lines = vec![file];
    if !(shown.temporary && shown.remote_source) {
        let folder = minimize(&progress.folder, WIDTH - TARGET.len(), !shown.remote_source);
        lines.push(format!("{TARGET}{folder}"));
    }
    let seconds = elapsed.as_secs();
    let cps = match elapsed.as_millis() {
        0 => 0,
        millis => u64::try_from(u128::from(progress.done) * 1000 / millis).unwrap_or(u64::MAX),
    };
    let left = match cps {
        0 => 0,
        cps => progress.total.saturating_sub(progress.done) / cps,
    };
    let half = WIDTH / 2 - 1;
    let elapsed_line = format!("{}  ", aligned(ELAPSED, &span(seconds), half));
    // NetBox's `TotalSizeSet`: without the total the start time, and the whole's bar by files.
    let (label, value, whole) = match progress.total_known {
        true => (LEFT, span(left), percent(progress.done, progress.total)),
        false => (
            START_TIME,
            progress.started.clone(),
            percent(progress.files_done, progress.files),
        ),
    };
    let elapsed_line = format!(
        "{elapsed_line}{}",
        aligned(label, &value, WIDTH - elapsed_line.chars().count())
    );
    let sent = format!(
        "{}  ",
        aligned(TRANSFERRED, &format_bytes(progress.done), half)
    );
    let speed = format!("{}/s", format_bytes(cps));
    let sent = format!(
        "{sent}{}",
        aligned(CPS, &speed, WIDTH - sent.chars().count())
    );
    lines.extend([
        bar(percent(progress.file_done, progress.file_size)),
        "\u{1}".to_string(),
        elapsed_line,
        sent,
        bar(whole),
    ]);
    (title.to_string(), lines)
}

#[cfg(test)]
#[path = "copy_progress_tests.rs"]
mod tests;
