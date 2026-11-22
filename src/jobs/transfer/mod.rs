//! Copy and move (ADR-0009, critical area: data loss). The engine runs on a worker thread (`Job`) and
//! talks to the UI through a [`Host`]: progress, questions, cancel. Its safety rules:
//! - an existing file is replaced only by the user's choice, through a temporary sibling put in its
//!   place in one step; in place only after the low-space warning was answered;
//! - a copy that did not finish never stays under the real name; a cancelled overwrite keeps the old
//!   file;
//! - a move deletes a source file only after its copy is complete (and, with Verify, read back and found
//!   the same): a file is always in one place;
//! - a folder is never copied into itself, a file never onto itself;
//! - links are copied as links, unless "Copy contents of symbolic links" is on; then a move takes the
//!   link away and leaves what it leads to;
//! - making links (Alt+F6) never replaces anything but a file the user chose to overwrite with a hard
//!   link, and that through a temporary sibling.
//!
//! Either end may be behind a provider (ADR-0013 §6): both on a disk, the OS copies and renames as
//! before; otherwise the file streams in chunks, and the same rules hold.

mod elevate;
mod engine;
mod file;
mod job;
mod link_contents;
mod make_links;
mod metadata;
mod retry;
mod sides;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_elevation;
#[cfg(test)]
mod tests_filter;
#[cfg(test)]
mod tests_hard;
#[cfg(test)]
mod tests_keep;
#[cfg(test)]
mod tests_link_contents;
#[cfg(test)]
mod tests_make_links;
#[cfg(test)]
mod tests_plugin;
#[cfg(test)]
mod tests_plugin_keep;
#[cfg(test)]
mod tests_rename;
#[cfg(test)]
mod tests_retry;
#[cfg(test)]
mod tests_verify;
#[cfg(test)]
mod tests_vfs;
#[cfg(test)]
mod tests_vfs_safety;
mod verify;

use std::time::SystemTime;

pub use engine::run;
pub use job::Job;

use crate::platform::files::NewLink;
use crate::vfs::filter::AreaFilter;
use crate::vfs::{Entry, VfsPath};

/// What to do with a file that exists (FAR's "Already existing files").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Existing {
    Ask,
    Overwrite,
    Skip,
    /// Copy under a free name: `name (2).ext`.
    Rename,
    Append,
    /// Overwrite when the existing file is older, skip otherwise.
    OnlyNewer,
}

/// FAR's "Access rights".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rights {
    /// What the target folder gives a new file; a renamed one keeps its own.
    #[default]
    Default,
    /// The source's.
    Copy,
    /// Only what the target folder passes on, also for what goes by a rename.
    Inherit,
}

/// What an operation keeps besides the contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Keep {
    /// "Preserve all timestamps": creation, last access and change too.
    pub timestamps: bool,
    pub rights: Rights,
    /// "Copy contents of symbolic links".
    pub link_contents: bool,
    /// Each copy is read back and compared with its source.
    pub verify: bool,
}

/// One copy or move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub sources: Vec<VfsPath>,
    /// An existing folder (the sources go into it) or a new name (one source).
    pub target: VfsPath,
    pub move_: bool,
    pub existing: Existing,
    /// Ask before replacing a read-only file.
    pub ask_read_only: bool,
    pub keep: Keep,
    /// "Retry skipped": just these, each to its place, instead of walking `sources`.
    pub retrying: Vec<Skipped>,
    /// Alt+F6: links of this kind instead of copies (`move_` is false).
    pub link: Option<NewLink>,
    /// FAR's "Use filter": Copy's filter, and the time its dates are counted from.
    pub filter: Option<(AreaFilter, SystemTime)>,
}

impl Request {
    /// FAR's `COPY_FILTERED`: the filter leaves `entry` out (a folder with all in it). A junction or a
    /// symbolic link made for the item itself is never filtered (FAR's `MkSymLink` before the copy).
    pub(super) fn filters_out(&self, entry: &Entry) -> bool {
        self.walks() && filtered_out(self.filter.as_ref(), entry)
    }

    /// FAR's `strRenamedName`: the one source's new name in its own folder (not `into` a folder there),
    /// when it got there whole (an item skipped is not there).
    pub(super) fn renamed(&self, into: bool, report: &Report) -> Option<VfsPath> {
        let [source] = self.sources.as_slice() else {
            return None;
        };
        let whole = report.failures.is_empty() && report.skipped.is_empty() && !report.cancelled;
        let beside = source.parent() == self.target.parent();
        let renamed = source.file_name() != self.target.file_name();
        let named = whole && !into && beside && renamed && self.link.is_none();
        (named && sides::stat(&self.target).is_ok()).then(|| self.target.clone())
    }
}

/// Whether `filter` leaves `entry` out.
pub(super) fn filtered_out(filter: Option<&(AreaFilter, SystemTime)>, entry: &Entry) -> bool {
    filter.is_some_and(|(filter, now)| !filter.takes(entry, *now))
}

impl Request {
    /// Whether folders are walked: not for a junction or a symbolic link, made for the item itself.
    pub fn walks(&self) -> bool {
        self.link.is_none_or(|kind| kind == NewLink::Hard)
    }
}

