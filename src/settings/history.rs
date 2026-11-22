//! The histories kept between runs, as FAR's (`history.cpp`, its history database): the commands
//! (Alt+F8), the folders (Alt+F12), the files viewed and edited (Alt+F11), and the dialogs' edit lines
//! by their names (FAR's `HISTORYTYPE_DIALOG`, `DialogsHistoryCount` and `Lifetime` the same). Each change is saved at once,
//! on a thread of its own; 1000 lines of each stay whatever their age, older than 90 days only those,
//! and locked ones always (`HistoryCount`, `HistoryLifetime`, `DeleteOldUnlocked`). A line given again
//! moves to the end and keeps its lock. Each copy of the program keeps only what it changed and puts
//! that onto the file as it is read anew for each save, so that two copies keep each other's lines and
//! what one takes out (or unlocks) stays so.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::platform::files::fold_case;
use crate::vfs::local::ops;

/// FAR's `HistoryCount`.
const KEPT: usize = 1000;
/// FAR's `HistoryLifetime`, in seconds.
const LIFETIME: u64 = 90 * 24 * 60 * 60;

/// Which history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum List {
    Commands,
    Folders,
    Views,
}

impl List {
    pub const ALL: [Self; 3] = [Self::Commands, Self::Folders, Self::Views];
}

/// A history to change: one of the three, or a dialog's edit lines by its name (FAR's `strHistory`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    List(List),
    Dialog(String),
}

impl From<List> for Key {
    fn from(list: List) -> Self {
        Self::List(list)
    }
}

/// What a line of the view and edit history opened (FAR's `HR_VIEWER`, `HR_EDITOR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Plain,
    View,
    Edit,
}

impl Kind {
    fn is_plain(&self) -> bool {
        *self == Self::Plain
    }
}

/// One line of a history, with when (seconds since 1970).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub time: u64,
    pub text: String,
    /// Ins in the history menu: kept whatever its age, Del leaves it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Kind::is_plain")]
    pub kind: Kind,
    /// Where a command was given (FAR's history data, which F3 in the menu shows).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

impl Entry {
    /// `text` given just now.
    pub fn new(text: &str, kind: Kind) -> Self {
        Self {
            time: now(),
            text: text.to_string(),
            locked: false,
            kind,
            folder: String::new(),
        }
    }

    /// The same line: names compare as the system compares them (without case on Windows); a view and
    /// an edit of one file are two lines.
    pub fn same(&self, other: &Self) -> bool {
        self.kind == other.kind && fold_case(&self.text) == fold_case(&other.text)
    }
}

/// The histories, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Histories {
    pub commands: Vec<Entry>,
    pub folders: Vec<Entry>,
    pub views: Vec<Entry>,
    /// The dialogs' edit lines, by the history's name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub dialogs: BTreeMap<String, Vec<Entry>>,
}

/// No lines (a dialog's history nothing was given to yet).
static NONE: Vec<Entry> = Vec::new();

impl Histories {
    pub fn list(&self, list: List) -> &Vec<Entry> {
        match list {
            List::Commands => &self.commands,
            List::Folders => &self.folders,
            List::Views => &self.views,
        }
    }

    pub fn list_mut(&mut self, list: List) -> &mut Vec<Entry> {
        match list {
            List::Commands => &mut self.commands,
            List::Folders => &mut self.folders,
            List::Views => &mut self.views,
        }
    }

    /// The lines of a dialog's history.
    pub fn dialog(&self, name: &str) -> &Vec<Entry> {
        self.dialogs.get(name).unwrap_or(&NONE)
    }

    pub fn entries_mut(&mut self, key: &Key) -> &mut Vec<Entry> {
        match key {
            Key::List(list) => self.list_mut(*list),
            Key::Dialog(name) => self.dialogs.entry(name.clone()).or_default(),
        }
    }

    /// Every history there is.
    fn keys(&self) -> Vec<Key> {
        let lists = List::ALL.into_iter().map(Key::List);
        lists
            .chain(self.dialogs.keys().cloned().map(Key::Dialog))
            .collect()
    }

