//! Adding a disk's files to an archive as arclite does (`Archive::update`, update.cpp): `PrepareUpdate`
//! walks the sources under the archive's folder, each file the archive has (its folder, a file to a
//! file, a folder to a folder, whatever the case) asked about as the answer says (Ask, Overwrite, Skip),
//! a folder there gone into, one only implied written; the archive is then written anew beside itself
//! (`update_write`). Failures are asked about (Retry, Ignore, Ignore All, Cancel) unless errors are
//! ignored, and kept for the log. What asks the user is [`UpdateAsks`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::extract::{Abort, Choice, Fault, FileInfo};
use crate::extract_options::Overwrite;
use crate::item::Item;
use crate::opened::Opened;
use crate::tree::Tree;

/// A file or folder of a disk going into the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    /// Its path in the archive.
    pub path: Vec<String>,
    pub source: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Its Windows attributes.
    pub attributes: u32,
}

/// What an update writes: the files and folders added in the order walked, the archive's items they
/// take the place of (by item, the added one), and whether any was skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub added: Vec<Added>,
    pub replaced: BTreeMap<usize, usize>,
    pub skipped: bool,
    /// The archive's items left out of the new one (arclite's `delete_files`).
    pub removed: std::collections::BTreeSet<usize>,
}

/// Where the writing is (arclite's `ArchiveUpdateProgress`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateAt<'a> {
    pub file: &'a Path,
    pub file_done: u64,
    pub file_total: u64,
    /// The bytes of all the files read so far, and of all.
    pub done: u64,
    pub total: u64,
    /// The bytes written into the new archive.
    pub written: u64,
}

/// What an update asks the user and tells.
pub trait UpdateAsks {
    /// The archive's `path` there already: the answer and whether it is for every file; none for
    /// Cancel (arclite's "Overwrite file" without Rename and Append).
    fn overwrite(
        &mut self,
        path: &Path,
        source: &FileInfo,
        destination: &FileInfo,
    ) -> Option<(Overwrite, bool)>;
    /// A failure: what to do (`can_retry` false: no Retry).
    fn fault(&mut self, fault: &Fault, can_retry: bool) -> Choice;
    /// Where the writing is.
    fn progress(&mut self, at: &UpdateAt<'_>);
    /// Whether the user stopped it (Esc, and the question after it answered so).
    fn stopped(&mut self) -> bool;
}

/// Failures asked about or let be, and the ones let be.
pub struct Faults<'a> {
    pub asks: &'a mut dyn UpdateAsks,
    pub ignore_errors: bool,
    pub log: Vec<Fault>,
}

impl Faults<'_> {
    /// `act` tried until done, or the failure ignored (arclite's `RETRY_OR_IGNORE`): `None` then.
    pub fn attempt<T>(
        &mut self,
        mut act: impl FnMut() -> Result<T, Fault>,
    ) -> Result<Option<T>, Abort> {
        loop {
            let fault = match act() {
                Ok(value) => return Ok(Some(value)),
                Err(fault) => fault,
            };
            if !self.ignore_errors {
                match self.asks.fault(&fault, true) {
                    Choice::Retry => continue,
                    Choice::Ignore => {}
                    Choice::IgnoreAll => self.ignore_errors = true,
                    Choice::Cancel => return Err(Abort),
                }
            }
            self.log.push(fault);
            return Ok(None);
        }
    }
}

/// The folder the sources were taken from and each one's path under it (arclite's `put_files`: the
/// folder given when every source is in it, else the longest folder they share).
pub fn under_folder(sources: &[PathBuf], folder: Option<&Path>) -> (PathBuf, Vec<PathBuf>) {
    let inside = |base: &Path| (sources.iter()).all(|source| relative(source, base).is_some());
    let base = match folder {
        Some(folder) if inside(folder) => folder.to_path_buf(),
        _ => shared_folder(sources),
    };
    let names = (sources.iter())
        .map(|source| relative(source, &base).unwrap_or_else(|| source.clone()))
        .collect();
    (base, names)
}

/// `path` under `base` (its parts whatever their case), never `base` itself.
fn relative(path: &Path, base: &Path) -> Option<PathBuf> {
    let lower = |part: std::path::Component<'_>| part.as_os_str().to_string_lossy().to_lowercase();
    let mut parts = path.components();
    for part in base.components() {
        if lower(parts.next()?) != lower(part) {
            return None;
        }
    }
    let rest: PathBuf = parts.collect();
    (!rest.as_os_str().is_empty()).then_some(rest)
}

/// The longest folder every source is in.
fn shared_folder(sources: &[PathBuf]) -> PathBuf {
    let mut shared = sources
        .first()
        .and_then(|first| first.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    while !sources
        .iter()
        .all(|source| relative(source, &shared).is_some())
    {
        match shared.parent() {
            Some(parent) => shared = parent.to_path_buf(),
            None => return PathBuf::new(),
        }
    }
    shared
}

/// arclite's `PrepareUpdate`: what of the sources (`names` under `src_dir`) goes into the archive's
/// folder `dst` (`/`-separated), as `overwrite` answers for a file there. A source named with its
/// folders is looked for in the archive's folder of that path (arclite looks in `dst`, and writes a
/// second item of the name).
pub fn prepare(
    opened: &Opened,
    dst: &str,
    sources: (&Path, &[PathBuf]),
    overwrite: Overwrite,
    faults: &mut Faults<'_>,
) -> Result<Plan, Abort> {
    let existing = Existing {
        tree: &opened.tree,
        items: &opened.items,
    };
    prepare_in(existing, dst, sources, overwrite, faults)
}

/// What an archive has already, as the walk looks at it: nothing for a new one.
#[derive(Clone, Copy)]
pub struct Existing<'a> {
    pub tree: &'a Tree,
    pub items: &'a [Item],
}

