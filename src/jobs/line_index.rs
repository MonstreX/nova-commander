//! A file's line index (ADR-0015 addendum 2026-11-13): a thread of its own reads the file in order in
//! pieces of 4 MB and finds the breaks with a vectorised byte search (`memchr`), so the disk sets the
//! pace; it keeps the start of every 1024th line only (under 1 MB for 100 million lines). Line N is
//! its checkpoint and at most 1023 lines counted on from it in a block or two. UTF-16's breaks are
//! looked for at whole units; a lone `\r` breaks a line, `\r\n` across two pieces is one break. Once at
//! the end it looks again every second: a file that grows has its index extended from where it was.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::vfs::{VfsPath, provider_for};

/// A checkpoint every so many lines.
pub const EVERY: u64 = 1024;
/// The pieces the file is read in (even: UTF-16's units stay whole).
const PIECE: usize = 4 << 20;
/// How often the end is looked at again.
const LOOK_AGAIN: Duration = Duration::from_secs(1);
const LF: u32 = b'\n' as u32;
const CR: u32 = b'\r' as u32;

/// How a line break is written: the unit's bytes and their order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    Bytes,
    Utf16Le,
    Utf16Be,
}

impl Units {
    fn size(self) -> usize {
        match self {
            Self::Bytes => 1,
            _ => 2,
        }
    }
}

/// The breaks of a file fed in pieces: where each next line starts.
#[derive(Debug, Clone)]
struct Breaks {
    units: Units,
    /// A `\r` ended the last piece: the line after it starts past it, or past a `\n` that follows.
    pending: Option<u64>,
    /// A `\r` ended the file and was counted: a `\n` the file grows with is its pair, no break.
    ended_cr: bool,
    /// That `\n` came: the line counted after the `\r` starts past it, not at it (review A8).
    paired: bool,
}

impl Breaks {
    fn new(units: Units) -> Self {
        Self {
            units,
            pending: None,
            ended_cr: false,
            paired: false,
        }
    }

    fn unit(&self, bytes: &[u8], at: usize) -> u32 {
        match self.units {
            Units::Bytes => u32::from(bytes[at]),
            Units::Utf16Le => u32::from(u16::from_le_bytes([bytes[at], bytes[at + 1]])),
            Units::Utf16Be => u32::from(u16::from_be_bytes([bytes[at], bytes[at + 1]])),
        }
    }

    /// The next break's unit from `from`: its place and value.
    fn next_break(&self, bytes: &[u8], from: usize) -> Option<(usize, u32)> {
        match self.units {
            Units::Bytes => {
                let at = from + memchr::memchr2(b'\n', b'\r', &bytes[from..])?;
                Some((at, u32::from(bytes[at])))
            }
            _ => (from..bytes.len().saturating_sub(1))
                .step_by(2)
                .map(|at| (at, self.unit(bytes, at)))
                .find(|(_, unit)| *unit == LF || *unit == CR),
        }
    }

    /// `bytes` at `base` (whole units): `found` hears each next line's start and may stop the feed
    /// (false); true when it went through.
    fn feed(&mut self, base: u64, bytes: &[u8], found: &mut dyn FnMut(u64) -> bool) -> bool {
        let size = self.units.size();
        let mut from = 0;
        if std::mem::take(&mut self.ended_cr) && bytes.len() >= size && self.unit(bytes, 0) == LF {
            from = size;
            self.paired = true;
        }
        if let Some(after) = self.pending.take() {
            let lf = bytes.len() >= size && self.unit(bytes, 0) == LF;
            if lf {
                from = size;
            }
            let start = if lf { base + size as u64 } else { after };
            if !found(start) {
                return false;
            }
        }
        while let Some((at, unit)) = self.next_break(bytes, from) {
            let past = at + size;
            if unit == CR && past + size > bytes.len() {
                // Its `\n` may begin the next piece.
                self.pending = Some(base + past as u64);
                return true;
            }
            let start = if unit == CR && self.unit(bytes, past) == LF {
                past + size
            } else {
                past
            };
            if !found(base + start as u64) {
                return false;
            }
            from = start;
        }
        true
    }

    /// The file's end: a `\r` that ended it ends its line.
    fn finish(&mut self, found: &mut dyn FnMut(u64) -> bool) {
        if let Some(after) = self.pending.take() {
            self.ended_cr = true;
            found(after);
        }
    }
}

#[derive(Debug, Default)]
struct State {
    /// The start of line 0, 1024, 2048, ...
    checkpoints: Vec<u64>,
    /// The breaks counted: the lines known to start.
    lines: u64,
    /// The bytes read.
    scanned: u64,
    /// At the end as far as it was read.
    done: bool,
    failed: Option<String>,
}

#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    stop: AtomicBool,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The count going on; dropping it stops its thread.
#[derive(Debug)]
pub struct LineIndex {
    shared: Arc<Shared>,
    path: VfsPath,
    units: Units,
}

/// Why a line's place is not given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotFound {
    /// The file has fewer lines.
    Past,
    /// The wait was stopped.
    Stopped,
    Failed(String),
}

