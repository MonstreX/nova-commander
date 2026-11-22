//! FAR's file of a disk's folders (`far/treelist.cpp`: `SaveTreeFile`, `ReadTreeFile`, `ReadCache`,
//! `AddTreeName`, `DelTreeName`; `GetCacheTreeName` names it), kept as nodes (ADR-0014): the tree shows
//! from it at once, and what the program makes, moves or deletes goes into it. One file per volume, named
//! by its label, serial number, file system and, for a network one, its share, as FAR's `tree3.cache`
//! files; they are in `TreeCache` next to the settings (owner 2026-11-11), never on the disk itself. A
//! line is a folder's path under the root from its `\` (FAR's lines, in UTF-8 here), then, for a folder
//! whose folders were read, a tab and its last write time then (nanoseconds since 1970). A disk without
//! a file gets none from a change (FAR's `ReadCache`): only the tree's worker makes one.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::delete;
use super::transfer;
use super::tree_nodes::Nodes;
use crate::platform::files::paths::is_share;
use crate::platform::files::{fold_case, is_plain_name, same_place};
use crate::platform::volumes;

/// What joins a line's names (FAR's `\`): no folder's name holds it.
const SEP: char = std::path::MAIN_SEPARATOR;

/// One reading or change of a file at a time: the tree's worker and the operations' share them.
static FILES: Mutex<()> = Mutex::new(());

fn files() -> MutexGuard<'static, ()> {
    FILES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A file that could not be written (FAR's "Cannot save folders tree to file").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsaved {
    pub file: PathBuf,
    pub error: String,
}

/// The folder of the trees' files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeCache {
    dir: PathBuf,
    /// The root paths under it have in place of their disk's (tests: never a real disk).
    root: Option<PathBuf>,
}

