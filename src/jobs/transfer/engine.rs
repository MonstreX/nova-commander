//! Walking the sources: refusals, moves by rename, folders merged into, links, the choice for an
//! existing name, and the error question with Retry. Files themselves are `file`'s.

use std::io;

use super::sides::{self, disks};
use super::{Answer, Existing, Host, Question, Report, Request};
use crate::jobs::elevation::{Approval, Verb};
use crate::platform::files::{Attributes, FileTimes};
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::{Entry, VfsPath, provider_for};

/// The user cancelled: the walk unwinds without touching anything else.
pub(super) struct Stopped;

pub(super) type Flow = Result<(), Stopped>;

/// What to do with a source whose target name exists.
pub(super) enum Action {
    /// Write a new file under this name.
    New(VfsPath),
    /// Write over this file (the target, or the name Rename gave).
    Replace(VfsPath),
    Append(VfsPath),
    Skip,
}

/// One running operation.
pub(super) struct Run<'a> {
    pub(super) request: &'a Request,
    pub(super) host: &'a mut dyn Host,
    /// "Remember choice" in the question about an existing file.
    exists: Option<Answer>,
    /// "All" (true) or "Skip all" (false) in the read-only question.
    read_only: Option<bool>,
    /// "All in place" in the low-space warning.
    pub(super) in_place_all: bool,
    /// The target's provider renames nothing (a plugin's panel that only writes): a file is replaced
    /// in place, the old one removed first.
    pub(super) no_rename: bool,
    /// "Skip all" in the error question.
    skip_errors: bool,
    /// Who holds the object of the failure about to be asked about.
    held: Vec<String>,
    /// The answers to "Access denied" for the rest of the operation.
    pub(super) approval: Approval,
    /// The helper has done something here: cleaning up after it needs no question.
    pub(super) helper_used: bool,
    /// Set as soon as the user cancels, also inside a copy.
    pub(super) stopped: bool,
    /// The failed step must not be repeated (an append that could not be undone).
    pub(super) no_retry: bool,
    /// Something of the selected object at hand was skipped or failed: it keeps its mark.
    pub(super) incomplete: bool,
    /// The folders made, with the times they get at the end ("Preserve all timestamps") and, made through
    /// a provider, their source's marks.
    pub(super) new_folders: Vec<(VfsPath, FileTimes, Option<Attributes>)>,
    /// How many followed links lie above what is at hand: it stays where it is on a move.
    pub(super) inside_links: u32,
    /// The folders on the way here, when links are followed: the guard against a link back up.
    pub(super) walking: Vec<(u32, u64)>,
    /// The selected object at hand, and the object within it, for "Retry skipped".
    pub(super) top: Option<VfsPath>,
    pub(super) current: Option<(VfsPath, VfsPath)>,
    pub(super) report: Report,
}

/// Runs the operation to its end, asking `host` whatever needs the user.
pub fn run(request: &Request, host: &mut dyn Host) -> Report {
    let mut run = Run {
        request,
        host,
        exists: None,
        read_only: None,
        in_place_all: false,
        no_rename: false,
        skip_errors: false,
        held: Vec::new(),
        approval: Approval::default(),
        helper_used: false,
        stopped: false,
        no_retry: false,
        incomplete: false,
        new_folders: Vec::new(),
        inside_links: 0,
        walking: Vec::new(),
        top: None,
        current: None,
        report: Report::default(),
    };
    if !request.retrying.is_empty() {
        run.retry_skipped();
        run.keep_folder_times();
        run.report.cancelled = run.stopped;
        return run.report;
    }
    // A case-only rename of the one selected item ("dir" to "Dir"): the target "is" a folder only
    // because the file system ignores case.
    if let [source] = request.sources.as_slice()
        && request.move_
        && sides::is_same(source, &request.target)
        && source.file_name() != request.target.file_name()
    {
        let _ = run.attempt(source, |run| run.rename(source, &request.target, false));
        run.report.cancelled = run.stopped;
        run.report.renamed = request.renamed(false, &run.report);
        return run.report;
    }
    let into = sides::leads_to_folder(&request.target).unwrap_or(false);
    if request.sources.len() > 1 && !into {
        run.fail(
            &request.target,
            "is not an existing folder; several items can only go into a folder",
        );
        return run.report;
    }
    for source in &request.sources {
        let destination = match source.file_name() {
            Some(name) if into => {
                if !run.name_fits(source, &name, &request.target) {
                    continue;
                }
                request.target.join(&name)
            }
            Some(_) => request.target.clone(),
            None => {
                run.fail(source, "has no name");
                continue;
            }
        };
        run.top = Some(source.clone());
        if run.selected(source, &destination).is_err() {
            break;
        }
    }
    run.keep_folder_times();
    run.report.cancelled = run.stopped;
    run.report.renamed = request.renamed(into, &run.report);
    run.report
}

