//! The worker's side of "Set": each object's changes, a question on each failure, the walk through the
//! selected folders with "Process subfolders".

use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::{Change, Host, Question, Reply, Report, Request, changes, failure, verb};
use crate::jobs::elevation::{Approval, Decision};
use crate::jobs::folder_size::identity;
use crate::platform::files::{self, FileProperties, FileTimes};
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::local::elevated_session::is_not_started;
use crate::vfs::{Entry, VfsPath, provider_for};

/// The user stopped the task (Esc, or Cancel in a question).
struct Cancelled;

/// Makes the changes `request` asks for; the report lists each change left out.
pub fn run(request: &Request, host: &mut dyn Host) -> Report {
    let mut engine = Engine {
        request,
        host,
        skip_all: false,
        approval: Approval::default(),
        failures: Vec::new(),
    };
    let cancelled = engine.items().is_err();
    Report {
        failures: engine.failures,
        cancelled,
        ..Report::default()
    }
}

/// A folder's objects, as its provider lists them.
fn children(folder: &VfsPath, stop: &AtomicBool) -> io::Result<Vec<Entry>> {
    let provider = provider_for(folder)?;
    let mut entries = Vec::new();
    provider.list(folder, stop, &mut |entry| {
        if !entry.is_parent {
            entries.push(entry);
        }
    })?;
    Ok(entries)
}

/// What the listing tells of an object; its owner is not read (FAR's `BUGBUG, should we read the
/// owner?`): a typed owner is given to every object.
fn properties_of(entry: &Entry) -> FileProperties {
    FileProperties {
        attributes: entry.attributes,
        times: FileTimes {
            created: entry.created,
            accessed: entry.accessed,
            modified: entry.modified,
            changed: entry.changed,
        },
        owner: None,
        link: None,
        names: Vec::new(),
        name_count: 1,
    }
}

struct Engine<'a> {
    request: &'a Request,
    host: &'a mut dyn Host,
    /// "Skip all": later failures are left out without asking.
    skip_all: bool,
    /// The answers to "Access denied" for the rest of the task.
    approval: Approval,
    failures: Vec<String>,
}

impl Engine<'_> {
    fn items(&mut self) -> Result<(), Cancelled> {
        let request = self.request;
        for item in &request.items {
            self.selected(item)?;
            let top = VfsPath::local(item);
            self.host
                .progress(&mut |progress| progress.finished.push(top.clone()));
        }
        Ok(())
    }

    /// A selected object, read again as it is now (the dialog may have been open a while), then with
    /// "Process subfolders" what a folder holds. FAR sets the owner of every object then, without
    /// comparing.
    fn selected(&mut self, path: &Path) -> Result<(), Cancelled> {
        // Before the attributes are read: a link that cannot be read can still be given a target
        // (FAR: "can be used even if the attributes are invalid").
        if let Some(target) = &self.request.link_target {
            self.make(path, &Change::LinkTarget(target.clone()))?;
        }
        let mut properties = match files::file_properties(path) {
            Ok(properties) => properties,
            Err(error) => {
                self.fail(&path.display().to_string(), &error);
                return Ok(());
            }
        };
        let walk = self.request.subfolders;
        if walk {
            properties.owner = None;
        }
        self.object(path, &properties, true)?;
        if walk && properties.attributes.is_directory() {
            self.walk(path)?;
        }
        Ok(())
    }

    /// FAR's `ScanTree` through a folder: each object as listed, then what its folders hold; a link to a
    /// folder is followed once per place it leads to.
    fn walk(&mut self, root: &Path) -> Result<(), Cancelled> {
        let root = VfsPath::local(root);
        let mut places: HashSet<(u32, u64)> = identity(&root).into_iter().collect();
        let mut pending = vec![root];
        let stop = self.host.stop_flag();
        while let Some(folder) = pending.pop() {
            let entries = match children(&folder, &stop) {
                Ok(entries) => entries,
                // The task stopped while its folder was listed.
                Err(error)
                    if error.kind() == io::ErrorKind::Interrupted
                        && stop.load(std::sync::atomic::Ordering::Relaxed) =>
                {
                    return Err(Cancelled);
                }
                Err(error) => {
                    self.fail(&folder.to_string(), &error);
                    continue;
                }
            };
            for entry in entries {
                let path = folder.join(&entry.name);
                let Some(local) = path.as_local() else {
                    continue;
                };
                self.object(local, &properties_of(&entry), false)?;
                // Each place once: a link may lead to a folder walked anyway.
                let new_place = match identity(&path) {
                    Some(place) => places.insert(place),
                    None => !entry.points_elsewhere(),
                };
                if entry.is_dir && new_place {
                    pending.push(path);
                }
            }
        }
        Ok(())
    }

    fn fail(&mut self, what: &str, error: &io::Error) {
        self.failures.push(format!("{what}: {error}"));
    }

    /// One object, as FAR's `process_single_file`; `selected`: one the user chose.
    fn object(
        &mut self,
        path: &Path,
        properties: &FileProperties,
        selected: bool,
    ) -> Result<(), Cancelled> {
        if self.host.cancelled() {
            return Err(Cancelled);
        }
        let shown = path.display().to_string();
        self.host
            .progress(&mut |progress| progress.current.clone_from(&shown));
        let request = self.request;
        let planned = changes(properties, &request.wanted, &request.owner, |row, own| {
            request.time(row, own, selected)
        });
        for change in planned {
            self.make(path, &change)?;
        }
        Ok(())
    }

    /// FAR's `retryable_ui_operation`: Retry tries again, Skip leaves this change out.
    fn make(&mut self, path: &Path, change: &Change) -> Result<(), Cancelled> {
        loop {
            let Err(error) = self.attempt(path, change)? else {
                return Ok(());
            };
            let reply = if self.skip_all {
                Reply::Skip
            } else {
                self.host.ask(Question::Failed {
                    failure: failure(change),
                    path: path.to_path_buf(),
                    message: error.to_string(),
                })
            };
            match reply {
                Reply::Retry => {}
                Reply::Cancel => return Err(Cancelled),
                skip => {
                    self.skip_all |= skip == Reply::SkipAll;
                    let failure = failure(change);
                    self.failures
                        .push(format!("{failure} {}: {error}", path.display()));
                    return Ok(());
                }
            }
        }
    }

    /// The change; refused for want of rights, FAR's "Access denied", and with the user's yes the
    /// helper makes it (ADR-0007). Esc there stops the task.
    fn attempt(&mut self, path: &Path, change: &Change) -> Result<io::Result<()>, Cancelled> {
        let result = change.apply(path);
        let Err(error) = &result else {
            return Ok(result);
        };
        if !crate::platform::elevation::needs_rights(error) || !self.host.elevate_available() {
            return Ok(result);
        }
        let host = &mut self.host;
        let decision = self.approval.decide(|| {
            let question = Question::Elevate {
                path: path.to_path_buf(),
                verb: verb(change),
            };
            match host.ask(question) {
                Reply::Elevate { for_all } => (Decision::Elevate, for_all),
                Reply::Refuse { for_all } => (Decision::Refuse, for_all),
                _ => (Decision::Cancel, false),
            }
        });
        let request = Elevated::Set {
            path: path.to_path_buf(),
            setting: change.clone(),
        };
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
}
