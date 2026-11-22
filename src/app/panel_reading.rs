//! A panel's folder read on a worker (ADR-0013 §5): entered or read again, waited for up to `read_wait`
//! and then taken between input events; an automatic refresh is never waited for (ADR-0019); Esc keeps
//! what was read of a folder entered, and the panel as it was before a folder read again (review R23).

use std::collections::BTreeSet;
use std::io;
use std::time::{Duration, Instant};

use super::{AUTO_REFRESH_GAP, Panel, Polled, QUIET_REFRESH, READ_WAIT};
use crate::jobs::listing::{self, Listed, Listing, Request};
use crate::vfs::VfsPath;

/// A folder that cannot be entered, as the command line and the panels' keys say it.
const CHANGE_DIRECTORY: &str = "Change directory";

/// A folder read in the background (ADR-0013 §5).
pub(super) struct Reading {
    pub(super) listing: Listing,
    /// The folder being read.
    pub(super) target: VfsPath,
    /// The name to put the cursor on.
    pub(super) focus: Option<String>,
    /// Entering another folder (a failure keeps the panel where it is), or reading this one again.
    pub(super) entering: bool,
    /// The count the title shows.
    pub(super) shown: usize,
    /// An automatic refresh: the title says nothing of it until then.
    pub(super) quiet_until: Option<Instant>,
    /// The title its failure is shown under.
    pub(super) failure_title: String,
}

impl std::fmt::Debug for Reading {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Reading")
            .field("dir", self.listing.dir())
            .field("entering", &self.entering)
            .finish_non_exhaustive()
    }
}

impl Panel {
    /// Its folder read again never waited for (the screen not held, as the automatic refresh does); while
    /// another folder is being entered, left to the automatic refresh after it.
    pub fn reload_quietly(&mut self) {
        if self
            .reading
            .as_ref()
            .is_some_and(|reading| reading.entering)
        {
            self.change_pending = true;
            return;
        }
        if let Some(result) = self.begin_within(self.path.clone(), None, false, Duration::ZERO) {
            self.reloaded(result);
        }
    }

    /// FAR's background update: a folder changed on disk is read again, cursor and selection kept, while
    /// the panel is on screen (`shown`), at most once a second; a change meanwhile waits. `limit`: FAR's
    /// "Disable automatic panel update if object count exceeds", 0 for none. True when a reading began.
    pub fn refresh_if_changed(&mut self, shown: bool, limit: usize) -> bool {
        if self.watch.as_ref().is_some_and(|watch| watch.take_change()) {
            self.change_pending = true;
        }
        let too_soon = self
            .last_auto_refresh
            .is_some_and(|at| at.elapsed() < AUTO_REFRESH_GAP);
        if !self.change_pending || !shown || self.reading.is_some() || too_soon {
            return false;
        }
        self.change_pending = false;
        if limit > 0 && self.entries.len() > limit {
            return false;
        }
        self.last_auto_refresh = Some(Instant::now());
        // Never waited for: a slow share that keeps changing would hold the screen once a second.
        if let Some(result) = self.begin_within(self.path.clone(), None, false, Duration::ZERO) {
            self.reloaded(result);
        }
        if let Some(reading) = &mut self.reading {
            reading.quiet_until = Some(Instant::now() + QUIET_REFRESH);
        }
        true
    }

    /// Starts reading `dir`: the result if it comes within `read_wait`; otherwise the reading goes on in
    /// the background. A newer request supersedes the one in flight (dropping it stops its worker).
    pub(super) fn begin(
        &mut self,
        dir: VfsPath,
        focus: Option<&str>,
        entering: bool,
    ) -> Option<io::Result<Listed>> {
        self.begin_within(dir, focus, entering, self.read_wait)
    }

    /// [`begin`](Self::begin) waiting up to `wait`.
    fn begin_within(
        &mut self,
        dir: VfsPath,
        focus: Option<&str>,
        entering: bool,
        wait: Duration,
    ) -> Option<io::Result<Listed>> {
        self.reading = None;
        // A pushd waiting for another folder is not for this one (one entered already is kept to be
        // taken).
        self.pushd.take_if(|(_, entered)| !*entered);
        let request = Request {
            refresh: !entering,
            watch_remote: self.watch_remote,
            share_parent: self.share_parent,
        };
        let listing = listing::start(dir.clone(), request);
        let result = listing.wait(wait);
        if result.is_none() {
            self.reading = Some(Reading {
                listing,
                target: dir,
                focus: focus.map(str::to_string),
                entering,
                shown: 0,
                quiet_until: None,
                failure_title: CHANGE_DIRECTORY.to_string(),
            });
        }
        result
    }

