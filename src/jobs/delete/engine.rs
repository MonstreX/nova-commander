//! The deletion itself, FAR's `ShellDelete`: each selected object in turn, folders walked depth first
//! when deleting past the Recycle Bin, FAR's questions on the way.

use std::io;
use std::sync::atomic::AtomicBool;

use super::{Answer, Host, Item, Mode, Progress, Question, Report, Request};
use crate::jobs::elevation::{Approval, Decision, Verb};
use crate::platform;
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::local::elevated_session::is_not_started;
use crate::vfs::{self, Entry, Provider, Removal, VfsPath};

/// The user cancelled: the rest is left as it is.
struct Cancelled;

type Step<T> = Result<T, Cancelled>;

/// What came of moving one object to the Recycle Bin.
enum Trashed {
    Done,
    Skipped,
    /// "Try to delete permanently?" answered Delete.
    RemoveInstead,
}

/// Runs the deletion to its end, or until the user cancels.
pub fn run(request: &Request, host: &mut dyn Host) -> Report {
    let mut deletion = Deletion {
        host,
        mode: request.mode,
        delete_folders: !request.ask_folders,
        read_only: (!request.ask_read_only).then_some(true),
        skip_errors: false,
        approval: Approval::default(),
        failures: Vec::new(),
    };
    let result = deletion.run(request);
    Report {
        failures: deletion.failures,
        cancelled: result.is_err(),
        ..Report::default()
    }
}

/// What the helper does for a removal of a local object, and FAR's reason for it.
fn elevated_removal(path: &VfsPath, how: Removal) -> Option<(Elevated, Verb)> {
    let path = path.as_local()?.to_path_buf();
    Some(match how {
        Removal::File => (Elevated::DeleteFile(path), Verb::Delete),
        Removal::EmptyDir => (Elevated::RemoveDirectory(path), Verb::Delete),
        Removal::Trash => (Elevated::MoveToTrash(path), Verb::Recycle),
    })
}

/// Whether a selected object is no longer there: one that was skipped, or of which something stayed,
/// keeps its mark.
fn is_gone(path: &VfsPath) -> bool {
    let found = vfs::provider_for(path).and_then(|provider| provider.stat(path));
    found.is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
}

/// FAR's `ShellDelete` state: the answers given "for all".
struct Deletion<'a> {
    host: &'a mut dyn Host,
    mode: Mode,
    /// "All" to a folder that is not empty, or the question turned off (FAR's `m_DeleteFolders`).
    delete_folders: bool,
    /// "All" (`Some(true)`) or "Skip all" (`Some(false)`) to a read-only file (`m_DeleteReadOnly`).
    read_only: Option<bool>,
    /// "Skip all" to an error (`m_SkipErrors`).
    skip_errors: bool,
    /// The answers to "Access denied" for the rest of the operation.
    approval: Approval,
    failures: Vec<String>,
}