impl Run<'_> {
    /// A selected item, reported as finished when all of it went (FAR's `ClearLastGetSelection`).
    pub(super) fn selected(&mut self, source: &VfsPath, destination: &VfsPath) -> Flow {
        self.incomplete = false;
        self.top(source, destination)?;
        if !self.incomplete {
            self.host
                .progress(&mut |progress| progress.finished.push(source.clone()));
        }
        Ok(())
    }

    /// A selected item: the refusals first (onto itself, into itself), then the item.
    fn top(&mut self, source: &VfsPath, destination: &VfsPath) -> Flow {
        if sides::is_same(source, destination) {
            // A case-only rename on a case-insensitive file system is a rename, not a copy onto itself.
            if self.moves() && source.file_name() != destination.file_name() {
                self.attempt(source, |run| run.rename(source, destination, false))?;
                return Ok(());
            }
            self.fail(source, "cannot be copied or moved onto itself");
            return Ok(());
        }
        // A junction or a symbolic link may stand inside what it leads to; a walk may not.
        if self.request.walks() && sides::is_inside(source, destination) {
            self.fail(source, "a folder cannot be copied or moved into itself");
            return Ok(());
        }
        self.item(source, destination)
    }

    pub(super) fn item(&mut self, source: &VfsPath, destination: &VfsPath) -> Flow {
        self.at_object(source, destination, |run| run.object(source, destination))
    }

    fn object(&mut self, source: &VfsPath, destination: &VfsPath) -> Flow {
        if self.host.cancelled() {
            self.stopped = true;
            return Err(Stopped);
        }
        let Some(entry) = self.attempt(source, |_| sides::stat(source))? else {
            return Ok(());
        };
        if self.request.filters_out(&entry) {
            // Left where it is; a selected one keeps its selection (FAR's `continue`).
            if self.top.as_ref() == Some(source) {
                self.incomplete = true;
            }
            return Ok(());
        }
        if let Some(kind) = self.request.link {
            return self.linked(source, destination, &entry, kind);
        }
        let follows = self.follows(source, &entry);
        // With a filter a folder is walked, not renamed whole (FAR's `m_UseFilter || !SameDisk`).
        let whole = !(self.request.filter.is_some() && sides::is_real_folder(&entry));
        if self.moves() && !follows && whole && self.renamed(source, destination, &entry)? {
            return Ok(());
        }
        if follows {
            return self.link_contents(source, destination);
        }
        if entry.points_elsewhere() {
            return self.link(source, destination);
        }
        if entry.is_dir {
            return self.dir(source, destination, &entry);
        }
        self.file(source, destination, &entry)
    }

    /// A rename within one volume or one provider; `replace` replaces an existing file in one step,
    /// without it an existing target is never touched. `CrossesDevices` when nothing can rename from
    /// one to the other.
    pub(super) fn rename(&mut self, from: &VfsPath, to: &VfsPath, replace: bool) -> io::Result<()> {
        if let Some((from_disk, to_disk)) = disks(from, to) {
            let result = self.host.move_item(&from_disk, &to_disk, replace);
            // A temporary copy taking its target's place is the target's replacement to the user.
            let (object, verb) = if sides::is_temporary(from) {
                (to, Verb::Replace)
            } else {
                (from, Verb::Move)
            };
            if !self.wants_helper(object, verb, &result) {
                return result;
            }
            let request = Elevated::MoveFile {
                from: from_disk,
                to: to_disk,
                replace,
            };
            return self.through_helper(result, &request, None);
        }
        if !sides::same_place(from, to) {
            return Err(io::ErrorKind::CrossesDevices.into());
        }
        provider_for(from)?
            .rename(from, to, replace)
            .map_err(|error| match error.kind() {
                io::ErrorKind::Unsupported => {
                    io::Error::new(io::ErrorKind::CrossesDevices, error.to_string())
                }
                _ => error,
            })
    }

    /// A move within one volume: one rename, whatever the size of the tree; an existing target is
    /// never replaced by it. False when the target exists (merge or question) or is on another volume
    /// (copy and delete).
    fn renamed(
        &mut self,
        source: &VfsPath,
        destination: &VfsPath,
        entry: &Entry,
    ) -> Result<bool, Stopped> {
        if sides::stat(destination).is_ok() {
            return Ok(false);
        }
        let renamed = self.attempt(source, |run| match run.rename(source, destination, false) {
            Ok(()) => Ok(true),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::CrossesDevices | io::ErrorKind::AlreadyExists
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        })?;
        if renamed == Some(true) {
            self.after_rename(destination);
        }
        if renamed == Some(true) && !sides::is_real_folder(entry) {
            let size = entry.size;
            self.host.progress(&mut |progress| {
                progress.files_done += 1;
                progress.bytes_done += size;
            });
        }
        // Skipped after an error counts as handled: nothing more is done with this item.
        Ok(renamed.unwrap_or(true))
    }

    pub(super) fn dir(&mut self, source: &VfsPath, destination: &VfsPath, entry: &Entry) -> Flow {
        let place = (self.request.keep.link_contents)
            .then(|| sides::identity(source))
            .flatten();
        self.walking.extend(place);
        let walked = self.walk_dir(source, destination, entry);
        if place.is_some() {
            self.walking.pop();
        }
        walked
    }

    /// A folder: merged into one that exists (FAR), created otherwise; a moved folder is removed once
    /// it is empty (what was skipped stays in it).
    fn walk_dir(&mut self, source: &VfsPath, destination: &VfsPath, entry: &Entry) -> Flow {
        // Read before anything is written into the target, which may be inside it through a link.
        let stop = self.host.stop_flag();
        let Some(names) = self.attempt(source, |_| sides::names(source, &stop))? else {
            return Ok(());
        };
        match sides::leads_to_folder(destination) {
            Ok(true) => {}
            Ok(false) => {
                self.fail(destination, "a file with this name exists");
                return Ok(());
            }
            Err(_) => {
                if self
                    .attempt(destination, |run| {
                        run.elevating(
                            destination,
                            Verb::Create,
                            || sides::elevated_make_dir(destination),
                            || sides::make_dir(destination),
                        )
                    })?
                    .is_none()
                {
                    return Ok(());
                }
                self.keep_new_folder(source, destination, entry);
            }
        }
        for name in names {
            if self.name_fits(source, &name, destination) {
                self.item(&source.join(&name), &destination.join(&name))?;
            }
        }
        let stop = self.host.stop_flag();
        if self.moves() && sides::names(source, &stop).is_ok_and(|names| names.is_empty()) {
            self.attempt(source, |run| {
                run.elevating(
                    source,
                    Verb::Delete,
                    || sides::elevated_remove_dir(source),
                    || sides::remove_empty_dir(source),
                )
            })?;
        }
        Ok(())
    }

    /// A name a provider gave, about to become part of a path, must be one name there: `..\\x` or
    /// `C:\\x` from a listing would write outside the target, `a:b` into a file's stream.
    fn name_fits(&mut self, source: &VfsPath, name: &str, destination: &VfsPath) -> bool {
        let fits = source.as_local().is_some() || destination.accepts_name(name);
        if !fits {
            self.fail(source, &format!("\"{name}\" cannot be a name there"));
        }
        fits
    }

    /// A link is copied as a link; one that exists is replaced through a temporary link. Only a disk
    /// holds links for now (3b step 5).
    pub(super) fn link(&mut self, source: &VfsPath, destination: &VfsPath) -> Flow {
        if disks(source, destination).is_none() {
            self.fail(source, "a link cannot be copied between these places");
            self.host.progress(&mut |progress| progress.files_done += 1);
            return Ok(());
        }
        let action = match sides::stat(destination) {
            Err(_) => Action::New(destination.clone()),
            Ok(existing) if sides::is_real_folder(&existing) => {
                self.fail(destination, "a folder with this name exists");
                return Ok(());
            }
            Ok(existing) => {
                let read_only = existing.attributes.is_read_only();
                self.decide(source, destination, read_only)?
            }
        };
        let done = match action {
            Action::Skip => false,
            Action::Append(_) => {
                self.fail(source, "a link cannot be appended to a file");
                false
            }
            Action::New(path) => self
                .attempt(source, |run| run.copy_link_elevating(source, &path))?
                .is_some(),
            Action::Replace(path) => self.replace_with_link(source, &path)?,
        };
        self.incomplete |= !done;
        if done && self.moves() {
            self.attempt(source, |run| run.remove_link(source))?;
        }
        self.host.progress(&mut |progress| progress.files_done += 1);
        Ok(())
    }

    fn replace_with_link(
        &mut self,
        source: &VfsPath,
        destination: &VfsPath,
    ) -> Result<bool, Stopped> {
        let temp = sides::temp_sibling(destination);
        if self
            .attempt(source, |run| run.copy_link_elevating(source, &temp))?
            .is_none()
        {
            return Ok(false);
        }
        let replaced = self.attempt(destination, |run| {
            run.writable(destination, |run| run.rename(&temp, destination, true))
        });
        if !matches!(replaced, Ok(Some(()))) {
            self.discard(&temp, true);
        }
        Ok(replaced?.is_some())
    }

    /// The dialog's choice for an existing target, the question when it is Ask, then the read-only
    /// question when the choice writes into a read-only file.
    pub(super) fn decide(
        &mut self,
        source: &VfsPath,
        target: &VfsPath,
        read_only: bool,
    ) -> Result<Action, Stopped> {
        let mut name = None;
        let answer = match self.exists {
            Some(answer) => answer,
            None => match self.request.existing {
                Existing::Ask => {
                    let question = Question::Exists {
                        source: sides::facts(source),
                        target: sides::facts(target),
                        append: self.request.link.is_none(),
                        free: sides::free_name(target),
                    };
                    let reply = self.host.ask(question);
                    if reply.for_all {
                        self.exists = Some(reply.answer);
                    }
                    name = reply.name;
                    reply.answer
                }
                Existing::Overwrite => Answer::Overwrite,
                Existing::Skip => Answer::Skip,
                Existing::Rename => Answer::Rename,
                Existing::Append => Answer::Append,
                Existing::OnlyNewer
                    if sides::facts(target).modified < sides::facts(source).modified =>
                {
                    Answer::Overwrite
                }
                Existing::OnlyNewer => Answer::Skip,
            },
        };
        let writes = matches!(answer, Answer::Overwrite | Answer::Append);
        if writes && read_only && !self.may_write_read_only(target)? {
            return Ok(Action::Skip);
        }
        match answer {
            Answer::Overwrite => Ok(Action::Replace(target.clone())),
            Answer::Append => Ok(Action::Append(target.clone())),
            Answer::Rename => match name {
                Some(name) => self.rename_to(source, &name),
                None => Ok(Action::New(sides::free_name(target))),
            },
            Answer::Cancel => self.stop(),
            _ => Ok(Action::Skip),
        }
    }

    /// FAR's Rename with a name (its `Retry`): a free name is written, one that exists is asked about
    /// again; a folder, or the source itself, fails.
    fn rename_to(&mut self, source: &VfsPath, name: &VfsPath) -> Result<Action, Stopped> {
        match sides::stat(name) {
            Err(_) => Ok(Action::New(name.clone())),
            Ok(existing) if sides::is_real_folder(&existing) => {
                self.fail(name, "a folder with this name exists");
                Ok(Action::Skip)
            }
            Ok(_) if sides::is_same(source, name) => {
                self.fail(source, "cannot be copied or moved onto itself");
                Ok(Action::Skip)
            }
            Ok(existing) => self.decide(source, name, existing.attributes.is_read_only()),
        }
    }

    fn may_write_read_only(&mut self, target: &VfsPath) -> Result<bool, Stopped> {
        if !self.request.ask_read_only {
            return Ok(true);
        }
        if let Some(all) = self.read_only {
            return Ok(all);
        }
        let reply = self.host.ask(Question::ReadOnly {
            target: target.clone(),
        });
        let yes = match reply.answer {
            Answer::Overwrite => true,
            Answer::Cancel => return self.stop(),
            _ => false,
        };
        if reply.for_all {
            self.read_only = Some(yes);
        }
        Ok(yes)
    }

    /// Runs `op` until it succeeds, asking Retry / Skip / Skip all / Cancel after each failure.
    /// `None`: skipped (the failure is in the report).
    pub(super) fn attempt<T>(
        &mut self,
        path: &VfsPath,
        op: impl FnMut(&mut Self) -> io::Result<T>,
    ) -> Result<Option<T>, Stopped> {
        self.attempt_on(path, None, op)
    }

    /// [`attempt`](Self::attempt) of a step that acts on `also` as well: a holder of either is named.
    pub(super) fn attempt_on<T>(
        &mut self,
        path: &VfsPath,
        also: Option<&VfsPath>,
        mut op: impl FnMut(&mut Self) -> io::Result<T>,
    ) -> Result<Option<T>, Stopped> {
        let objects: Vec<&VfsPath> = std::iter::once(path).chain(also).collect();
        loop {
            let error = match op(self) {
                Ok(value) => return Ok(Some(value)),
                Err(error) => error,
            };
            // A listing the task's cancel stopped (review of 19-2): the cancel, not a failure.
            let interrupted = error.kind() == io::ErrorKind::Interrupted
                && self
                    .host
                    .stop_flag()
                    .load(std::sync::atomic::Ordering::Relaxed);
            if interrupted {
                return self.stop();
            }
            if self.stopped {
                return Err(Stopped);
            }
            let retry = !std::mem::take(&mut self.no_retry);
            if !self.skip_errors {
                self.held = crate::jobs::held::held_lines(&objects, &error);
            }
            if !self.ask_after_error(path, &error.to_string(), retry)? {
                return Ok(None);
            }
        }
    }

    /// The error question; true for Retry. Skip records the failure.
    pub(super) fn ask_after_error(
        &mut self,
        path: &VfsPath,
        message: &str,
        retry: bool,
    ) -> Result<bool, Stopped> {
        if self.stopped {
            return Err(Stopped);
        }
        if !self.skip_errors {
            let reply = self.host.ask(Question::Error {
                path: path.clone(),
                message: message.to_string(),
                held: std::mem::take(&mut self.held),
                retry,
            });
            match reply.answer {
                Answer::Retry if retry => return Ok(true),
                Answer::Cancel => return self.stop(),
                _ => self.skip_errors = reply.for_all,
            }
        }
        self.fail(path, message);
        self.remember_skipped();
        Ok(false)
    }

    pub(super) fn stop<T>(&mut self) -> Result<T, Stopped> {
        self.stopped = true;
        Err(Stopped)
    }

    pub(super) fn fail(&mut self, path: &VfsPath, message: &str) {
        self.incomplete = true;
        self.report.failures.push(format!("{path}: {message}"));
    }
}

/// Makes a link like `source` at `path`; both are on a disk ([`Run::link`] looked).
pub(super) fn copy_link(source: &VfsPath, path: &VfsPath) -> io::Result<()> {
    match disks(source, path) {
        Some((source, path)) => crate::platform::files::copy_link(&source, &path),
        None => Err(io::ErrorKind::Unsupported.into()),
    }
}