impl LineIndex {
    /// The count of `path`'s lines from `floor` (past a byte order mark), its breaks written as
    /// `units` say.
    pub fn start(path: VfsPath, units: Units, floor: u64) -> Self {
        let shared = Arc::new(Shared::default());
        shared.state().checkpoints.push(floor);
        let counting = Arc::clone(&shared);
        let read = path.clone();
        let started = std::thread::Builder::new()
            .name("line index".to_string())
            .spawn(move || count(&counting, &read, units, floor));
        if let Err(error) = started {
            shared.state().failed = Some(error.to_string());
        }
        Self {
            shared,
            path,
            units,
        }
    }

    /// Where line `line` (from 0) starts: waited for while the count goes on, until `stopped`.
    pub fn line_start(&self, line: u64, stopped: &dyn Fn() -> bool) -> Result<u64, NotFound> {
        let checkpoint = loop {
            let state = self.shared.state();
            if let Some(failed) = &state.failed {
                return Err(NotFound::Failed(failed.clone()));
            }
            if state.lines >= line {
                break state.checkpoints[usize::try_from(line / EVERY).unwrap_or(usize::MAX)];
            }
            if state.done {
                return Err(NotFound::Past);
            }
            if stopped() {
                return Err(NotFound::Stopped);
            }
            let wait = Duration::from_millis(20);
            drop(self.shared.wake.wait_timeout(state, wait));
        };
        self.count_on(checkpoint, line % EVERY)
            .map_err(|error| NotFound::Failed(error.to_string()))
    }

    /// The start of the line `more` breaks past `from`.
    fn count_on(&self, from: u64, more: u64) -> std::io::Result<u64> {
        if more == 0 {
            return Ok(from);
        }
        let mut file = provider_for(&self.path)?.open_random(&self.path)?;
        let mut breaks = Breaks::new(self.units);
        let mut piece = vec![0; PIECE.min(1 << 20)];
        let (mut at, mut left, mut start) = (from, more, from);
        loop {
            let read = file.read_at(at, &mut piece)?;
            let whole = read - read % self.units.size();
            if whole == 0 {
                breaks.finish(&mut |next| {
                    start = next;
                    true
                });
                return Ok(start);
            }
            let mut found = |next: u64| {
                start = next;
                left -= 1;
                left > 0
            };
            if !breaks.feed(at, &piece[..whole], &mut found) || left == 0 {
                return Ok(start);
            }
            at += whole as u64;
        }
    }
}

impl Drop for LineIndex {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
    }
}

/// The counting thread: the file read in order from `floor`, checkpoints kept, the end looked at
/// again every second.
fn count(shared: &Shared, path: &VfsPath, units: Units, floor: u64) {
    let opened = provider_for(path).and_then(|provider| provider.open_read(path));
    let mut file = match opened {
        Ok(file) => file,
        Err(error) => return shared.state().failed = Some(error.to_string()),
    };
    if floor > 0
        && let Err(error) = std::io::copy(&mut (&mut file).take(floor), &mut std::io::sink())
    {
        // Told, not left to wait for (review A8).
        return shared.state().failed = Some(error.to_string());
    }
    let mut breaks = Breaks::new(units);
    let mut piece = vec![0; PIECE];
    let mut at = floor;
    let mut carry = 0;
    while !shared.stop.load(Ordering::Relaxed) {
        let read = match file.read(&mut piece[carry..]) {
            Ok(read) => read,
            Err(error) => return shared.state().failed = Some(error.to_string()),
        };
        let held = carry + read;
        if read == 0 {
            end_reached(shared, &mut breaks, at);
            continue;
        }
        // A unit cut by the piece's end waits for the rest.
        let whole = held - held % units.size();
        let lines_before = shared.state().lines;
        let mut starts = Vec::new();
        breaks.feed(at, &piece[..whole], &mut |next| {
            starts.push(next);
            true
        });
        if std::mem::take(&mut breaks.paired) {
            moved_start(shared, at, at + units.size() as u64);
        }
        record(shared, lines_before, &starts, at + whole as u64);
        piece.copy_within(whole..held, 0);
        carry = held - whole;
        at += whole as u64;
    }
}

/// Lines found in a piece: counted, every 1024th start kept.
fn record(shared: &Shared, before: u64, starts: &[u64], scanned: u64) {
    let mut state = shared.state();
    for (number, start) in (before + 1..).zip(starts) {
        if number % EVERY == 0 {
            state.checkpoints.push(*start);
        }
    }
    state.lines = before + starts.len() as u64;
    state.scanned = scanned;
    state.done = false;
    shared.wake.notify_all();
}

/// The line counted as starting at `from` starts at `to`: its checkpoint, when it has one, moved.
fn moved_start(shared: &Shared, from: u64, to: u64) {
    let mut state = shared.state();
    if let Some(last) = state.checkpoints.last_mut().filter(|last| **last == from) {
        *last = to;
    }
}

/// At the end: a `\r` there ends its line, then a second's wait for the file to grow (or the stop).
fn end_reached(shared: &Shared, breaks: &mut Breaks, at: u64) {
    let mut last = None;
    breaks.finish(&mut |next| {
        last = Some(next);
        true
    });
    let before = shared.state().lines;
    if let Some(start) = last {
        record(shared, before, &[start], at);
    }
    let mut state = shared.state();
    state.done = true;
    shared.wake.notify_all();
    drop(shared.wake.wait_timeout(state, LOOK_AGAIN));
}

#[cfg(test)]
#[path = "line_index_tests.rs"]
mod tests;