    /// FAR's `AddToHistory`: the line given again leaves its old place and keeps its lock.
    pub fn add(&mut self, key: impl Into<Key>, mut entry: Entry) {
        let entries = self.entries_mut(&key.into());
        if let Some(at) = entries.iter().position(|known| known.same(&entry)) {
            entry.locked |= entries.remove(at).locked;
        }
        entries.push(entry);
    }
}

/// `history.json` beside the settings file.
pub fn history_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("history.json")
}

/// The saved histories; a missing file is an empty history, a broken one `InvalidData`.
pub fn load(path: &Path) -> io::Result<Histories> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Histories::default()),
        Err(error) => return Err(error),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str(text).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
    })
}

fn save(path: &Path, histories: &Histories) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string(histories).map_err(io::Error::other)?;
    ops::write_atomic(path, text.as_bytes())
}

/// Oldest first, each line once (its latest), and only what FAR keeps.
pub fn compact(mut entries: Vec<Entry>, now: u64) -> Vec<Entry> {
    entries.sort_by_key(|entry| entry.time);
    // The latest of each line: its place, and the lock of the older ones (given again it keeps it).
    let mut latest: HashMap<(String, Kind), (usize, bool)> = HashMap::new();
    for (at, entry) in entries.iter().enumerate() {
        let key = (fold_case(&entry.text).into_owned(), entry.kind);
        let locked = latest.get(&key).is_some_and(|(_, locked)| *locked) || entry.locked;
        latest.insert(key, (at, locked));
    }
    let mut kept: Vec<Entry> = Vec::with_capacity(latest.len());
    for (at, mut entry) in entries.into_iter().enumerate() {
        let key = (fold_case(&entry.text).into_owned(), entry.kind);
        if let Some(&(last, locked)) = latest.get(&key)
            && last == at
        {
            entry.locked = locked;
            kept.push(entry);
        }
    }
    let oldest_kept = now.saturating_sub(LIFETIME);
    // FAR's `ORDER BY lock DESC, time DESC LIMIT Count`.
    let mut ranked: Vec<usize> = (0..kept.len()).collect();
    ranked.sort_by_key(|&index| Reverse((kept[index].locked, kept[index].time, index)));
    let newest: HashSet<usize> = ranked.into_iter().take(KEPT).collect();
    let mut index = 0;
    kept.retain(|entry| {
        let keep = entry.locked || entry.time >= oldest_kept || newest.contains(&index);
        index += 1;
        keep
    });
    kept
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// What the writer's thread is told.
#[derive(Debug)]
enum Change {
    Add(Key, Entry),
    /// The line, as it was.
    Remove(Key, Entry),
    /// The line with its new lock.
    Lock(Key, Entry),
    /// Del in the menu, at this time: what is not locked goes.
    Clear(Key, u64),
}

/// What this copy of the program has changed and not saved yet, in order. The file is read anew before
/// each save and these are done to it again, so what another copy saved meanwhile stays, and what this
/// copy took out goes from the file however many copies held it.
#[derive(Debug, Default)]
struct Pending {
    changes: Vec<Change>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    fn take(&mut self, change: Change) {
        self.changes.push(change);
    }

    /// The file with this copy's changes done to it.
    fn apply(&self, mut file: Histories, now: u64) -> Histories {
        for change in &self.changes {
            match change {
                Change::Add(key, entry) => file.add(key.clone(), entry.clone()),
                Change::Remove(key, line) => file
                    .entries_mut(key)
                    .retain(|entry| !(entry.same(line) && entry.time <= line.time)),
                Change::Lock(key, line) => {
                    let entries = file.entries_mut(key).iter_mut();
                    for entry in entries.filter(|entry| entry.same(line) && entry.time == line.time)
                    {
                        entry.locked = line.locked;
                    }
                }
                // What another copy gave since stays.
                Change::Clear(key, time) => file
                    .entries_mut(key)
                    .retain(|entry| entry.locked || entry.time > *time),
            }
        }
        for key in file.keys() {
            let entries = std::mem::take(file.entries_mut(&key));
            *file.entries_mut(&key) = compact(entries, now);
        }
        file.dialogs.retain(|_, entries| !entries.is_empty());
        file
    }
}

/// Why a save did not happen.
#[derive(Debug)]
enum Failure {
    /// The file cannot be read now (another copy is putting its new file in place): next time.
    Now(io::Error),
    /// A broken file, which is not saved over, or the save itself failed.
    Lasting(io::Error),
}

/// Puts what is pending into the file. Missing, the file is empty.
fn save_pending(path: &Path, pending: &Pending) -> Result<(), Failure> {
    let file = match load(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return Err(Failure::Lasting(error));
        }
        Err(error) => return Err(Failure::Now(error)),
    };
    save(path, &pending.apply(file, now())).map_err(Failure::Lasting)
}

