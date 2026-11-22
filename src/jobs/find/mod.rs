//! Alt+F7: FAR's `background_searcher` (`far/findfile.cpp`) on its own thread. Each root is walked as
//! FAR's `ScanTree` with `FilesFirst` walks it: a folder's files, then each of its folders followed at
//! once into; the names the mask takes are found, folders only when asked and no text is looked for,
//! files with the text in them (or without it, `not_containing`) when there is one. Links to folders
//! are walked into when asked (FAR's "Search in symbolic links"), never into a folder they are inside of.
//! With "Search in alternate streams" each named stream of an object is one more object
//! (`a.txt:notes`, after the object itself), looked at as a file.
//! With "Use filter" each object goes through Find file's filter first (FAR's `FileInFilterEx`): one an
//! excluding filter takes is left out, a folder with what is in it; one no filter takes is left out
//! while some filter includes, a folder still walked into.
//! With "Search in archives" each file the filter lets through, whatever its name, is offered to the
//! plugins after it is looked at (FAR's `ArchiveSearch`): one that opens it without any window gives its
//! folder, walked from its root into its folders, the objects found there named by their place inside
//! the archive's file; closed after. No archive inside an archive (its files have no real names).
//! The UI takes what is found as it comes, the place being searched, and whether the search is over.

pub mod text;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use super::folder_size::identity;
use crate::platform::files::alternate_streams;
use crate::vfs::filter::{AreaFilter, Verdict};
use crate::vfs::{Entry, FileMask, FileOpener, OpenedFile, VfsPath, provider_for};

/// How often a paused search looks whether it may go on.
const PAUSE_POLL: Duration = Duration::from_millis(50);

/// What to look for and where.
#[derive(Debug, Clone)]
pub struct Request {
    /// Walked one after another (FAR's `Locations`; the selected folders).
    pub roots: Vec<VfsPath>,
    /// The drives walked after them, taken when the search starts.
    pub drives: Option<Drives>,
    /// Into the folders (all areas but "The current folder only" and "In PATH folders").
    pub recurse: bool,
    pub mask: FileMask,
    /// FAR's "Search for folders".
    pub folders: bool,
    /// FAR's "Search in symbolic links": links to folders are walked into.
    pub links: bool,
    /// FAR's "Containing"; none: names only.
    pub pattern: Option<text::Pattern>,
    /// FAR's "Not containing": the files without the text.
    pub not_containing: bool,
    /// FAR's "Search only in the first": so many bytes of each file, 0 for all.
    pub first: u64,
    /// FAR's "Search in alternate streams".
    pub streams: bool,
    /// FAR's "Use filter": Find file's filter, and the time its dates are counted from.
    pub filter: Option<(AreaFilter, SystemTime)>,
    /// FAR's "Search in archives": the opener of a disk's files as the plugins' folders.
    pub archives: Option<Archives>,
}

/// The opener of "Search in archives" (FAR's `OpenFilePlugin` with `OFP_SEARCH`).
#[derive(Clone)]
pub struct Archives(pub Arc<dyn FileOpener>);

impl std::fmt::Debug for Archives {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Archives")
    }
}

impl Request {
    /// What the filter says of `entry`; with no filter it is listed.
    fn verdict(&self, entry: &Entry) -> Verdict {
        self.filter
            .as_ref()
            .map_or(Verdict::Include, |(filter, now)| {
                filter.verdict(entry, *now)
            })
    }
}

/// FAR's "In all non-removable drives" (`network`) and "In all local drives": the fixed drives and RAM
/// disks, with or without the network drives, those with a letter and those without.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drives {
    pub network: bool,
}

impl Drives {
    fn roots(self) -> Vec<VfsPath> {
        use crate::platform::volumes::{DriveKind, drive_kind, unmounted_volumes, volume_roots};
        let takes = |root: &String| match drive_kind(root) {
            DriveKind::Fixed | DriveKind::RamDisk => true,
            DriveKind::Remote => self.network,
            _ => false,
        };
        let lettered = volume_roots().into_iter().map(|root| root.path);
        (lettered.chain(unmounted_volumes()))
            .filter(takes)
            .map(VfsPath::local)
            .collect()
    }
}