impl Deletion<'_> {
    fn run(&mut self, request: &Request) -> Step<()> {
        for item in &request.items {
            self.item(item)?;
            if is_gone(&item.path) {
                self.host
                    .progress(&mut |progress| progress.finished.push(item.path.clone()));
            }
        }
        Ok(())
    }

    /// One selected object (FAR's `process_item`).
    fn item(&mut self, item: &Item) -> Step<()> {
        self.step(&item.path)?;
        if item.entry.is_parent || item.path.parent().is_none() {
            return Ok(());
        }
        let provider = match vfs::provider_for(&item.path) {
            Ok(provider) => provider,
            Err(error) => {
                self.fail(&item.path, &error);
                return Ok(());
            }
        };
        let provider = provider.as_ref();
        // The panel listed it a while ago (the task may have waited in the queue): what it is now, so
        // a folder replaced by a link meanwhile is not walked through.
        let entry = match provider.stat(&item.path) {
            Ok(entry) => entry,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            // A folder not read again is not walked from what the panel listed (review R5): a plugin's
            // entry may not tell a junction, whose target's contents the walk would delete.
            Err(error) if item.entry.is_dir => {
                self.fail(&item.path, &error);
                return Ok(());
            }
            Err(_) => item.entry.clone(),
        };
        if !entry.is_dir {
            self.file(provider, &item.path, &entry)?;
        } else if entry.points_elsewhere() {
            // A link to a folder: the link only, wherever it goes (FAR: no question for a symlink).
            self.link(provider, &item.path)?;
        } else {
            self.top_folder(provider, &item.path)?;
        }
        Ok(())
    }

    /// A link to a folder, removed as a link.
    fn link(&mut self, provider: &dyn Provider, path: &VfsPath) -> Step<bool> {
        match self.mode {
            Mode::Trash => self.trash_or_remove(provider, path, true, Removal::EmptyDir),
            Mode::Remove => self.remove(provider, path, Removal::EmptyDir, true),
        }
    }

    /// A selected folder: FAR's question when it is not empty, then the bin or the walk.
    fn top_folder(&mut self, provider: &dyn Provider, path: &VfsPath) -> Step<()> {
        let contents = match self.mode {
            Mode::Trash => None,
            Mode::Remove => match self.contents(provider, path)? {
                Some(contents) => Some(contents),
                None => return Ok(()),
            },
        };
        let not_empty = match &contents {
            Some(contents) => !contents.is_empty(),
            None => has_entries(provider, path),
        };
        if not_empty && !self.confirm_folder(path)? {
            return Ok(());
        }
        match contents {
            Some(contents) => {
                self.folder(provider, path, contents)?;
            }
            None => {
                if let Trashed::RemoveInstead = self.trash(provider, path, true)? {
                    self.remove_instead(provider, path)?;
                }
            }
        }
        Ok(())
    }

    /// A folder the Recycle Bin did not take, deleted for good (FAR's `CannotRecycleTryRemove`): it
    /// was asked about already; what is inside is deleted past the bin too.
    fn remove_instead(&mut self, provider: &dyn Provider, path: &VfsPath) -> Step<bool> {
        self.mode = Mode::Remove;
        let result = match self.contents(provider, path) {
            Ok(Some(contents)) => self.folder(provider, path, contents),
            Ok(None) => Ok(false),
            Err(cancelled) => Err(cancelled),
        };
        self.mode = Mode::Trash;
        result
    }

    /// A folder past the Recycle Bin: what is inside, depth first, then the folder. A folder that keeps
    /// something (the user skipped it, or it failed and was reported) stays without one more error
    /// (FAR then reports "Cannot delete the folder" too). True when it is gone.
    fn folder(
        &mut self,
        provider: &dyn Provider,
        path: &VfsPath,
        contents: Vec<Entry>,
    ) -> Step<bool> {
        let mut emptied = true;
        for entry in contents {
            let inner = path.join(&entry.name);
            self.step(&inner)?;
            emptied &= self.inner(provider, &inner, &entry)?;
        }
        if !emptied {
            return Ok(false);
        }
        self.remove(provider, path, Removal::EmptyDir, true)
    }

    /// One object inside a folder being deleted past the Recycle Bin; true when it is gone.
    fn inner(&mut self, provider: &dyn Provider, path: &VfsPath, entry: &Entry) -> Step<bool> {
        if !entry.is_dir {
            return self.file(provider, path, entry);
        }
        // Looked at again just before it is walked: a folder made a link since it was listed is removed
        // as the link it is now, never walked into (R7 review C8).
        let now = provider.stat(path).ok();
        if entry.points_elsewhere() || now.as_ref().is_some_and(Entry::points_elsewhere) {
            return self.remove(provider, path, Removal::EmptyDir, true);
        }
        let Some(inside) = self.contents(provider, path)? else {
            return Ok(false);
        };
        if !inside.is_empty() && !self.confirm_folder(path)? {
            return Ok(false);
        }
        self.folder(provider, path, inside)
    }

    /// A file or a link to a file, after FAR's read-only question; true when it is gone.
    fn file(&mut self, provider: &dyn Provider, path: &VfsPath, entry: &Entry) -> Step<bool> {
        if entry.attributes.is_read_only() && !self.confirm_read_only(path)? {
            return Ok(false);
        }
        match self.mode {
            Mode::Trash => self.trash_or_remove(provider, path, false, Removal::File),
            Mode::Remove => self.remove(provider, path, Removal::File, false),
        }
    }

    /// To the Recycle Bin; on "Try to delete permanently?" Delete, removed as `instead`.
    fn trash_or_remove(
        &mut self,
        provider: &dyn Provider,
        path: &VfsPath,
        folder: bool,
        instead: Removal,
    ) -> Step<bool> {
        match self.trash(provider, path, folder)? {
            Trashed::RemoveInstead => self.remove(provider, path, instead, folder),
            Trashed::Done => Ok(true),
            Trashed::Skipped => Ok(false),
        }
    }

    /// FAR's `RemoveToRecycleBin` with its question.
    fn trash(&mut self, provider: &dyn Provider, path: &VfsPath, folder: bool) -> Step<Trashed> {
        let error = match self.removal(provider, path, Removal::Trash)? {
            Ok(()) => {
                self.count_done();
                return Ok(Trashed::Done);
            }
            Err(error) => error,
        };
        if self.skip_errors {
            self.fail(path, &error);
            return Ok(Trashed::Skipped);
        }
        let reply = self.host.ask(Question::CannotTrash {
            path: path.clone(),
            folder,
            message: error.to_string(),
        });
        match reply.answer {
            Answer::Delete => Ok(Trashed::RemoveInstead),
            Answer::Skip => {
                self.skip_errors |= reply.for_all;
                self.fail(path, &error);
                Ok(Trashed::Skipped)
            }
            Answer::Retry | Answer::Elevate | Answer::Cancel => Err(Cancelled),
        }
    }

    /// `provider`'s removal; when the rights were missing and the user agrees (ADR-0007), the helper's.
    /// Esc stops the operation.
    fn removal(
        &mut self,
        provider: &dyn Provider,
        path: &VfsPath,
        how: Removal,
    ) -> Step<io::Result<()>> {
        let result = provider.remove(path, how);
        let Err(error) = &result else {
            return Ok(result);
        };
        let Some((request, verb)) = elevated_removal(path, how) else {
            return Ok(result);
        };
        if !platform::elevation::needs_rights(error) || !self.host.elevate_available() {
            return Ok(result);
        }
        let host = &mut self.host;
        let decision = self.approval.decide(|| {
            let reply = host.ask(Question::Elevate {
                path: path.clone(),
                verb,
            });
            let decision = match reply.answer {
                Answer::Elevate => Decision::Elevate,
                Answer::Skip => Decision::Refuse,
                _ => Decision::Cancel,
            };
            (decision, reply.for_all)
        });
        match decision {
            Decision::Elevate => match self.host.run_elevated(&request) {
                Err(error) if is_not_started(&error) => {
                    self.approval.give_up();
                    Ok(result)
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => Err(Cancelled),
                done => Ok(done),
            },
            Decision::Refuse => Ok(result),
            Decision::Cancel => Err(Cancelled),
        }
    }

    /// One removal, with FAR's Retry / Skip / Skip all / Cancel on failure; true when the object is
    /// gone. An object already gone is what the user wanted: no error (FAR reports it).
    fn remove(
        &mut self,
        provider: &dyn Provider,
        path: &VfsPath,
        how: Removal,
        folder: bool,
    ) -> Step<bool> {
        loop {
            match self.removal(provider, path, how)? {
                Ok(()) => {
                    self.count_done();
                    return Ok(true);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
                Err(error) => {
                    if !self.retry(path, folder, &error)? {
                        return Ok(false);
                    }
                }
            }
        }
    }

    /// The entries of a folder to walk; `None` when it cannot be read and the user skips it.
    fn contents(&mut self, provider: &dyn Provider, path: &VfsPath) -> Step<Option<Vec<Entry>>> {
        let stop = self.host.stop_flag();
        loop {
            match list(provider, path, &stop) {
                Ok(entries) => return Ok(Some(entries)),
                // The task's cancel stopped the listing: the cancel, not a failure ("Skip all" or not).
                Err(error)
                    if error.kind() == io::ErrorKind::Interrupted
                        && stop.load(std::sync::atomic::Ordering::Relaxed) =>
                {
                    return Err(Cancelled);
                }
                Err(error) => {
                    if !self.retry(path, true, &error)? {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// FAR's error question: true to try again.
    fn retry(&mut self, path: &VfsPath, folder: bool, error: &io::Error) -> Step<bool> {
        if self.skip_errors {
            self.fail(path, error);
            return Ok(false);
        }
        let reply = self.host.ask(Question::Error {
            path: path.clone(),
            folder,
            message: error.to_string(),
            kind: error.kind(),
            held: crate::jobs::held::held_lines(&[path], error),
        });
        match reply.answer {
            Answer::Retry => Ok(true),
            Answer::Skip => {
                self.skip_errors |= reply.for_all;
                self.fail(path, error);
                Ok(false)
            }
            Answer::Delete | Answer::Elevate | Answer::Cancel => Err(Cancelled),
        }
    }

    /// FAR's "The following folder will be deleted": true to delete it.
    fn confirm_folder(&mut self, path: &VfsPath) -> Step<bool> {
        if self.delete_folders {
            return Ok(true);
        }
        let reply = self.host.ask(Question::NotEmpty {
            path: path.clone(),
            mode: self.mode,
        });
        match reply.answer {
            Answer::Delete => {
                self.delete_folders |= reply.for_all;
                Ok(true)
            }
            Answer::Skip => Ok(false),
            Answer::Retry | Answer::Elevate | Answer::Cancel => Err(Cancelled),
        }
    }

    /// FAR's `ConfirmDeleteReadOnlyFile`: true to delete it.
    fn confirm_read_only(&mut self, path: &VfsPath) -> Step<bool> {
        if let Some(delete) = self.read_only {
            return Ok(delete);
        }
        let reply = self.host.ask(Question::ReadOnly { path: path.clone() });
        let delete = match reply.answer {
            Answer::Delete => true,
            Answer::Skip => false,
            Answer::Retry | Answer::Elevate | Answer::Cancel => return Err(Cancelled),
        };
        if reply.for_all {
            self.read_only = Some(delete);
        }
        Ok(delete)
    }

    /// The next object: the user may have cancelled or paused; the progress names it.
    fn step(&mut self, path: &VfsPath) -> Step<()> {
        if self.host.cancelled() {
            return Err(Cancelled);
        }
        let current = path.to_string();
        self.host
            .progress(&mut |progress: &mut Progress| progress.current.clone_from(&current));
        Ok(())
    }

    fn count_done(&mut self) {
        self.host.progress(&mut |progress| progress.done += 1);
    }

    fn fail(&mut self, path: &VfsPath, error: &io::Error) {
        self.failures.push(format!("{path}: {error}"));
    }
}

/// A folder's entries, as its provider lists them; stops (`Interrupted`) once `stop` is set.
pub(super) fn list(
    provider: &dyn Provider,
    path: &VfsPath,
    stop: &AtomicBool,
) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    provider.list(path, stop, &mut |entry| {
        entries.push(entry);
    })?;
    Ok(entries)
}

/// Whether a folder has anything inside (FAR's `is_not_empty_directory`); one that cannot be read
/// counts as not empty, so the question is asked.
fn has_entries(provider: &dyn Provider, path: &VfsPath) -> bool {
    let stop = AtomicBool::new(false);
    let mut any = false;
    let result = provider.list(path, &stop, &mut |_| {
        any = true;
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    any || result.is_err()
}
