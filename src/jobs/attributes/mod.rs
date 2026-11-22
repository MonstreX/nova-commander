//! The changes of FAR's "Attributes" dialog (`far/setattr.cpp`, ADR-0011), run as a background task
//! (ADR-0010): each selected item in FAR's order (`process_single_file`): the owner, compression,
//! encryption, sparseness, the reparse point, the other attributes, then the times; a link's new target
//! before all that (FAR's `ModifyReparsePoint`). A change refused for want of rights asks FAR's "Access
//! denied" first and goes through the elevated helper (ADR-0007). Each change that fails asks Retry /
//! Skip / Skip all / Cancel (FAR's `retryable_ui_operation`); Skip leaves out that one change and goes on
//! with the next. With "Process subfolders" everything inside the selected folders follows
//! (FAR's `ScanTree`): the boxes set or cleared, a typed time with each object's own parts where left
//! blank, the owner.

mod engine;
mod job;
mod plan;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_elevation;
pub mod times;

use std::path::PathBuf;
use std::time::SystemTime;

pub use super::work::Report;
pub use engine::run;
pub use job::Job;
pub use plan::{Change, changes, failure, verb};

use self::times::TimeFormat;
use crate::jobs::elevation::Verb;
use crate::vfs::VfsPath;

/// One of the four time lines as the dialog left it.
/// The fields; a part left blank keeps each object's own (all blank, the default, keeps the time).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeEdit {
    pub date: String,
    pub time: String,
    /// The line is as the dialog first showed it for the selected objects, which keep their own time
    /// (local time read back in the hour that repeats when daylight saving ends could move it by an
    /// hour); what is inside their folders takes it, as FAR compares the line with each object's own.
    pub as_shown: bool,
}

/// What "Set" changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The objects the user chose; each is read again when its turn comes.
    pub items: Vec<PathBuf>,
    /// Each attribute in [`Flag::ALL`](crate::platform::files::Flag::ALL)'s order: on, off, or `None`
    /// (`[?]`) to leave as each object has it.
    pub wanted: [Option<bool>; 20],
    /// The owner to give; blank leaves it.
    pub owner: String,
    /// Write, creation, access, change.
    pub times: [TimeEdit; 4],
    pub utc: bool,
    pub format: TimeFormat,
    /// FAR's "Process subfolders": everything inside the selected folders too, links to folders followed
    /// (FAR's default `ScanJunction`), each place once.
    pub subfolders: bool,
    /// The one selected link's new target, environment variables expanded (FAR's `ModifyReparsePoint`).
    pub link_target: Option<String>,
}

impl Request {
    /// A line's new time for an object whose own time is `original`; `selected`: one the user chose,
    /// not one inside its folder.
    pub fn time(
        &self,
        row: usize,
        original: Option<SystemTime>,
        selected: bool,
    ) -> Option<SystemTime> {
        let edit = &self.times[row];
        if edit.as_shown && selected {
            return original;
        }
        times::parse((&edit.date, &edit.time), original, self.utc, self.format)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// FAR's `OperationFailed`: what could not be done, the name, the system's reason.
    Failed {
        /// FAR's `MSetAttr...CannotFor`.
        failure: &'static str,
        path: PathBuf,
        message: String,
    },
    /// FAR's "Access denied" (ADR-0007 §2).
    Elevate { path: PathBuf, verb: Verb },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Retry,
    Skip,
    SkipAll,
    Cancel,
    /// "Access denied": Ok, and whether "Do this for all current objects" was on.
    Elevate {
        for_all: bool,
    },
    /// "Access denied": Skip.
    Refuse {
        for_all: bool,
    },
}

/// How far the task is, for FAR's "Setting file attributes for" progress.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// What is changed now.
    pub current: String,
    /// Selected objects finished since the UI last looked; the UI takes their marks off.
    pub finished: Vec<VfsPath>,
}

/// The engine's link to whoever runs it: the worker thread's channels, or a script in tests.
pub trait Host {
    fn ask(&mut self, question: Question) -> Reply;
    /// Whether the user stopped the task; may wait while it is paused.
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
    fn run_elevated(
        &mut self,
        request: &crate::vfs::local::elevated::Request,
    ) -> std::io::Result<()> {
        crate::vfs::local::elevated_session::call(request, &mut |_, _| !self.cancelled())
    }
}
