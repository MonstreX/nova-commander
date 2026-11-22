//! Extraction as arclite does it (`Archive::extract`, extract.cpp): the destination's folders made, the
//! selected folders made below it, then their files in the archive's order, each asked about when one is
//! there (overwrite, skip, rename, append), an encrypted one's password asked once for the archive; its
//! attributes and times set as it closes; the folders' last. Each failure is asked about (Retry, Ignore,
//! Ignore All, Cancel) unless errors are ignored, and kept for the log. What asks the user is [`Asks`]
//! (the plugin's windows, `asking`). 7-Zip's library gives the files in one pass (`extract_library`).

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nc_plugin_sdk::system::{SetTimes, set_attributes, set_times};

use crate::extract_options::{ExtractOptions, Overwrite};
use crate::names::{auto_rename, corrected};
use crate::opened::Opened;

/// The attributes a file on a disk takes (`SetFileAttributesW`'s): read-only, hidden, system, archive,
/// temporary, offline, not indexed.
const SETTABLE: u32 = 0x1 | 0x2 | 0x4 | 0x20 | 0x100 | 0x1000 | 0x2000;

/// A failure as arclite tells it (its `Error`): the system's text when the system failed, what it is
/// about (its `objects`: an archive tested), its lines, the warnings with them (a test's open warnings).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fault {
    pub system: Option<String>,
    pub objects: Vec<String>,
    pub messages: Vec<String>,
    pub warnings: Vec<String>,
}

impl Fault {
    /// The system's failure on `path`.
    pub(crate) fn system(error: &io::Error, path: &Path) -> Self {
        Self {
            system: Some(system_text(error)),
            messages: vec![path.display().to_string()],
            ..Self::default()
        }
    }

    /// arclite's own words (`E_MESSAGE`), nothing named before them.
    pub(crate) fn message(messages: Vec<String>) -> Self {
        Self {
            messages,
            ..Self::default()
        }
    }
}

/// The system's words for `error`, without the code Rust adds.
fn system_text(error: &io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

/// The user's answer to a failure (arclite's `retry_or_ignore_error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Retry,
    Ignore,
    IgnoreAll,
    Cancel,
}

/// A file's size and time as the overwrite question shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// The user stopped the extraction (arclite's `E_ABORT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abort;

/// What the extraction asks the user and tells.
pub trait Asks {
    /// The existing `path`: the answer and whether it is for every file; none for Cancel.
    fn overwrite(
        &mut self,
        path: &Path,
        source: &FileInfo,
        destination: &FileInfo,
    ) -> Option<(Overwrite, bool)>;
    /// The archive's password; none for Cancel.
    fn password(&mut self) -> Option<String>;
    /// A failure: what to do (`can_retry` false: no Retry).
    fn fault(&mut self, fault: &Fault, can_retry: bool) -> Choice;
    /// The file at hand and the bytes written of all.
    fn progress(&mut self, file: &Path, done: u64, total: u64);
    /// Whether the user stopped it (Esc, and the question after it answered so).
    fn stopped(&mut self) -> bool;
}

/// One extraction going on.
struct Extraction<'a> {
    opened: &'a mut Opened,
    asks: &'a mut dyn Asks,
    /// The answer for every existing file, once given.
    overwrite: Overwrite,
    ignore_errors: bool,
    log: Vec<Fault>,
    done: u64,
    total: u64,
    /// The files the user skipped (arclite's `skipped_indices`).
    skipped: BTreeSet<usize>,
}

/// What an attempt that may fail came to.
enum Tried<T> {
    Done(T),
    /// Failed, the failure let be (logged).
    Ignored,
}

