//! FAR's proclist performance counters (`perfthread.cpp`'s `Counters`): 23 of them, each read from the
//! system's record of a process, and what each shows per second or in percent between two reads
//! (`qwResults`): a processor time's share of the time gone by (all the processors together), a count's
//! growth per second, the others as they are.

use nc_plugin_sdk::system::ProcessRecord;

/// How a counter is shown (FAR's `counter_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A processor time in 100 ns units.
    Duration,
    Bytes,
    Number,
}

/// How the counter's value between two reads is had (the performance registry's types FAR reads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
    /// As it is (`PERF_COUNTER_RAWCOUNT`, `..._LARGE_RAWCOUNT`).
    Raw,
    /// Its growth's share of the time gone by, in percent (`PERF_100NSEC_TIMER`).
    Timer,
    /// Its growth per second (`PERF_COUNTER_COUNTER`, `PERF_COUNTER_BULK_COUNT`).
    PerSecond,
}

/// One counter: its name, its column's title and per second's (FAR's `idName`, `idCol`, `idCol + 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    pub name: &'static str,
    pub column: &'static str,
    pub per_second_column: &'static str,
    pub kind: Kind,
    pub rate: Rate,
}

const fn counter(
    name: &'static str,
    column: &'static str,
    per_second_column: &'static str,
    kind: Kind,
    rate: Rate,
) -> Counter {
    Counter {
        name,
        column,
        per_second_column,
        kind,
        rate,
    }
}

/// The number of counters (FAR's `NCOUNTERS`).
pub const COUNT: usize = 23;

/// FAR's counters in their order (the modes name them by it: `X0` .. `X22`).
pub const COUNTERS: [Counter; COUNT] = [
    counter(
        "Processor Time",
        "CPU Time",
        "%CPU",
        Kind::Duration,
        Rate::Timer,
    ),
    counter(
        "Privileged Time",
        "Privil Time",
        "%Privil",
        Kind::Duration,
        Rate::Timer,
    ),
    counter(
        "User Time",
        "User Time",
        "%Usr",
        Kind::Duration,
        Rate::Timer,
    ),
    counter(
        "Handle Count",
        "Handles",
        "Handles",
        Kind::Number,
        Rate::Raw,
    ),
    counter(
        "Page File Bytes",
        "Page File",
        "Page File",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Page File Bytes Peak",
        "Page File Peak",
        "Page File Peak",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Working Set",
        "Mem Usage",
        "Mem Usage",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Working Set Peak",
        "Peak Mem Usage",
        "Peak Mem Usage",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Pool Nonpaged Bytes",
        "NP Pool",
        "NP Pool",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Pool Paged Bytes",
        "Paged Pool",
        "Paged Pool",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Private Bytes",
        "Private Bytes",
        "Private Bytes",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Page Faults",
        "PgFaults",
        "PgFt/s",
        Kind::Number,
        Rate::PerSecond,
    ),
    counter(
        "Virtual Bytes",
        "VM Size",
        "VM Size",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "Virtual Bytes Peak",
        "VM Peak",
        "VM Peak",
        Kind::Bytes,
        Rate::Raw,
    ),
    counter(
        "IO Data Bytes",
        "IO Bytes",
        "Byt/s",
        Kind::Bytes,
        Rate::PerSecond,
    ),
    counter(
        "IO Read Bytes",
        "ReadBytes",
        "RdB/s",
        Kind::Bytes,
        Rate::PerSecond,
    ),
    counter(
        "IO Write Bytes",
        "WritBytes",
        "WrB/s",
        Kind::Bytes,
        Rate::PerSecond,
    ),
    counter(
        "IO Other Bytes",
        "OthrBytes",
        "OthB/s",
        Kind::Bytes,
        Rate::PerSecond,
    ),
    counter(
        "IO Data Operations",
        "IO Oper",
        "IO/s",
        Kind::Number,
        Rate::PerSecond,
    ),
    counter(
        "IO Read Operations",
        "Reads",
        "Rd/s",
        Kind::Number,
        Rate::PerSecond,
    ),
    counter(
        "IO Write Operations",
        "Writes",
        "Wr/s",
        Kind::Number,
        Rate::PerSecond,
    ),
    counter(
        "IO Other Operations",
        "OthrOps",
        "OthOp/s",
        Kind::Number,
        Rate::PerSecond,
    ),
    counter(
        "Working Set - Private",
        "Priv Mem Usage",
        "Priv Mem Usage",
        Kind::Bytes,
        Rate::Raw,
    ),
];

/// The working set's and the page file's counters (FAR's `IDX_WORKINGSET`, `IDX_PAGEFILE`): a process's
/// size is their sum.
pub const WORKING_SET: usize = 6;
pub const PAGE_FILE: usize = 4;

/// Whether FAR shows the counter per second when a mode asks (`can_be_per_sec`): a time or a count.
pub fn per_second(index: usize) -> bool {
    COUNTERS
        .get(index)
        .is_some_and(|counter| counter.kind != Kind::Bytes)
}

/// The counters' values in a process's record (FAR's `qwCounters`).
pub fn values(record: &ProcessRecord) -> [u64; COUNT] {
    [
        record.user_time + record.kernel_time,
        record.kernel_time,
        record.user_time,
        u64::from(record.handles),
        record.page_file,
        record.page_file_peak,
        record.working_set,
        record.working_set_peak,
        record.nonpaged_pool,
        record.paged_pool,
        record.private_bytes,
        record.page_faults,
        record.virtual_bytes,
        record.virtual_bytes_peak,
        record.read_bytes + record.write_bytes,
        record.read_bytes,
        record.write_bytes,
        record.other_bytes,
        record.read_operations + record.write_operations,
        record.read_operations,
        record.write_operations,
        record.other_operations,
        record.working_set_private,
    ]
}

/// What each counter shows between the read before (`old`) and this one, `elapsed_ms` apart, the
/// processor times shared by `processors` (FAR's `qwResults`); a counter that went down shows 0, a
/// process seen for the first time its raw counters only.
pub fn results(
    now: &[u64; COUNT],
    old: Option<&[u64; COUNT]>,
    elapsed_ms: u64,
    processors: u64,
) -> [u64; COUNT] {
    let mut shown = [0u64; COUNT];
    for (index, counter) in COUNTERS.iter().enumerate() {
        shown[index] = match (counter.rate, old) {
            (Rate::Raw, _) => now[index],
            (_, None) => 0,
            _ if elapsed_ms == 0 => 0,
            (Rate::Timer, Some(old)) => {
                now[index].saturating_sub(old[index]) / (elapsed_ms * 100) / processors.max(1)
            }
            (Rate::PerSecond, Some(old)) => {
                now[index].saturating_sub(old[index]) * 1000 / elapsed_ms
            }
        };
    }
    shown
}

#[cfg(test)]
#[path = "counters_tests.rs"]
mod tests;