impl TreeCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, root: None }
    }

    /// Trees whose root is `root`, a test's own folder.
    #[cfg(all(test, windows))]
    pub fn under(dir: PathBuf, root: PathBuf) -> Self {
        Self {
            dir,
            root: Some(root),
        }
    }

    /// The folders of `root`'s file (the root not among them), a folder before those in it (tests).
    #[cfg(all(test, windows))]
    pub fn read(&self, root: &Path) -> Option<Vec<PathBuf>> {
        let nodes = self.load(root)?;
        let in_order = nodes.in_order();
        Some(
            in_order
                .iter()
                .skip(1)
                .map(|node| node.path.clone())
                .collect(),
        )
    }

    /// A file of `folders` under `root`, none of them read (tests).
    #[cfg(all(test, windows))]
    pub fn save(&self, root: &Path, folders: &[PathBuf], min: usize) -> Result<(), Unsaved> {
        let mut nodes = Nodes::new(root);
        for folder in folders {
            nodes.insert(folder);
        }
        self.store(&nodes, min)
    }

    /// The root of `path`'s disk (FAR's `extract_root_directory`).
    pub fn root_of(&self, path: &Path) -> PathBuf {
        match &self.root {
            Some(root) if path.starts_with(root) => root.clone(),
            _ => path.ancestors().last().unwrap_or(path).to_path_buf(),
        }
    }

    /// The file of `root`'s volume; none when the volume cannot be read.
    pub fn file_of(&self, root: &Path) -> Option<PathBuf> {
        let volume = volumes::volume_of(root).to_string_lossy().into_owned();
        let facts = volumes::volume_facts(&volume)?;
        let mut name = format!(
            "{}.{:08X}.{}",
            facts.label,
            facts.serial.unwrap_or(0),
            facts.file_system
        );
        let share = volumes::network_name(&volume)
            .or_else(|| is_share(Path::new(&volume)).then(|| volume.clone()));
        if let Some(share) = share {
            name.push('.');
            name.push_str(share.trim_end_matches(std::path::is_separator));
        }
        let name: String = (name.chars())
            .map(|ch| match ch {
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                ch if ch.is_control() => '_',
                ch => ch,
            })
            .collect();
        Some(self.dir.join(format!("{name}.tree")))
    }

    /// FAR's `ReadTreeFile`: the tree the file of `root`'s volume keeps, the times of the folders read
    /// with it; none without a file or with no line in it.
    pub fn load(&self, root: &Path) -> Option<Nodes> {
        let file = self.file_of(root)?;
        let text = {
            let _files = files();
            std::fs::read_to_string(file).ok()?
        };
        let mut nodes = Nodes::new(root);
        let mut any = false;
        for line in lines_of(&text) {
            any = true;
            let (path, time) = line.split_once('\t').unwrap_or((line, ""));
            let names: Vec<&str> = path.split(SEP).filter(|name| !name.is_empty()).collect();
            // A line no folder's names make (a file spoilt or edited by hand) leads nowhere.
            let odd = |name: &&str| !is_plain_name(name);
            if !names.iter().any(odd) {
                nodes.put(&names, time_of(time));
            }
        }
        any.then_some(nodes)
    }

    /// FAR's `SaveTreeFile`: the tree of `nodes`, unless with the root it has fewer than `min` folders.
    /// A file that cannot be written is taken away (FAR's `WriteTree`).
    pub fn store(&self, nodes: &Nodes, min: usize) -> Result<(), Unsaved> {
        if nodes.len() < min {
            return Ok(());
        }
        let root = nodes.root();
        let Some(file) = self.file_of(root) else {
            return Ok(());
        };
        let lines: Vec<String> = (nodes.in_order().into_iter())
            .map(|node| {
                let line = line_of(root, &node.path).unwrap_or_else(|| SEP.to_string());
                match node.read.and_then(nanos_of) {
                    Some(nanos) => format!("{line}\t{nanos}"),
                    None => line,
                }
            })
            .collect();
        let _files = files();
        write(&file, &lines).map_err(|error| {
            let _ = std::fs::remove_file(&file);
            Unsaved {
                file,
                error: error.to_string(),
            }
        })
    }

    /// FAR's `AddTreeName`: `folders` under `root` into its file with the folders above them (a tree
    /// has no branch without its stem), each once whatever its case.
    pub fn add(&self, root: &Path, folders: &[PathBuf]) {
        self.change(root, |lines| {
            let mut known: std::collections::HashSet<String> = lines
                .iter()
                .map(|line| fold_case(path_part(line)).into_owned())
                .collect();
            for folder in folders {
                let mut stem: Vec<String> = (folder.ancestors())
                    .map_while(|above| line_of(root, above))
                    .collect();
                stem.reverse();
                for line in stem {
                    if known.insert(fold_case(&line).into_owned()) {
                        lines.push(line);
                    }
                }
            }
        });
    }

    /// FAR's `DelTreeName` of what is gone: `folder` and the folders under it no longer there leave
    /// the file (of a folder left half deleted, what is still there stays).
    pub fn forget_gone(&self, folder: &Path) {
        let root = self.root_of(folder);
        let Some(own) = line_of(&root, folder) else {
            return;
        };
        let own = fold_case(&own).into_owned();
        self.change(&root, |lines| {
            lines.retain(|line| {
                let line = path_part(line);
                let folded = fold_case(line);
                let under = folded == own
                    || (folded.starts_with(&own) && folded[own.len()..].starts_with(SEP));
                !under || folder_of(&root, line).is_some_and(|path| path.is_dir())
            });
        });
    }

    /// Whether `root`'s disk has a file: a change of one without needs no walk.
    fn has_file(&self, root: &Path) -> bool {
        self.file_of(root).is_some_and(|file| file.is_file())
    }

    /// F7's folder (and those made above it, up to `top`, the first that was not there: see
    /// [`first_missing`]) into the file.
    pub fn made(&self, top: &Path) {
        let top = lexical(top);
        let root = self.root_of(&top);
        if !self.has_file(&root) {
            return;
        }
        let mut made = vec![top.clone()];
        made.extend(super::tree::folders_under(&top));
        self.add(&root, &made);
    }

    /// A folder renamed in its case only (`dir` to `Dir`): its lines and those under it take the new
    /// case.
    fn renamed_case(&self, from: &Path, to: &Path) {
        let root = self.root_of(from);
        let (Some(old), Some(new)) = (line_of(&root, from), line_of(&root, to)) else {
            return;
        };
        let own = fold_case(&old).into_owned();
        let below = format!("{own}{SEP}");
        let skip = old.chars().count();
        self.change(&root, |lines| {
            for line in lines.iter_mut() {
                let folded = fold_case(path_part(line));
                if folded == own || folded.starts_with(&below) {
                    let rest: String = line.chars().skip(skip).collect();
                    *line = format!("{new}{rest}");
                }
            }
        });
    }

    /// What a copy, move or link is to do to folders, worked out before it starts (its target is
    /// then not made yet): where each source goes.
    pub fn before_transfer(&self, request: &transfer::Request) -> TransferPlan {
        let target = request.target.as_local();
        let places: Vec<(PathBuf, Option<PathBuf>)> = if request.retrying.is_empty() {
            // The engine's: a case-only rename of the one item goes to the target itself.
            let case_only = request.move_
                && request.sources.len() == 1
                && (request.sources[0].as_local())
                    .zip(target)
                    .is_some_and(|(source, target)| {
                        same_place(source, target) && source.file_name() != target.file_name()
                    });
            let into =
                !case_only && (request.sources.len() > 1 || target.is_some_and(Path::is_dir));
            (request.sources.iter())
                .filter_map(|source| {
                    let local = source.as_local()?.to_path_buf();
                    let destination = target.map(|target| match local.file_name() {
                        Some(name) if into => target.join(name),
                        _ => target.to_path_buf(),
                    });
                    Some((local, destination))
                })
                .collect()
        } else {
            (request.retrying.iter())
                .filter_map(|skipped| {
                    let source = skipped.source.as_local()?.to_path_buf();
                    Some((
                        source,
                        skipped.destination.as_local().map(Path::to_path_buf),
                    ))
                })
                .collect()
        };
        TransferPlan {
            cache: self.clone(),
            places,
            moves: request.move_,
            walks: request.walks(),
        }
    }

    /// What a deletion did (FAR's `DelTreeName` of each folder removed): the folders gone leave.
    pub fn after_delete(&self, request: &delete::Request) {
        for item in request.items.iter().filter(|item| item.entry.is_dir) {
            if let Some(folder) = item.path.as_local() {
                self.forget_gone(folder);
            }
        }
    }

    /// The lines of `root`'s file changed by `edit`, written back when they changed; nothing without a
    /// file.
    fn change(&self, root: &Path, edit: impl FnOnce(&mut Vec<String>)) {
        let Some(file) = self.file_of(root) else {
            return;
        };
        let _files = files();
        let Ok(text) = std::fs::read_to_string(&file) else {
            return;
        };
        let before: Vec<String> = lines_of(&text).map(str::to_string).collect();
        let mut lines = before.clone();
        edit(&mut lines);
        if lines != before && write(&file, &lines).is_err() {
            let _ = std::fs::remove_file(&file);
        }
    }
}

