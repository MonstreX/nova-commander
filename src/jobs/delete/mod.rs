//! Delete, as FAR's `far/delete.cpp` (critical area: data loss), run as a background task (ADR-0010).
//! The engine runs on a worker thread and talks to the UI through a [`Host`]: progress, questions,
//! cancel. Its rules:
//! - nothing is deleted before the user's question in the UI (FAR's "Do you wish to delete");
//! - a folder that is not empty is asked about (unless the user said All or turned the question off),
//!   at the top and, deleting past the Recycle Bin, at every level;
//! - a read-only file is asked about; a link is deleted as a link, what it points to is never touched;
//! - a drive root and `..` are never deleted;
//! - to the Recycle Bin: one call per selected object; what the bin cannot take is asked about
//!   ("Try to delete permanently?") and deleted for good only on Delete.

mod counter;
mod engine;
mod job;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_elevation;

use std::io;

pub use super::work::Report;
pub use counter::{Counter, Tally};
pub use engine::run;
pub use job::Job;

use crate::vfs::{Entry, VfsPath};

/// FAR's `delete_type` (its `erase`, the wipe, is deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// To the Recycle Bin (F8 with FAR's default "Delete to Recycle Bin").
    Trash,
    /// For good, past the Recycle Bin (Shift+Del).
    Remove,
}

/// One object the user chose, as the panel listed it (FAR's `SelFindData`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: VfsPath,
    pub entry: Entry,
}

/// One deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub items: Vec<Item>,
    pub mode: Mode,
    /// FAR's Confirmations > "Delete non-empty folders".
    pub ask_folders: bool,
    /// FAR's Confirmations > "Delete read-only files" (`Confirm.RO`).
    pub ask_read_only: bool,
    /// FAR's `DelShowTotal`: everything is counted, beside the deletion.
    pub count_totals: bool,
}

/// What the engine asks; the worker waits for the [`Reply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// "The following folder will be deleted" / "moved to the Recycle Bin": Delete, All
    /// ([`Reply::for_all`]), Skip, Cancel.
    NotEmpty { path: VfsPath, mode: Mode },
    /// "The following file is read only. Do you wish to delete it?": Delete, All, Skip, Skip all,
    /// Cancel.
    ReadOnly { path: VfsPath },
    /// "Cannot move the file / folder to the Recycle Bin. Try to delete permanently?": Delete, Skip,
    /// Skip all, Cancel.
    CannotTrash {
        path: VfsPath,
        folder: bool,
        message: String,
    },
    /// FAR's "Access denied" (ADR-0007): Elevate, Skip (`for_all`: the rest of the operation), Cancel.
    Elevate {
        path: VfsPath,
        verb: crate::jobs::elevation::Verb,
    },
    /// "Cannot delete the file / folder": Retry, Skip, Skip all, Cancel.
    Error {
        path: VfsPath,
        folder: bool,
        message: String,
        kind: io::ErrorKind,
        /// Who holds the file (FAR's `OperationFailed`).
        held: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Delete,
    Skip,
    Retry,
    Elevate,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    pub answer: Answer,
    /// All / Skip all: the same answer for the rest of the operation.
    pub for_all: bool,
}

impl Reply {
    pub fn once(answer: Answer) -> Self {
        Self {
            answer,
            for_all: false,
        }
    }

    pub fn always(answer: Answer) -> Self {
        Self {
            answer,
            for_all: true,
        }
    }
}

/// How far the deletion is, for FAR's "Deleting" progress.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// What is deleted (or counted) now.
    pub current: String,
    /// Objects deleted: files, folders, links (FAR's `ProcessedItems`).
    pub done: u64,
    /// Objects to delete, when counted (`count_totals`); final once `totals_known`.
    pub total: u64,
    pub totals_known: bool,
    /// Selected objects finished since the UI last looked; the UI takes their marks off.
    pub finished: Vec<VfsPath>,
}

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
    /// Whether "Access denied" can be offered at all.
    fn elevate_available(&self) -> bool {
        crate::platform::elevation::can_elevate()
    }
    /// Has the helper carry out `request`.
    fn run_elevated(&mut self, request: &crate::vfs::local::elevated::Request) -> io::Result<()> {
        crate::vfs::local::elevated_session::call(request, &mut |_, _| !self.cancelled())
    }
}
