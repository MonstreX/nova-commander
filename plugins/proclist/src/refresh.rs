//! The processes read again each second (FAR's `PerfThread::ThreadProc`): a thread of the plugin's reads
//! the system's records into the table and then asks for the panel to be read again (FAR's
//! `ACTL_SYNCHRO`); the plugin's thread reads the table when it lists the panel. Ctrl+R and the keys
//! that change processes read them at once (FAR's `SmartReread` with a read this fast). A read the system
//! refuses leaves the table as it was (FAR's `bOK` false keeps its data). The files' descriptions (the
//! friendly names, `ZD`) are read by the thread too, a part each time, never by a listing: read cold they
//! take seconds. The thread ends as the last panel closes, or with the plugin.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nc_plugin_sdk::system::{ProcessDetails, ProcessRecord};

use crate::table::Table;

/// FAR's `dwRefreshMsec`.
pub const EVERY: Duration = Duration::from_secs(1);

/// What a read takes from the system: the records, the processors, a process's details.
pub trait Source: Send + Sync + 'static {
    fn records(&self) -> io::Result<Vec<ProcessRecord>>;
    fn processors(&self) -> u64;
    fn details(&self, pid: u32) -> Option<ProcessDetails>;
    /// The description of the file `path` (its version's).
    fn description(&self, path: &str) -> Option<String>;
}

/// The local computer's.
pub struct Local;

impl Source for Local {
    fn records(&self) -> io::Result<Vec<ProcessRecord>> {
        nc_plugin_sdk::system::processes()
    }

    fn processors(&self) -> u64 {
        u64::try_from(nc_plugin_sdk::system::logical_processors()).unwrap_or(1)
    }

    fn details(&self, pid: u32) -> Option<ProcessDetails> {
        nc_plugin_sdk::system::process_details(pid)
    }

    fn description(&self, path: &str) -> Option<String> {
        // FAR's `GetVersionInfo` reads a path without its `\??\`.
        let path = path.strip_prefix(r"\??\").unwrap_or(path);
        nc_plugin_sdk::system::file_version(path.as_ref())?.description
    }
}

/// How long one turn of the thread reads descriptions before the panel is read again.
const DESCRIBING: Duration = Duration::from_millis(300);

struct Shared {
    source: Box<dyn Source>,
    table: Mutex<Table>,
    /// The files' descriptions read so far, by path (none: the file has none).
    descriptions: Mutex<HashMap<String, Option<String>>>,
    /// One read at a time: the thread's and the plugin's own.
    reading: Mutex<()>,
    stop: Mutex<bool>,
    woken: Condvar,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// The records read now into the next table; refused, the table stays.
    fn read(&self) -> io::Result<()> {
        let _one = lock(&self.reading);
        let records = self.source.records()?;
        let processors = self.source.processors();
        let before = lock(&self.table).clone();
        let mut details = |pid| self.source.details(pid);
        let next = before.next(&records, Instant::now(), processors, &mut details);
        *lock(&self.table) = next;
        Ok(())
    }

    /// The descriptions of the table's files not read yet, for `DESCRIBING` at most: whether some were.
    fn describe(&self) -> bool {
        let paths: Vec<String> = {
            let table = lock(&self.table);
            let known = lock(&self.descriptions);
            let mut paths: Vec<String> = (table.processes.iter())
                .filter_map(|process| process.path.clone())
                .filter(|path| !known.contains_key(path))
                .collect();
            paths.sort_unstable();
            paths.dedup();
            paths
        };
        let started = Instant::now();
        let mut some = false;
        for path in paths {
            if started.elapsed() >= DESCRIBING || *lock(&self.stop) {
                break;
            }
            let description = self.source.description(&path);
            lock(&self.descriptions).insert(path, description);
            some = true;
        }
        some
    }

    /// Waits `EVERY` or until stopped: true when stopped.
    fn wait(&self) -> bool {
        let stop = lock(&self.stop);
        let (stop, _) = (self.woken)
            .wait_timeout_while(stop, EVERY, |stop| !*stop)
            .unwrap_or_else(PoisonError::into_inner);
        *stop
    }
}

/// The table and the thread keeping it.
pub struct Refresher {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Why the thread could not start (the table then read on demand only).
    pub not_started: Option<io::Error>,
}

impl Refresher {
    /// The table read once now, then each second by a thread that calls `changed` after each read.
    pub fn start(source: Box<dyn Source>, changed: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            source,
            table: Mutex::new(Table::default()),
            descriptions: Mutex::new(HashMap::new()),
            reading: Mutex::new(()),
            stop: Mutex::new(false),
            woken: Condvar::new(),
        });
        // Refused now, the thread's next read fills it.
        let _ = shared.read();
        let theirs = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("proclist refresh".to_string())
            .spawn(move || {
                loop {
                    if theirs.describe() {
                        changed();
                    }
                    if theirs.wait() {
                        break;
                    }
                    if theirs.read().is_ok() {
                        changed();
                    }
                }
            });
        let (thread, not_started) = match spawned {
            Ok(thread) => (Some(thread), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            shared,
            thread,
            not_started,
        }
    }

    /// The table as last read.
    pub fn table(&self) -> MutexGuard<'_, Table> {
        lock(&self.shared.table)
    }

    /// The description of the file `path` as read so far: `None` not read yet, `Some(None)` none.
    pub fn description(&self, path: &str) -> Option<Option<String>> {
        lock(&self.shared.descriptions).get(path).cloned()
    }

    /// The processes read again now (FAR's `SmartReread`); refused, the table stays as it was.
    pub fn read_now(&self) -> io::Result<()> {
        self.shared.read()
    }
}

impl Drop for Refresher {
    fn drop(&mut self) {
        *lock(&self.shared.stop) = true;
        self.shared.woken.notify_all();
        // Waited for even when a file's version read hangs (an image on a dead share): the plugin's
        // library is let go after this, so a thread left running would run code no longer there. FAR's
        // proclist reads the versions on its main thread, which hangs the same.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