/// Reads in a row that fail before the failure is told.
const FAILED_READS_TOLD: u32 = 3;
/// How often and how long the last changes are tried when the program ends.
const LAST_TRIES: u32 = 10;
const BETWEEN_TRIES: std::time::Duration = std::time::Duration::from_millis(50);

fn tell(failure: &Mutex<Option<String>>, error: &io::Error) {
    let mut failure = failure.lock().unwrap_or_else(PoisonError::into_inner);
    failure.get_or_insert(error.to_string());
}

/// Saves the histories, off the UI thread.
#[derive(Debug)]
pub struct Writer {
    changes: Option<Sender<Change>>,
    thread: Option<JoinHandle<()>>,
    failure: Arc<Mutex<Option<String>>>,
}

impl Writer {
    /// Starts saving to `path`.
    pub fn start(path: PathBuf) -> Self {
        let (changes, received) = mpsc::channel::<Change>();
        let failure = Arc::new(Mutex::new(None));
        let reported = Arc::clone(&failure);
        let thread = std::thread::Builder::new()
            .name("history".to_string())
            .spawn(move || {
                let mut pending = Pending::default();
                let mut failed_reads = 0;
                while let Ok(change) = received.recv() {
                    for change in std::iter::once(change).chain(received.try_iter()) {
                        pending.take(change);
                    }
                    match save_pending(&path, &pending) {
                        Ok(()) => (pending, failed_reads) = (Pending::default(), 0),
                        Err(Failure::Lasting(error)) => tell(&reported, &error),
                        Err(Failure::Now(error)) => {
                            failed_reads += 1;
                            if failed_reads == FAILED_READS_TOLD {
                                tell(&reported, &error);
                            }
                        }
                    }
                }
                // The program ends: a file that cannot be read just now is tried a little longer.
                for _ in 0..LAST_TRIES {
                    if pending.is_empty()
                        || !matches!(save_pending(&path, &pending), Err(Failure::Now(_)))
                    {
                        break;
                    }
                    std::thread::sleep(BETWEEN_TRIES);
                }
            });
        // Without a thread nothing is saved, and that is told as a failure to save is (review R24).
        let thread = thread.inspect_err(|error| tell(&failure, error)).ok();
        Self {
            changes: thread.is_some().then_some(changes),
            thread,
            failure,
        }
    }

    fn send(&self, change: Change) {
        if let Some(changes) = &self.changes {
            // The thread only ends when this is dropped: there is always someone to take it.
            let _ = changes.send(change);
        }
    }

    /// `entry` was given just now.
    pub fn add(&self, key: impl Into<Key>, entry: &Entry) {
        self.send(Change::Add(key.into(), entry.clone()));
    }

    /// `entry` leaves the history.
    pub fn remove(&self, key: impl Into<Key>, entry: &Entry) {
        self.send(Change::Remove(key.into(), entry.clone()));
    }

    /// `entry` is locked or unlocked, as it says.
    pub fn set_lock(&self, key: impl Into<Key>, entry: &Entry) {
        self.send(Change::Lock(key.into(), entry.clone()));
    }

    /// What is not locked leaves the history.
    pub fn clear_unlocked(&self, key: impl Into<Key>) {
        self.send(Change::Clear(key.into(), now()));
    }

    /// Why saving failed, once.
    pub fn take_failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Waits until everything is saved, at most a few seconds (review R12: a profile on a share that
    /// does not answer does not hold the program's end).
    pub fn finish(mut self) {
        self.changes = None;
        if let Some(thread) = self.thread.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !thread.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // A thread that panicked has nothing more to save; one still writing ends with the process.
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
