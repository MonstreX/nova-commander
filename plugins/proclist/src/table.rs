//! The processes as the panel shows them (FAR's `PerfThread::RefreshImpl` for the local computer): each
//! read of the system's records turned into the processes with their counters and what those show
//! since the read before; what only opening a process tells (its path, command line, bitness, GDI and
//! USER objects, owner) read once, when it is first seen (FAR's `!pOldTask`), and kept after. A process
//! whose number came back with another start is a new one (FAR takes it for the old). `_Total`, the
//! counters' sums, comes first, hidden, as the performance registry's instance FAR reads.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nc_plugin_sdk::system::{Owner, ProcessDetails, ProcessRecord};

use crate::counters::{self, COUNT};

/// The number FAR gives `_Total` (`static_cast<DWORD>(-1)`).
pub const TOTAL: u32 = u32::MAX;

/// The bitness of a process that is not WOW64's (FAR's `GetDefaultBitness` on 64-bit Windows).
pub const DEFAULT_BITS: u32 = 64;

/// FAR opens the processes above this number only (the idle process, System and the like below).
const FIRST_OPENED: u32 = 8;

/// One process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    pub threads: u32,
    /// Its base priority (FAR's `Priority Base`).
    pub priority: i32,
    pub session: u32,
    /// Its start in 100 ns units since 1601; 0: not known.
    pub created: i64,
    /// FAR's `qwCounters`.
    pub counters: [u64; COUNT],
    /// What they show since the read before (FAR's `qwResults`).
    pub results: [u64; COUNT],
    pub path: Option<String>,
    pub command_line: Option<String>,
    /// 32 or 64; none for `_Total`.
    pub bits: Option<u32>,
    pub gdi: u32,
    pub user: u32,
    /// `domain\user:session` (FAR's `Owner` of the item).
    pub owner: Option<String>,
}

impl Process {
    pub fn is_total(&self) -> bool {
        self.pid == TOTAL
    }

    /// The entry's name, one for each process (the program selects and reads its entries by name):
    /// the image's name and the number; `_Total` alone.
    pub fn key(&self) -> String {
        if self.is_total() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, self.pid)
        }
    }

    /// Its start as a time; none when not known.
    pub fn started(&self) -> Option<SystemTime> {
        /// 100 ns units from 1601 to 1970.
        const TO_1970: i64 = 116_444_736_000_000_000;
        let since = u64::try_from(self.created.checked_sub(TO_1970)?).ok()?;
        Some(UNIX_EPOCH + Duration::from_nanos(since.saturating_mul(100)))
    }

    /// FAR's size of the entry: the working set and the page file.
    pub fn size(&self) -> u64 {
        self.counters[counters::WORKING_SET].saturating_add(self.counters[counters::PAGE_FILE])
    }
}

/// The processes of one read.
#[derive(Debug, Clone, Default)]
pub struct Table {
    /// `_Total` first, then as the system lists them.
    pub processes: Vec<Process>,
    /// When the records were read; none before the first read.
    pub read_at: Option<Instant>,
}

/// FAR's owner text: `domain\user:session`.
pub fn owner_text(owner: &Owner, session: u32) -> String {
    let domain = if owner.domain.is_empty() {
        String::new()
    } else {
        format!("{}\\", owner.domain)
    };
    format!("{domain}{}:{session}", owner.user)
}

/// A record's name: the idle process has none (FAR's performance registry names it `Idle`).
fn name_of(record: &ProcessRecord) -> String {
    if record.name.is_empty() && record.pid == 0 {
        "Idle".to_string()
    } else {
        record.name.clone()
    }
}

/// What opening `record`'s process tells, put into `process`: once, when it is first seen.
fn open_details(process: &mut Process, details: Option<ProcessDetails>) {
    process.bits = Some(DEFAULT_BITS);
    let Some(details) = details else {
        return;
    };
    process.path = details.path;
    process.command_line = details.command_line;
    if details.wow64 {
        process.bits = Some(32);
    }
    process.gdi = details.gdi_objects;
    process.user = details.user_objects;
    if process.created == 0 {
        process.created = details.created.unwrap_or(0);
    }
    process.owner = (details.owner.as_ref()).map(|owner| owner_text(owner, process.session));
}

impl Table {
    /// The process `pid`, `_Total` too.
    pub fn process(&self, pid: u32) -> Option<&Process> {
        self.processes.iter().find(|process| process.pid == pid)
    }

    /// The process whose entry is named `key`.
    pub fn by_key(&self, key: &str) -> Option<&Process> {
        let pid = if key == "_Total" {
            TOTAL
        } else {
            key.rsplit_once(' ')?.1.parse().ok()?
        };
        self.process(pid).filter(|process| process.key() == key)
    }

    /// The table after this one: `records` read `now`, the processor times shared by `processors`, a
    /// new process's details asked of `details`.
    pub fn next(
        &self,
        records: &[ProcessRecord],
        now: Instant,
        processors: u64,
        details: &mut dyn FnMut(u32) -> Option<ProcessDetails>,
    ) -> Self {
        let elapsed = (self.read_at).map_or(0, |at| {
            u64::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u64::MAX)
        });
        let before: HashMap<u32, &Process> = (self.processes.iter())
            .map(|process| (process.pid, process))
            .collect();
        let mut processes = Vec::with_capacity(records.len() + 1);
        processes.push(Process::default());
        for record in records {
            let old =
                (before.get(&record.pid).copied()).filter(|old| old.created == record.created);
            let mut process = old.cloned().unwrap_or_default();
            process.pid = record.pid;
            process.parent = record.parent;
            process.name = name_of(record);
            process.threads = record.threads;
            process.priority = record.base_priority;
            process.session = record.session;
            process.created = record.created;
            process.counters = counters::values(record);
            let before = old.map(|old| &old.counters);
            process.results = counters::results(&process.counters, before, elapsed, processors);
            if old.is_none() {
                let opened = (record.pid > FIRST_OPENED)
                    .then(|| details(record.pid))
                    .flatten();
                open_details(&mut process, opened);
            }
            processes.push(process);
        }
        processes[0] = self.total(&processes[1..], elapsed, processors);
        Self {
            processes,
            read_at: Some(now),
        }
    }

    /// `_Total`: every counter and thread summed.
    fn total(&self, processes: &[Process], elapsed: u64, processors: u64) -> Process {
        let mut total = Process {
            pid: TOTAL,
            name: "_Total".to_string(),
            ..Process::default()
        };
        for process in processes {
            total.threads = total.threads.saturating_add(process.threads);
            for (sum, value) in total.counters.iter_mut().zip(process.counters) {
                *sum = sum.saturating_add(value);
            }
        }
        let before = self.process(TOTAL).map(|old| &old.counters);
        total.results = counters::results(&total.counters, before, elapsed, processors);
        total
    }
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