/// A copy, move or link's folders, once it has ended (FAR's `AddTreeName`, `DelTreeName`,
/// `RenTreeName` as `ShellCopy` goes): the folders it made are put in (a link to a folder alone, not
/// what it leads to), a move's sources that are gone leave.
#[derive(Debug)]
pub struct TransferPlan {
    cache: TreeCache,
    /// Each source and where it goes (none: not a local folder, only a move's source leaves).
    places: Vec<(PathBuf, Option<PathBuf>)>,
    moves: bool,
    walks: bool,
}

impl TransferPlan {
    pub fn apply(self) {
        for (source, destination) in &self.places {
            if let Some(destination) = destination
                && self.moves
                && same_place(source, destination)
            {
                self.cache.renamed_case(source, destination);
                continue;
            }
            if self.moves {
                self.cache.forget_gone(source);
            }
            let Some(destination) = destination.as_ref().filter(|place| place.is_dir()) else {
                continue;
            };
            let root = self.cache.root_of(destination);
            if !self.cache.has_file(&root) {
                continue;
            }
            let mut made = vec![destination.clone()];
            if self.walks {
                made.extend(super::tree::folders_under(destination));
            }
            self.cache.add(&root, &made);
        }
    }
}

/// `path` with its `.` and `..` worked out by name (F7's name may hold them).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            part => out.push(part),
        }
    }
    out
}

/// The first folder of `path` (`parent` joined with F7's name) that is not there yet: the top of
/// what F7 is to make; none when it is all there.
pub fn first_missing(path: &Path) -> Option<PathBuf> {
    let path = lexical(path);
    let mut top = None;
    for above in path.ancestors() {
        if above.exists() {
            break;
        }
        top = Some(above.to_path_buf());
    }
    top
}

/// FAR's lines: those that start with [`SEP`].
fn lines_of(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| line.starts_with(SEP))
}

/// A line's path, without the time after it.
fn path_part(line: &str) -> &str {
    line.split_once('\t').map_or(line, |(path, _)| path)
}

fn nanos_of(time: SystemTime) -> Option<u64> {
    u64::try_from(time.duration_since(UNIX_EPOCH).ok()?.as_nanos()).ok()
}

fn time_of(text: &str) -> Option<SystemTime> {
    let nanos: u64 = text.trim().parse().ok()?;
    Some(UNIX_EPOCH + Duration::from_nanos(nanos))
}

/// The folder of a line (none for the root's own [`SEP`]).
fn folder_of(root: &Path, line: &str) -> Option<PathBuf> {
    let rest = path_part(line).trim_start_matches(SEP);
    (!rest.is_empty()).then(|| {
        let mut path = root.to_path_buf();
        path.extend(rest.split(SEP));
        path
    })
}

/// The line of a folder under `root`.
fn line_of(root: &Path, folder: &Path) -> Option<String> {
    let folder = lexical(folder);
    let rest = folder.strip_prefix(root).ok()?;
    let names: Vec<String> = (rest.components())
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!names.is_empty()).then(|| format!("{SEP}{}", names.join(std::path::MAIN_SEPARATOR_STR)))
}

/// The lines written whole or not at all: to a file beside it first.
fn write(file: &Path, lines: &[String]) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let eol = crate::platform::LINE_BREAK;
    let mut text = lines.join(eol);
    text.push_str(eol);
    // Whole or not at all, the exact name, as the settings (R7 review E5).
    crate::vfs::local::ops::write_atomic(file, text.as_bytes())
}

#[cfg(test)]
#[path = "tree_cache_tests.rs"]
mod tests;