/// [`prepare`] into what `existing` has (a new archive: an empty tree, no items).
pub fn prepare_in(
    existing: Existing<'_>,
    dst: &str,
    (src_dir, names): (&Path, &[PathBuf]),
    overwrite: Overwrite,
    faults: &mut Faults<'_>,
) -> Result<Plan, Abort> {
    let mut walk = Walk {
        existing,
        src_dir,
        overwrite,
        plan: Plan::default(),
    };
    let base: Vec<String> = dst
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    for name in names {
        let path = src_dir.join(name);
        let mut parts = base.clone();
        let sub = name.parent().into_iter().flat_map(|parent| {
            parent
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
        });
        parts.extend(sub);
        let Some(meta) = faults.attempt(|| stat(&path))? else {
            continue;
        };
        let folder = existing.tree.find_dir(&parts.join("/"));
        walk.source(&path, &meta, (&parts, folder), faults)?;
    }
    Ok(walk.plan)
}

/// A source as the walk takes it.
struct Meta {
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
    attributes: u32,
}

/// The source's own size, times and attributes, not a link's target's (arclite's `FindData`).
fn stat(path: &Path) -> Result<Meta, Fault> {
    let meta = std::fs::symlink_metadata(path).map_err(|error| Fault::system(&error, path))?;
    Ok(Meta {
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().ok(),
        created: meta.created().ok(),
        accessed: meta.accessed().ok(),
        attributes: attributes_of(&meta),
    })
}

#[cfg(windows)]
fn attributes_of(meta: &std::fs::Metadata) -> u32 {
    std::os::windows::fs::MetadataExt::file_attributes(meta)
}

#[cfg(not(windows))]
fn attributes_of(meta: &std::fs::Metadata) -> u32 {
    if meta.is_dir() { 0x10 } else { 0x20 }
}

/// The walk of `PrepareUpdate`.
struct Walk<'a> {
    existing: Existing<'a>,
    /// The sources' folder: the question names a file from it (arclite's `sub_dir` and name).
    src_dir: &'a Path,
    overwrite: Overwrite,
    plan: Plan,
}

impl Walk<'_> {
    /// arclite's `process_file`, then into a folder (`process_file_enum`): `parts` its archive folder,
    /// `folder` that folder's node when the archive has it.
    fn source(
        &mut self,
        path: &Path,
        meta: &Meta,
        (parts, folder): (&[String], Option<usize>),
        faults: &mut Faults<'_>,
    ) -> Result<(), Abort> {
        let name =
            (path.file_name()).map_or_else(String::new, |name| name.to_string_lossy().into());
        let mut at = parts.to_vec();
        at.push(name.clone());
        let tree = self.existing.tree;
        let there = folder.and_then(|folder| {
            (tree.nodes[folder].children.iter().copied()).find(|&child| {
                let node = &tree.nodes[child];
                node.is_dir == meta.is_dir && node.name.to_lowercase() == name.to_lowercase()
            })
        });
        let added = Added {
            path: at.clone(),
            source: path.to_path_buf(),
            is_dir: meta.is_dir,
            size: meta.size,
            modified: meta.modified,
            created: meta.created,
            accessed: meta.accessed,
            attributes: meta.attributes,
        };
        match there.map(|node| (node, tree.nodes[node].item)) {
            // A folder only implied: written now (arclite's "fake index").
            None | Some((_, None)) => self.plan.added.push(added),
            Some((_, Some(_))) if meta.is_dir => {}
            Some((_, Some(item))) => {
                if !self.replace(item, &added, faults)? {
                    self.plan.skipped = true;
                    return Ok(());
                }
            }
        }
        if meta.is_dir {
            self.folder(path, (&at, there), faults)?;
        }
        Ok(())
    }

    /// The archive's file `item` taken by `added`, as the answer says (arclite's overwrite question).
    fn replace(
        &mut self,
        item: usize,
        added: &Added,
        faults: &mut Faults<'_>,
    ) -> Result<bool, Abort> {
        let answer = if self.overwrite == Overwrite::Ask {
            let existing = &self.existing.items[item];
            let source = FileInfo {
                is_dir: false,
                size: added.size,
                modified: added.modified,
            };
            let destination = FileInfo {
                is_dir: false,
                size: existing.size,
                modified: existing.modified,
            };
            let shown = (added.source.strip_prefix(self.src_dir))
                .map_or_else(|_| added.source.clone(), Path::to_path_buf);
            let (answer, all) = (faults.asks)
                .overwrite(&shown, &source, &destination)
                .ok_or(Abort)?;
            if all {
                self.overwrite = answer;
            }
            answer
        } else {
            self.overwrite
        };
        if answer == Overwrite::Skip {
            return Ok(false);
        }
        self.plan.replaced.insert(item, self.plan.added.len());
        self.plan.added.push(added.clone());
        Ok(true)
    }

    /// What is in the folder `path` walked (arclite's `DirList`), a failure to list it asked about.
    fn folder(
        &mut self,
        path: &Path,
        (parts, node): (&[String], Option<usize>),
        faults: &mut Faults<'_>,
    ) -> Result<(), Abort> {
        let listed = faults.attempt(|| {
            let entries = std::fs::read_dir(path).map_err(|error| Fault::system(&error, path))?;
            let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
            paths.sort();
            Ok(paths)
        })?;
        for child in listed.unwrap_or_default() {
            let Some(meta) = faults.attempt(|| stat(&child))? else {
                continue;
            };
            self.source(&child, &meta, (parts, node), faults)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