impl Extraction<'_> {
    /// `act` tried until done, or the failure ignored (arclite's `RETRY_OR_IGNORE` / `IGNORE_END`).
    fn attempt<T>(
        &mut self,
        can_retry: bool,
        mut act: impl FnMut() -> Result<T, Fault>,
    ) -> Result<Tried<T>, Abort> {
        loop {
            let fault = match act() {
                Ok(value) => return Ok(Tried::Done(value)),
                Err(fault) => fault,
            };
            if !self.ignore_errors {
                match self.asks.fault(&fault, can_retry) {
                    Choice::Retry if can_retry => continue,
                    Choice::Ignore => {}
                    Choice::IgnoreAll => self.ignore_errors = true,
                    Choice::Retry | Choice::Cancel => return Err(Abort),
                }
            }
            self.log.push(fault);
            return Ok(Tried::Ignored);
        }
    }

    /// arclite's `PrepareExtract`: the folders among `nodes` made under `parent` with what is in them,
    /// their files gathered (their items, their paths).
    fn prepare(
        &mut self,
        nodes: &[usize],
        parent: &Path,
        files: &mut Vec<(usize, PathBuf)>,
    ) -> Result<(), Abort> {
        for &node in nodes {
            let (name, is_dir, item) = {
                let node = &self.opened.tree.nodes[node];
                (node.name.clone(), node.is_dir, node.item)
            };
            let path = parent.join(corrected(&name, true));
            if !is_dir {
                files.extend(item.map(|item| (item, path)));
                continue;
            }
            self.attempt(true, || match std::fs::create_dir(&path) {
                Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                    Err(Fault::system(&error, &path))
                }
                _ => Ok(()),
            })?;
            let children = self.opened.tree.nodes[node].children.clone();
            self.prepare(&children, &path, files)?;
        }
        Ok(())
    }

    /// What to do with `path` of `item`: `Ask` for a new file, none to skip it.
    fn action(&mut self, item: usize, path: &Path) -> Result<Option<Overwrite>, Abort> {
        let Ok(there) = std::fs::symlink_metadata(path) else {
            return Ok(Some(Overwrite::Ask));
        };
        let mut action = self.overwrite;
        if action == Overwrite::Ask {
            let item = &self.opened.items[item];
            let source = FileInfo {
                is_dir: item.is_dir,
                size: item.size,
                modified: item.modified,
            };
            let destination = FileInfo {
                is_dir: there.is_dir(),
                size: there.len(),
                modified: there.modified().ok(),
            };
            let (answer, all) = (self.asks)
                .overwrite(path, &source, &destination)
                .ok_or(Abort)?;
            action = answer;
            // arclite's `strict_case`: the archive's case of the name taken.
            if action == Overwrite::Overwrite && !same_case(path) {
                action = Overwrite::OverwriteCase;
            }
            if all {
                self.overwrite = action;
            }
        }
        Ok((action != Overwrite::Skip).then_some(action))
    }

    /// The file's attributes and times as the archive keeps them: true when set (or let be), false
    /// after a failure let be (the file goes).
    fn finish(&mut self, item: usize, target: &Path) -> Result<bool, Abort> {
        let item = self.opened.items[item].clone();
        let times = SetTimes {
            created: item.created,
            accessed: item.accessed,
            modified: item.modified,
        };
        let set = self.attempt(false, || {
            set_times(target, times).map_err(|error| Fault::system(&error, target))?;
            set_attributes(target, item.attributes & SETTABLE)
                .map_err(|error| Fault::system(&error, target))
        })?;
        Ok(matches!(set, Tried::Done(())))
    }

    /// arclite's `SetDirAttr`: the folders among `nodes` under `parent`, what is in them first, their
    /// attributes and times (a folder only implied the archive file's).
    fn set_folders(&mut self, nodes: &[usize], parent: &Path) -> Result<(), Abort> {
        for &node in nodes {
            if !self.opened.tree.nodes[node].is_dir {
                continue;
            }
            let path = parent.join(corrected(&self.opened.tree.nodes[node].name, true));
            let children = self.opened.tree.nodes[node].children.clone();
            self.set_folders(&children, &path)?;
            let entry = crate::panel::entry(self.opened, node);
            let times = SetTimes {
                created: entry.created,
                accessed: entry.accessed,
                modified: entry.modified,
            };
            self.attempt(true, || {
                let _ = set_attributes(&path, 0);
                set_times(&path, times).map_err(|error| Fault::system(&error, &path))?;
                set_attributes(&path, entry.attributes & SETTABLE)
                    .map_err(|error| Fault::system(&error, &path))
            })?;
        }
        Ok(())
    }
}

/// The file `target` opened for its data as `action` says: added to (`Append`), made anew in the
/// archive's case (`OverwriteCase`: the old one deleted first), else made or emptied (deleted and made
/// again when that fails, as arclite does).
fn create(target: &Path, action: Overwrite) -> Result<File, Fault> {
    let fault = |error: io::Error| Fault::system(&error, target);
    if action == Overwrite::Append {
        return OpenOptions::new().append(true).open(target).map_err(fault);
    }
    if action != Overwrite::OverwriteCase
        && let Ok(file) = File::create(target)
    {
        return Ok(file);
    }
    let _ = std::fs::remove_file(target);
    File::create(target).map_err(fault)
}

/// Whether the file at `path` has the name's very case (its folder's listing tells).
fn same_case(path: &Path) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return true;
    };
    let Ok(listed) = std::fs::read_dir(parent) else {
        return true;
    };
    let found = listed.flatten().find(|entry| {
        entry.file_name().to_string_lossy().to_lowercase() == name.to_string_lossy().to_lowercase()
    });
    found.is_none_or(|entry| entry.file_name() == name)
}

/// The nodes `nodes` of the folder `source` written under `options.dst_dir` as arclite writes them: the
/// failures let be (the log), or the user's stop.
pub fn extract(
    opened: &mut Opened,
    nodes: &[usize],
    options: &ExtractOptions,
    asks: &mut dyn Asks,
) -> Result<Vec<Fault>, Abort> {
    extract_noting(opened, nodes, options, asks).map(|(log, _)| log)
}

/// [`extract`], with the files the user skipped (arclite's `skipped_indices`, for F6's deleting).
pub fn extract_noting(
    opened: &mut Opened,
    nodes: &[usize],
    options: &ExtractOptions,
    asks: &mut dyn Asks,
) -> Result<(Vec<Fault>, BTreeSet<usize>), Abort> {
    // arclite's `prepare_dst_dir`: made where it can be, quietly.
    let _ = std::fs::create_dir_all(&options.dst_dir);
    let mut extraction = Extraction {
        opened,
        asks,
        overwrite: options.overwrite,
        ignore_errors: options.ignore_errors,
        log: Vec::new(),
        done: 0,
        total: 0,
        skipped: BTreeSet::new(),
    };
    let mut files = Vec::new();
    extraction.prepare(nodes, &options.dst_dir, &mut files)?;
    files.sort_by_key(|(item, _)| *item);
    extraction.total = (files.iter())
        .map(|(item, _)| extraction.opened.items[*item].size)
        .sum();
    library::extract_files(&mut extraction, &files, &options.dst_dir)?;
    extraction.set_folders(nodes, &options.dst_dir)?;
    Ok((extraction.log, extraction.skipped))
}

#[path = "extract_library.rs"]
pub(crate) mod library;

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