/// One object found: the folder it is in and its entry as that folder lists it. Found in an archive,
/// its folder is its place inside the archive's file (`archive:D:\a.zip!/dir`, FAR's `ArcListItem`),
/// which stays after the search has closed the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub folder: VfsPath,
    pub entry: Entry,
}

impl Found {
    pub fn path(&self) -> VfsPath {
        self.folder.join(&self.entry.name)
    }
}

#[derive(Debug, Default)]
struct State {
    /// Found and not taken by the UI yet.
    found: Vec<Found>,
    /// FAR's `m_Status`: the folder walked, or the file read for the text.
    status: String,
    /// FAR's `m_Percent`: how much of the file read for the text is read.
    percent: u8,
    over: bool,
}

#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    /// Also the walk's `vfs::stop`: a plugin's file read for the text stops with the search.
    stop: Arc<AtomicBool>,
    pause: AtomicBool,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// FAR's `PauseEvent.wait()`; false once stopped.
    fn go_on(&self) -> bool {
        while self.pause.load(Ordering::Relaxed) && !self.stopped() {
            std::thread::sleep(PAUSE_POLL);
        }
        !self.stopped()
    }

    fn set_status(&self, status: String) {
        self.state().status = status;
    }

    fn add(&self, folder: &VfsPath, entry: Entry) {
        self.state().found.push(Found {
            folder: folder.clone(),
            entry,
        });
    }
}

/// Marks the search over however its thread ends, a panic too (review R28: never "searching" for ever).
struct Over(Arc<Shared>);

impl Drop for Over {
    fn drop(&mut self) {
        self.0.state().over = true;
    }
}

/// Every root of `request` walked in turn, the drives after them, until the search stops.
fn walk_all(request: &Request, shared: &Shared) {
    let drives = request.drives.map(Drives::roots).unwrap_or_default();
    let walker = Walker {
        request,
        shared,
        archive: None,
        outer: None,
        recurse: request.recurse,
    };
    for root in request.roots.iter().chain(&drives) {
        if shared.stopped() {
            break;
        }
        walker.walk(root);
    }
}

/// A search on its own thread; dropping it stops the search.
#[derive(Debug)]
pub struct Search(Arc<Shared>);

impl Search {
    /// Starts `request`. Without a thread for it the search is over at once, with nothing found.
    pub fn start(request: Request) -> Self {
        let shared = Arc::new(Shared::default());
        let searching = Arc::clone(&shared);
        let started = std::thread::Builder::new()
            .name("find file".to_string())
            .spawn(move || {
                let _over = Over(Arc::clone(&searching));
                let stop = Arc::clone(&searching.stop);
                crate::vfs::stop::stopped_by(stop, || walk_all(&request, &searching));
            });
        if started.is_err() {
            shared.state().over = true;
        }
        Self(shared)
    }

    /// A search that never moves on, with `found` already there and over or not.
    #[cfg(test)]
    pub fn stalled(found: Vec<Found>, over: bool) -> Self {
        let search = Self(Arc::new(Shared::default()));
        *search.0.state() = State {
            found,
            status: "the place".to_string(),
            percent: 42,
            over,
        };
        search
    }

    /// Whether the search was told to stop, and to wait.
    #[cfg(test)]
    pub fn told(&self) -> (bool, bool) {
        (self.0.stopped(), self.0.pause.load(Ordering::Relaxed))
    }

    /// What was found since the last call.
    pub fn take_found(&self) -> Vec<Found> {
        std::mem::take(&mut self.0.state().found)
    }

    pub fn status(&self) -> String {
        self.0.state().status.clone()
    }

    pub fn percent(&self) -> u8 {
        self.0.state().percent
    }

    /// The walk has ended (all found may not be taken yet).
    pub fn over(&self) -> bool {
        self.0.state().over
    }

