//! What the plugin answers while 7-Zip's library opens an archive (arclite's `ArchiveOpener`): its
//! "Reading archive..." window half a second in (the volume, the files and the bytes read of their totals,
//! a bar for each), Esc's question, the password of encrypted headers (arclite's `password_dialog`,
//! asked once; one given with `arc:-p` first).

use std::path::{Path, PathBuf};
use std::time::Instant;

use nc_plugin_sdk::sevenzip::Opening;
use nc_plugin_sdk::{Host, Message};

use crate::asking::{interrupted, password_dialog};
use crate::layout::fit;
use crate::progress::{Progress, SIZES, WIDTH, bar, data_size, percent};

/// The totals and what is done (arclite's `total_files`, `completed_bytes`, ...).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub total_files: u64,
    pub total_bytes: u64,
    pub files: u64,
    pub bytes: u64,
}

/// arclite's `ArchiveOpener::do_update_ui`: the title and lines of the window.
pub fn window(volume: &str, counts: Counts) -> (String, Vec<String>) {
    let files = percent(counts.files, counts.total_files);
    let bytes = percent(counts.bytes, counts.total_bytes);
    let done = if counts.total_files != 0 {
        files
    } else {
        bytes
    };
    let lines = vec![
        fit(volume, WIDTH),
        format!("{} / {}", counts.files, counts.total_files),
        bar(files),
        "\u{1}".to_string(),
        format!(
            "{} / {}",
            data_size(counts.bytes, &SIZES),
            data_size(counts.total_bytes, &SIZES)
        ),
        bar(bytes),
    ];
    (format!("{{{done}%}} Reading archive..."), lines)
}

/// The host's answers for one archive's opening.
pub struct OpenAsks {
    host: Host,
    archive: PathBuf,
    /// The password given or asked; asked once.
    password: Option<String>,
    /// Whether the user typed it in the password window (arclite's `password_len > 0`).
    typed: bool,
    counts: Counts,
    progress: Progress,
    /// Alt+F7's search (`OPM_FIND`): no window, no password asked, a stop taken without a question.
    pub silent: bool,
}

impl OpenAsks {
    pub fn new(host: Host, archive: &Path, password: Option<String>) -> Self {
        Self {
            host,
            archive: archive.to_path_buf(),
            password,
            typed: false,
            counts: Counts::default(),
            progress: Progress::new(&archive.display().to_string()),
            silent: false,
        }
    }

    /// The password the headers took, when they asked one.
    pub fn password(&self) -> Option<String> {
        self.password.clone()
    }

    /// Whether a password was typed in its window (none given before).
    pub fn typed(&self) -> bool {
        self.typed
    }

    /// The window when due; Esc's question: false stops.
    fn update(&mut self) -> bool {
        if self.silent {
            return !self.host.cancelled();
        }
        if self.progress.due().is_some() {
            let name = (self.archive.file_name())
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let (title, lines) = window(&name, self.counts);
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            let _ = self.host.show(&Message::new(&title, &lines).left_align());
        }
        !interrupted(&self.host, &mut self.progress)
    }
}

impl Opening for OpenAsks {
    fn set_total(&mut self, files: Option<u64>, bytes: Option<u64>) -> bool {
        if let Some(files) = files {
            self.counts.total_files = files;
        }
        if let Some(bytes) = bytes {
            self.counts.total_bytes = bytes;
        }
        self.update()
    }

    fn set_completed(&mut self, files: Option<u64>, bytes: Option<u64>) -> bool {
        if let Some(files) = files {
            self.counts.files = files;
        }
        if let Some(bytes) = bytes {
            self.counts.bytes = bytes;
        }
        self.update()
    }

    fn password(&mut self) -> Option<String> {
        if self.password.is_none() && !self.silent {
            let since = Instant::now();
            let closed = self.host.dialog(&password_dialog(&self.archive));
            self.progress.shown_again(since);
            // The items: the path, "Password:", the line, the separator, OK, Cancel.
            let given = (closed.button == Some(4))
                .then(|| closed.values.get(2).map(|value| value.text.clone()))
                .flatten();
            let given = given?;
            // arclite's `password_len`: an empty password typed ends its loop as none would.
            self.typed = !given.is_empty();
            self.password = Some(given);
        }
        self.password.clone()
    }
}

#[cfg(test)]
#[path = "open_asking_tests.rs"]
mod tests;