/// A file's name, size and last write time, for the question about an existing file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub path: VfsPath,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// What the engine asks the user; the worker waits for the [`Reply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// The target file exists (FAR's "File already exists"): Overwrite, Skip, Rename, Append (when
    /// `append`: not for a link), Cancel; "Remember choice" is [`Reply::for_all`].
    Exists {
        source: FileFacts,
        target: FileFacts,
        append: bool,
        /// The first free name beside it, `name (2).ext`: what Rename offers (FAR's `GenerateName`).
        free: VfsPath,
    },
    /// The file to replace is read-only: Overwrite (`for_all`: all), Skip (`for_all`: skip all), Cancel.
    ReadOnly { target: VfsPath },
    /// Not enough room for the new file next to the old one, enough once the old one is gone:
    /// InPlace (`for_all`: all in place), Skip, Cancel.
    LowSpace {
        target: VfsPath,
        new_size: u64,
        old_size: u64,
        free: u64,
    },
    /// FAR's "Access denied" (ADR-0007): Elevate, Skip (`for_all`: the rest of the operation), Cancel.
    Elevate {
        path: VfsPath,
        verb: crate::jobs::elevation::Verb,
    },
    /// A step failed: Retry (when `retry`), Skip (`for_all`: skip all), Cancel.
    Error {
        path: VfsPath,
        message: String,
        /// Who holds the file (FAR's `OperationFailed`).
        held: Vec<String>,
        retry: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Overwrite,
    Skip,
    Rename,
    Append,
    InPlace,
    Retry,
    Elevate,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub answer: Answer,
    /// The same answer for the rest of the operation.
    pub for_all: bool,
    /// Rename's new name (FAR's "Rename" dialog); without it a free one, `name (2).ext`.
    pub name: Option<VfsPath>,
}

impl Reply {
    pub fn once(answer: Answer) -> Self {
        Self {
            answer,
            for_all: false,
            name: None,
        }
    }

    pub fn always(answer: Answer) -> Self {
        Self {
            answer,
            for_all: true,
            name: None,
        }
    }

    /// Rename to `name`; `for_all`: the files after it get free names.
    pub fn renamed(name: VfsPath, for_all: bool) -> Self {
        Self {
            answer: Answer::Rename,
            for_all,
            name: Some(name),
        }
    }
}

/// How far the operation is, for FAR's progress dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub source: VfsPath,
    pub target: VfsPath,
    pub file_size: u64,
    pub file_done: u64,
    pub files_done: u64,
    pub bytes_done: u64,
    /// Counted by a walker running ahead of the copy; final once `totals_known`.
    pub files_total: u64,
    pub bytes_total: u64,
    pub totals_known: bool,
    /// While the file at hand is read back: the bytes compared so far.
    pub verified: Option<u64>,
    /// Selected objects finished since the UI last looked; the UI takes their marks off.
    pub finished: Vec<VfsPath>,
}

impl Progress {
    /// What is done of what there is to do: bytes, or objects when there are none (links, empty
    /// files).
    pub fn amounts(&self) -> (u64, u64) {
        if self.bytes_total == 0 {
            (self.files_done, self.files_total)
        } else {
            (self.bytes_done, self.bytes_total)
        }
    }
}

pub use super::work::{Report, Skipped};

/// The engine's link to whoever runs it: the worker thread's channels, or a script in tests.
pub trait Host {
    fn ask(&mut self, question: Question) -> Reply;
    /// Whether the user stopped the operation; may wait while it is paused.
    fn cancelled(&mut self) -> bool;
    fn progress(&mut self, change: &mut dyn FnMut(&mut Progress));
    /// The job's cancel flag, handed to what it lists (review R15); none of its own: never set.
    fn stop_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        crate::jobs::work::never_stopped()
    }
    /// The bytes the user may write in `dir`; `None` when the file system does not say.
    fn free_space(&mut self, dir: &std::path::Path) -> Option<u64> {
        crate::platform::volumes::free_space(dir)
    }
    /// A rename within one volume (`CrossesDevices` otherwise); `replace` replaces an existing file in
    /// one step, without it an existing target is never touched.
    fn move_item(
        &mut self,
        from: &std::path::Path,
        to: &std::path::Path,
        replace: bool,
    ) -> std::io::Result<()> {
        crate::platform::files::move_file(from, to, replace)
    }
    /// Whether "Access denied" can be offered at all.
    fn elevate_available(&self) -> bool {
        crate::platform::elevation::can_elevate()
    }
    /// Has the helper carry out `request`; a copy's progress counts on from `base`.
    fn run_elevated(
        &mut self,
        request: &crate::vfs::local::elevated::Request,
        base: Option<u64>,
    ) -> std::io::Result<()> {
        crate::vfs::local::elevated_session::call(request, &mut |done, total| {
            if let Some(base) = base.filter(|_| total > 0) {
                self.progress(&mut |progress| {
                    progress.file_done = done;
                    progress.file_size = total;
                    progress.bytes_done = base + done;
                });
            }
            !self.cancelled()
        })
    }
}