    /// FAR's `Pause` while its question is up, and `Resume`.
    pub fn pause(&self, on: bool) {
        self.0.pause.store(on, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.0.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A folder being walked: its folders still to go into.
struct Level {
    folder: VfsPath,
    folders: std::vec::IntoIter<Entry>,
    /// The disk's number for the folder, asked only when a link is met (FAR's `ActiveDirectories`).
    place: Option<Option<(u32, u64)>>,
}

impl Level {
    fn place(&mut self) -> Option<(u32, u64)> {
        *self.place.get_or_insert_with(|| identity(&self.folder))
    }
}

/// One walk of the search (FAR's `background_searcher` scanning a place): the disk's, or inside an
/// archive opened on the way (`ArchiveSearch`: from its root and into its folders, FAR's
/// `FINDAREA_FROM_CURRENT`).
#[derive(Clone, Copy)]
struct Walker<'a> {
    request: &'a Request,
    shared: &'a Shared,
    /// Inside an archive: its file on a disk, and its folder opened (FAR's `m_FindFileArcItem`).
    archive: Option<(&'a Path, &'a OpenedFile)>,
    /// The walk this one is inside of (its archive opened by that one).
    outer: Option<&'a Walker<'a>>,
    /// Into the folders.
    recurse: bool,
}

impl Walker<'_> {
    /// Where the search's folder `folder` is for the results: inside an archive, its place inside the
    /// archive's file (the archive's folder is closed after the walk); else itself.
    fn place(&self, folder: &VfsPath) -> VfsPath {
        match self.archive {
            Some((file, opened)) => opened.place_of(file, folder),
            None => folder.clone(),
        }
    }

    /// FAR's `m_Status`: the place as the results show it.
    fn status(&self, path: &VfsPath) {
        self.shared.set_status(self.place(path).shown());
    }

    fn add(&self, folder: &VfsPath, entry: Entry) {
        self.shared.add(&self.place(folder), entry);
    }

    /// FAR's `DoScanTree` for one root.
    fn walk(&self, root: &VfsPath) {
        self.status(root);
        let mut levels = vec![self.open(root)];
        while let Some(level) = levels.last_mut() {
            if !self.shared.go_on() {
                return;
            }
            let Some(folder) = level.folders.next() else {
                levels.pop();
                continue;
            };
            let path = level.folder.join(&folder.name);
            self.status(&path);
            let parent = level.folder.clone();
            let link = folder.points_elsewhere();
            let enter = self.folder(&parent, folder);
            if enter && (!link || !leads_back(&path, &mut levels)) {
                let next = self.open(&path);
                levels.push(next);
            }
        }
    }

    /// A folder met in `parent`: found when folders are looked for, its streams looked at; whether it
    /// is walked into (FAR's `SkipDir` for a folder a filter leaves out, a link only when asked).
    fn folder(&self, parent: &VfsPath, folder: Entry) -> bool {
        let request = self.request;
        let verdict = request.verdict(&folder);
        let link = folder.points_elsewhere();
        let enter = self.recurse && (!link || request.links) && verdict != Verdict::Exclude;
        let streams = streams_of(parent, &folder, request);
        let wanted = request.folders && request.pattern.is_none();
        if wanted && verdict.lists() && request.mask.matches(&folder.name) {
            self.add(parent, folder);
        }
        for stream in streams {
            self.take_file(parent, stream);
        }
        enter
    }

    /// Lists `folder`: its files are looked at now, its folders are kept to go into.
    fn open(&self, folder: &VfsPath) -> Level {
        let mut entries = Vec::new();
        if let Ok(provider) = provider_for(folder) {
            // A folder that cannot be read is searched in for what was listed of it.
            let _ = provider.list(folder, &self.shared.stop, &mut |entry| entries.push(entry));
        }
        let (folders, files): (Vec<Entry>, Vec<Entry>) =
            entries.into_iter().partition(|entry| entry.is_dir);
        for file in files {
            if !self.shared.go_on() {
                break;
            }
            let streams = streams_of(folder, &file, self.request);
            self.take_file(folder, file);
            for stream in streams {
                self.take_file(folder, stream);
            }
        }
        Level {
            folder: folder.clone(),
            folders: folders.into_iter(),
            place: None,
        }
    }

    /// The file found when the mask takes its name and it has what is looked for; then, with "Search in
    /// archives", what is in it when it opens as an archive (FAR: every file the filter lets through,
    /// whatever its name, `ArchiveSearch` after `IsFileIncluded`).
    fn take_file(&self, folder: &VfsPath, file: Entry) {
        if !self.request.verdict(&file).lists() {
            return;
        }
        let archive = self.archive_of(folder, &file);
        // A file read when the search stopped is not taken: "Not containing" would take it.
        if self.request.mask.matches(&file.name)
            && self.takes_file(folder, &file)
            && !self.shared.stopped()
        {
            self.add(folder, file);
        }
        if let Some(archive) = archive {
            self.search_archive(&archive);
        }
    }

    /// The disk's file to search in as an archive: by its path on a disk or its real name (FAR's
    /// `OPIF_REALNAMES`: an archive's own files have none, so no archive inside an archive).
    fn archive_of(&self, folder: &VfsPath, file: &Entry) -> Option<PathBuf> {
        self.request.archives.as_ref()?;
        let path = folder.join(&file.name);
        (path.as_local().map(Path::to_path_buf)).or_else(|| file.real_path.clone())
    }

    /// FAR's `ArchiveSearch`: `file` opened by a plugin without any window, walked from its root into
    /// its folders, closed after.
    fn search_archive(&self, file: &Path) {
        let Some(Archives(opener)) = &self.request.archives else {
            return;
        };
        if !self.shared.go_on() || self.is_inside(file) {
            return;
        }
        self.shared.set_status(file.display().to_string());
        let Some(opened) = opener.open(file, &self.shared.stop) else {
            return;
        };
        let inside = Walker {
            archive: Some((file, &opened)),
            outer: Some(self),
            recurse: true,
            ..*self
        };
        inside.walk(opened.root());
    }

    /// Whether this walk is inside `file` already (a list naming itself, two naming each other: a
    /// plugin with real names opened on the way), which is not opened again.
    fn is_inside(&self, file: &Path) -> bool {
        let mut walk = Some(self);
        while let Some(current) = walk {
            if current.archive.is_some_and(|(open, _)| open == file) {
                return true;
            }
            walk = current.outer;
        }
        false
    }

    /// FAR's `IsFileIncluded` past the mask: with a pattern, whether the file has it (a file that cannot
    /// be read has not, so "Not containing" takes it, as FAR's).
    fn takes_file(&self, folder: &VfsPath, file: &Entry) -> bool {
        let request = self.request;
        let Some(pattern) = &request.pattern else {
            return true;
        };
        let path = folder.join(&file.name);
        self.status(&path);
        let shared = self.shared;
        let mut percent = |percent| shared.state().percent = percent;
        let reading = text::Reading {
            limit: request.first,
            size: file.size,
            stop: &shared.stop,
            percent: &mut percent,
        };
        let has = provider_for(&path)
            .and_then(|provider| provider.open_read(&path))
            .and_then(|mut reader| text::contains(&mut reader, pattern, reading))
            .unwrap_or(false);
        has != request.not_containing
    }
}

/// Whether a link leads to a folder it is inside of (FAR's recursive symlinks guard).
fn leads_back(link: &VfsPath, levels: &mut [Level]) -> bool {
    let Some(place) = identity(link) else {
        return false;
    };
    levels.iter_mut().any(|level| level.place() == Some(place))
}

/// The object's named streams as objects of their own, when asked: `a.txt:notes`, the stream's size,
/// never a folder (FAR's `enum_streams` loop of `DoScanTree`).
fn streams_of(folder: &VfsPath, entry: &Entry, request: &Request) -> Vec<Entry> {
    // No path made for each name when streams are not looked for.
    if !request.streams {
        return Vec::new();
    }
    let path = folder.join(&entry.name);
    let Some(local) = path.as_local() else {
        return Vec::new();
    };
    (alternate_streams(local).into_iter())
        .map(|(stream, size)| Entry {
            name: format!("{}:{stream}", entry.name),
            size,
            is_dir: false,
            // FAR: `Attributes &= ~FILE_ATTRIBUTE_DIRECTORY`, a folder's stream is a file.
            attributes: entry.attributes.without_directory(),
            link: None,
            link_target: None,
            ..entry.clone()
        })
        .collect()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "archives_tests.rs"]
mod archives_tests;