    pub(super) fn entered(&mut self, listed: Listed, focus: Option<&str>) {
        let reselect = self.track_plugins(&listed.dir, &listed.look);
        self.look = listed.look;
        self.path = listed.dir;
        self.watch = listed.watch;
        self.capabilities = listed.capabilities;
        self.change_pending = false;
        self.selected.clear();
        // Before the sort: selected first sorts them up (FAR's `PopPrevData` sorts after `MoveSelection`).
        self.reselect(reselect, &listed.entries);
        self.size_count = None;
        self.offset = 0;
        self.error = None;
        self.set_entries(listed.entries, focus);
    }

    /// The folder read again: the cursor stays on the name it is on now (the user may have moved it
    /// while the reading went on), vanished names leave the selection. A folder that is gone leaves
    /// the panel in the nearest existing one above it (FAR's `SetCurPath`).
    pub(super) fn reloaded(&mut self, result: io::Result<Listed>) {
        self.reloaded_onto(result, None);
    }

    /// The folder read again as [`Self::reloaded`] has it, the cursor on the name `onto` when it is
    /// given and there (FAR's `Update(UPDATE_KEEP_SELECTION)` then `GoToFile`, which finds no other).
    pub(super) fn reloaded_onto(&mut self, result: io::Result<Listed>, onto: Option<&str>) {
        let current = self.current().map(|entry| entry.name.clone());
        let (row, next) = self.cursor_fallback();
        match result {
            Ok(listed) if listed.dir != self.path => self.entered(listed, None),
            Ok(mut listed) => {
                let there = |name: &&str| listed.entries.iter().any(|entry| entry.name == *name);
                let focus = (onto.filter(there).map(str::to_string)).or(current);
                self.keep_folder_sizes(&mut listed.entries);
                self.error = None;
                // The new watch was opened before the reading: what the old one saw is in it.
                self.watch = listed.watch;
                self.capabilities = listed.capabilities;
                self.look = listed.look;
                self.change_pending = false;
                self.set_entries(listed.entries, focus.as_deref());
                self.keep_cursor_place(focus.as_deref(), row, next.as_deref());
            }
            Err(error) => {
                // A folder deleted while another program still holds it denies access until it is
                // gone: a readable folder that turns so is read once more a moment later.
                let first_failure = self.error.is_none();
                self.change_pending =
                    first_failure && error.kind() == io::ErrorKind::PermissionDenied;
                if self.change_pending {
                    // Ctrl+R as well as a change: the folder gets the gap to go.
                    self.last_auto_refresh = Some(Instant::now());
                }
                self.entries.clear();
                self.entries_changed();
                self.cursor = 0;
                self.error = Some(error.to_string());
                self.watch = None;
            }
        }
        let names: BTreeSet<&str> = self.entries.iter().map(|e| e.name.as_str()).collect();
        self.selected.retain(|name| names.contains(name.as_str()));
    }

    /// The entries shown put through the hidden files' switch and the filter as they are now, the cursor
    /// and the selection kept on what stays (what they would add needs the folder read).
    fn refilter(&mut self) {
        let focus = self.current().map(|entry| entry.name.clone());
        let entries = std::mem::take(&mut self.entries);
        self.set_entries(entries, focus.as_deref());
        let names: BTreeSet<&str> = self.entries.iter().map(|e| e.name.as_str()).collect();
        self.selected.retain(|name| names.contains(name.as_str()));
    }

    /// Takes a reading that finished in the background; called between input events.
    pub fn poll(&mut self) -> Polled {
        let Some(reading) = self.reading.take() else {
            return Polled::Quiet;
        };
        let Some(result) = reading.listing.wait(Duration::ZERO) else {
            let read = reading.listing.read_so_far();
            let changed = read != reading.shown;
            self.reading = Some(Reading {
                shown: read,
                ..reading
            });
            return if changed {
                Polled::Changed
            } else {
                Polled::Quiet
            };
        };
        self.settle(reading, result)
    }

    fn settle(&mut self, reading: Reading, result: io::Result<Listed>) -> Polled {
        if !reading.entering {
            self.reloaded_onto(result, reading.focus.as_deref());
            return Polled::Changed;
        }
        match result {
            Ok(listed) => {
                self.entered(listed, reading.focus.as_deref());
                if let Some((_, entered)) = &mut self.pushd {
                    *entered = true;
                }
                Polled::Changed
            }
            Err(error) => {
                self.pushd = None;
                // A pop that failed leaves the plugins' panels as they are.
                self.plugin_move_failed();
                Polled::Failed(reading.failure_title, error)
            }
        }
    }

    /// The folder being entered in the background, if it fails, is said to under `title`.
    pub fn title_failure(&mut self, title: &str) {
        if let Some(reading) = self.reading.as_mut().filter(|reading| reading.entering) {
            reading.failure_title = title.to_string();
        }
    }

    /// `pushd`: `folder` to remember once the folder being entered now is; it, when that one already
    /// is.
    pub fn push_when_entered(&mut self, folder: String) -> Option<String> {
        if !self
            .reading
            .as_ref()
            .is_some_and(|reading| reading.entering)
        {
            return Some(folder);
        }
        self.pushd = Some((folder, false));
        None
    }

    /// The folder `pushd` remembers, once the folder it entered is.
    pub fn take_pushed(&mut self) -> Option<String> {
        (self.pushd.take_if(|(_, entered)| *entered)).map(|(folder, _)| folder)
    }

    /// Whether a folder is read in the background, an automatic refresh too (the panel is busy).
    pub fn is_reading(&self) -> bool {
        self.reading.is_some()
    }

    /// The entries read so far while a folder is read in the background (the title's "Reading: N
    /// files"); none for an automatic refresh in its first moment, which the title does not show.
    pub fn reading(&self) -> Option<usize> {
        let quiet =
            |reading: &&Reading| (reading.quiet_until).is_some_and(|until| Instant::now() < until);
        (self.reading.as_ref())
            .filter(|reading| !quiet(reading))
            .map(|reading| reading.shown)
    }

    /// The reading going on shown in the title only after a moment, as a reading waited for is: a
    /// plugin's own panel read at its asking (each second for the process list) starts without the
    /// screen waiting, and its title must not flash "Reading" for a read that takes a moment.
    pub fn quiet_reading(&mut self) {
        if let Some(reading) = &mut self.reading {
            reading.quiet_until = Some(Instant::now() + QUIET_REFRESH);
        }
    }

    /// How long a folder entered is waited for before the screen goes on.
    pub fn read_wait(&self) -> Duration {
        self.read_wait
    }

    /// The folder being read, if one is.
    pub fn reading_target(&self) -> Option<&VfsPath> {
        self.reading.as_ref().map(|reading| &reading.target)
    }

    /// How long `change_dir` waits before the reading goes on in the background.
    pub fn set_read_wait(&mut self, wait: Duration) {
        self.read_wait = wait;
    }

    /// Esc -> Yes while reading: what was read so far of a folder entered is shown. A folder read again
    /// goes back to before, its entries and selection whole (FAR shows the part read and drops the rest's
    /// selection: a stopped refresh would lose what the panel showed, review R23). A reading stopped
    /// before anything of it was read (or failed after the stop) leaves the panel as it is; a worker the
    /// OS holds up (a share that does not answer) is not waited for longer than `READ_WAIT`: it is left
    /// to end on its own and the panel stays as it is.
    pub fn stop_reading(&mut self) {
        let Some(reading) = self.reading.take() else {
            return;
        };
        reading.listing.stop();
        if !reading.entering {
            // Dropped: its worker's call broken, nothing of it taken. What asked for it (Ctrl+H, a
            // filter) changed already: the old list is put through it, its selection kept on what stays.
            self.refilter();
            return;
        }
        let Some(result) = reading.listing.wait(READ_WAIT) else {
            self.plugin_move_failed();
            return;
        };
        let nothing_read = result.as_ref().map_or(true, |listed| {
            listed.entries.iter().all(|entry| entry.is_parent)
        });
        // Stopped before anything came, or failed after the stop: the panel stays as it is shown (a pop
        // too: the plugins' panels stay as they are).
        if nothing_read {
            self.plugin_move_failed();
            return;
        }
        let _ = self.settle(reading, result);
    }
}
