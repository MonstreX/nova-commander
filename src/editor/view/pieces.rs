//! The editor's document (ADR-0015 §4): a piece table over the file's bytes as they are read - the
//! original stays on the disk, read by blocks as the screen needs them; what is typed goes to a buffer
//! of its own, in the file's code page, so the document is one sequence of bytes the view core lays out
//! as it does a file. Inserting and deleting split and drop pieces; undo and redo give the pieces back
//! (the buffer only grows). Saving streams the pieces in order: the original's from the disk.

use std::io::{self, Write};

use super::blocks::{Blocks, Bytes, Missing};
use crate::vfs::RandomRead;

/// Where a piece's bytes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Original,
    Added,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Piece {
    source: Source,
    start: u64,
    len: u64,
}

/// The pieces as they were before an edit, and where the cursor was then and after.
#[derive(Debug, Clone)]
struct Step {
    pieces: Vec<Piece>,
    len: u64,
    /// The place the edit was made at: undo and redo put the cursor back there.
    at: u64,
}

/// A change an attempt made to the undo and redo lists, undone in reverse by `roll_back`.
#[derive(Debug, Clone)]
enum Logged {
    PushedUndo,
    PoppedUndo(Step),
    PushedRedo,
    PoppedRedo(Step),
    ClearedRedo(Vec<Step>),
}

/// The table as an attempt found it at its first change, and the lists' changes since.
#[derive(Debug, Clone)]
struct Attempt {
    pieces: Vec<Piece>,
    len: u64,
    saved: Option<usize>,
    group: Option<bool>,
    groups: usize,
    log: Vec<Logged>,
}

#[derive(Debug, Default, Clone)]
pub struct PieceTable {
    original: Blocks,
    added: Vec<u8>,
    pieces: Vec<Piece>,
    /// The first bytes of the pieces up to the first one an edit changed since, made on by the reads
    /// that go past them (R7 review F2: a document of many pieces, as "All" leaves it, was looked
    /// through piece by piece at every read).
    starts: std::cell::RefCell<Vec<u64>>,
    len: u64,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The undo steps there were at the last save or the opening (FAR's `UndoSavePos`); none when
    /// that state was undone and edited past.
    saved: Option<usize>,
    /// Edits taken as one undo step (FAR's `undo_type::begin` / `end`): whether its step is kept.
    group: Option<bool>,
    /// The groups begun and not ended (one inside another is the outer one's).
    groups: usize,
    /// An editor's step under way (`begin_attempt`): none while it changed nothing yet.
    attempt: Option<Option<Box<Attempt>>>,
    /// The next insert is a character typed (`type_next`), and where the last one typed ended: one
    /// typed right after it joins its undo step (FAR's `AddUndoData` merging typing; review A2, A3).
    type_next: bool,
    typed_end: Option<u64>,
    /// The next group joins the last step when that one was a joined one too (`join_next`: the
    /// replacements of "All", FAR's `undo_block`), and whether the last step can be joined.
    join_next: bool,
    joinable: bool,
}

impl PieceTable {
    /// The document of a file `len` bytes long, none of it read yet.
    pub fn new(len: u64) -> Self {
        let mut table = Self {
            original: Blocks::new(len).keeping_whole(),
            len,
            saved: Some(0),
            ..Self::default()
        };
        if len > 0 {
            table.pieces.push(Piece {
                source: Source::Original,
                start: 0,
                len,
            });
        }
        table
    }

    /// A block of the file as read.
    pub fn insert_block(&mut self, index: u64, bytes: Vec<u8>) {
        self.original.insert(index, bytes);
    }

    /// The document's bytes now.
    pub fn size(&self) -> u64 {
        self.len
    }

    /// Whether it differs from the file as it was opened or saved (undone to it: not).
    pub fn is_changed(&self) -> bool {
        self.saved != Some(self.undo.len())
    }

    /// The runs of bytes in `from..to` typed since the file was opened or saved (the buffer's).
    pub fn added(&self, from: u64, to: u64) -> Vec<(u64, u64)> {
        let mut runs: Vec<(u64, u64)> = Vec::new();
        let mut start = 0;
        for piece in &self.pieces {
            if start >= to {
                break;
            }
            let end = start + piece.len;
            if piece.source == Source::Added && end > from {
                let run = (start.max(from), end.min(to));
                // Pieces typed one after another make one run.
                match runs.last_mut() {
                    Some(last) if last.1 == run.0 => last.1 = run.1,
                    _ => runs.push(run),
                }
            }
            start = end;
        }
        runs
    }

    /// The piece that holds `pos` (`pos` < len) and its first byte.
    /// The piece holding `pos` and its first byte, for a read: by the pieces' first bytes, those not
    /// known yet worked out as far as `pos`.
    fn piece_at_read(&self, pos: u64) -> Option<(usize, u64)> {
        let mut starts = self.starts.borrow_mut();
        loop {
            let known = starts.len();
            let end = match known {
                0 => 0,
                _ => starts[known - 1] + self.pieces[known - 1].len,
            };
            if known == self.pieces.len() || (known > 0 && end > pos) {
                break;
            }
            starts.push(end);
        }
        let index = starts
            .partition_point(|&start| start <= pos)
            .checked_sub(1)?;
        let start = starts[index];
        (pos < start + self.pieces[index].len).then_some((index, start))
    }

    /// The pieces from `index` on changed: their first bytes worked out again when read.
    fn reindex(&mut self, index: usize) {
        self.starts.get_mut().truncate(index);
    }

    /// The piece holding `pos` and its first byte, for an edit: looked for from the start (an edit
    /// changes the first bytes after it; "All"'s, the last first, find theirs at once).
    fn piece_at(&self, pos: u64) -> Option<(usize, u64)> {
        let mut start = 0;
        for (index, piece) in self.pieces.iter().enumerate() {
            if pos < start + piece.len {
                return Some((index, start));
            }
            start += piece.len;
        }
        None
    }

    /// The pieces split so one starts at `pos`; the index of that piece (the count at the end).
    fn split_at(&mut self, pos: u64) -> usize {
        let Some((index, start)) = self.piece_at(pos) else {
            return self.pieces.len();
        };
        if pos == start {
            return index;
        }
        let piece = self.pieces[index];
        let head = pos - start;
        self.reindex(index + 1);
        self.pieces[index].len = head;
        self.pieces.insert(
            index + 1,
            Piece {
                start: piece.start + head,
                len: piece.len - head,
                ..piece
            },
        );
        index + 1
    }

    /// An editor's step begins: what it changes can be rolled back (`roll_back`) when it stops for a
    /// block not read yet, so that its retry does not do it twice (ADR-0015 S2d.4b).
    pub fn begin_attempt(&mut self) {
        self.attempt = Some(None);
    }

    /// Whether the step under way changed the table.
    pub fn attempt_changed(&self) -> bool {
        matches!(self.attempt, Some(Some(_)))
    }

    /// The step is done: its changes stay.
    pub fn end_attempt(&mut self) {
        self.attempt = None;
    }

    /// The step stopped: the table as it was before it, the undo and redo lists too.
    pub fn roll_back(&mut self) {
        let Some(Some(attempt)) = self.attempt.take() else {
            return;
        };
        let attempt = *attempt;
        for logged in attempt.log.into_iter().rev() {
            match logged {
                Logged::PushedUndo => drop(self.undo.pop()),
                Logged::PoppedUndo(step) => self.undo.push(step),
                Logged::PushedRedo => drop(self.redo.pop()),
                Logged::PoppedRedo(step) => self.redo.push(step),
                Logged::ClearedRedo(steps) => self.redo = steps,
            }
        }
        (self.pieces, self.len) = (attempt.pieces, attempt.len);
        self.reindex(0);
        (self.saved, self.group, self.groups) = (attempt.saved, attempt.group, attempt.groups);
    }

    /// The table kept as it is before the attempt's first change.
    fn touch(&mut self) {
        if let Some(None) = self.attempt {
            self.attempt = Some(Some(Box::new(Attempt {
                pieces: self.pieces.clone(),
                len: self.len,
                saved: self.saved,
                group: self.group,
                groups: self.groups,
                log: Vec::new(),
            })));
        }
    }

    fn log(&mut self, logged: Logged) {
        if let Some(Some(attempt)) = self.attempt.as_mut() {
            attempt.log.push(logged);
        }
    }

    /// The document as it is, for reading on another thread: its pieces and bytes, no undo steps.
    pub fn reading_copy(&self) -> PieceTable {
        PieceTable {
            original: self.original.clone(),
            added: self.added.clone(),
            pieces: self.pieces.clone(),
            len: self.len,
            ..PieceTable::default()
        }
    }

    /// The pieces there are and the undo steps (tests).
    #[cfg(test)]
    pub fn counts(&self) -> (usize, usize) {
        (self.pieces.len(), self.undo.len())
    }

    /// The next insert is a character typed at the cursor: right after the one typed before it, it
    /// joins that one's undo step.
    pub fn type_next(&mut self) {
        self.type_next = true;
        self.joinable = false;
    }

    /// A step that is no typing (a move, another edit): the next character typed starts a new undo
    /// step (FAR's `FEDITOR_NEWUNDO`).
    pub fn end_typing(&mut self) {
        self.type_next = false;
        self.typed_end = None;
        self.joinable = false;
    }

    /// The next edits (a group) join the last step if it was joined too: the replacements of "All"
    /// one undo step, however the search between them ended.
    pub fn join_next(&mut self) {
        self.type_next = false;
        self.typed_end = None;
        self.join_next = true;
    }

    /// The edits from here to `end_group` undone and redone as one.
    pub fn begin_group(&mut self) {
        self.typed_end = None;
        self.touch();
        if self.groups == 0 {
            let join = std::mem::take(&mut self.join_next);
            let joins = join
                && self.joinable
                && self.saved != Some(self.undo.len())
                && !self.undo.is_empty();
            // `Some(true)`: its step is the last one already.
            self.group = Some(joins);
            self.joinable = join;
        }
        self.groups += 1;
    }

    pub fn end_group(&mut self) {
        self.touch();
        self.groups = self.groups.saturating_sub(1);
        if self.groups == 0 {
            self.group = None;
        }
    }

    fn remember(&mut self, at: u64) {
        self.touch();
        match self.group {
            Some(true) => return,
            Some(false) => self.group = Some(true),
            None => {}
        }
        // The saved state among the steps redone is gone for good.
        if self.saved.is_some_and(|saved| saved > self.undo.len()) {
            self.saved = None;
        }
        self.undo.push(Step {
            pieces: self.pieces.clone(),
            len: self.len,
            at,
        });
        self.log(Logged::PushedUndo);
        if !self.redo.is_empty() {
            let redo = std::mem::take(&mut self.redo);
            self.log(Logged::ClearedRedo(redo));
        }
    }

    /// `bytes` put in at `pos` (in the document's code page).
    pub fn insert(&mut self, pos: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let pos = pos.min(self.len);
        let typed = std::mem::take(&mut self.type_next);
        // Typed right after the last one typed, the save not between: the same undo step.
        let joins = typed
            && self.typed_end == Some(pos)
            && self.group.is_none()
            && self.saved != Some(self.undo.len())
            && !self.undo.is_empty();
        if joins {
            self.touch();
        } else {
            self.remember(pos);
        }
        let index = self.split_at(pos);
        let len = bytes.len() as u64;
        let start = self.added.len() as u64;
        self.added.extend_from_slice(bytes);
        // Right after the last bytes added (typing on): that piece grows, no new one (review A3: a
        // piece per key made each undo step's copy of the pieces grow with the typing).
        let grows = index > 0
            && matches!(self.pieces[index - 1], Piece { source: Source::Added, start: s, len: l } if s + l == start);
        if grows {
            self.reindex(index);
            self.pieces[index - 1].len += len;
        } else {
            self.reindex(index);
            self.pieces.insert(
                index,
                Piece {
                    source: Source::Added,
                    start,
                    len,
                },
            );
        }
        self.len += len;
        self.typed_end = typed.then_some(pos + len);
    }

    /// `count` bytes taken out from `pos`.
    pub fn delete(&mut self, pos: u64, count: u64) {
        let end = (pos + count).min(self.len);
        if pos >= end {
            return;
        }
        self.typed_end = None;
        self.remember(pos);
        let first = self.split_at(pos);
        let past = self.split_at(end);
        self.pieces.drain(first..past);
        self.reindex(first);
        self.len -= end - pos;
    }

    /// The last edit undone; where it was made.
    pub fn undo(&mut self) -> Option<u64> {
        (self.typed_end, self.joinable) = (None, false);
        self.touch();
        let step = self.undo.pop()?;
        if self.attempt.is_some() {
            self.log(Logged::PoppedUndo(step.clone()));
        }
        self.reindex(0);
        self.redo.push(Step {
            pieces: std::mem::replace(&mut self.pieces, step.pieces),
            len: std::mem::replace(&mut self.len, step.len),
            at: step.at,
        });
        self.log(Logged::PushedRedo);
        Some(step.at)
    }

    /// The last edit undone made again; where it was made.
    pub fn redo(&mut self) -> Option<u64> {
        (self.typed_end, self.joinable) = (None, false);
        self.touch();
        let step = self.redo.pop()?;
        if self.attempt.is_some() {
            self.log(Logged::PoppedRedo(step.clone()));
        }
        self.reindex(0);
        self.undo.push(Step {
            pieces: std::mem::replace(&mut self.pieces, step.pieces),
            len: std::mem::replace(&mut self.len, step.len),
            at: step.at,
        });
        self.log(Logged::PushedUndo);
        Some(step.at)
    }

    /// Whether every byte of the original the document has is read (kept whole, [`KEPT_WHOLE`]).
    ///
    /// [`KEPT_WHOLE`]: super::blocks::KEPT_WHOLE
    pub fn original_held(&self) -> bool {
        (self.pieces.iter())
            .filter(|piece| piece.source == Source::Original)
            .all(|piece| self.original.holds(piece.start, piece.start + piece.len))
    }

    /// The original's bytes at `at`, at most `want`, written to `out`: from what was read, else from
    /// `file` (its bytes on the disk); how many.
    fn copy_original(
        &self,
        (file, buffer): (&mut dyn RandomRead, &mut [u8]),
        out: &mut dyn Write,
        (at, want): (u64, usize),
    ) -> io::Result<usize> {
        if let Ok(read) = self.original.at(at)
            && !read.is_empty()
        {
            let count = read.len().min(want);
            out.write_all(&read[..count])?;
            return Ok(count);
        }
        let want = want.min(buffer.len());
        let read = file.read_at(at, &mut buffer[..want])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The file became shorter while it was edited",
            ));
        }
        out.write_all(&buffer[..read])?;
        Ok(read)
    }

    /// The document written to `out` in order, the original's pieces from what was read, else from
    /// `file` (its bytes on the disk); `copied` hears the bytes written so far and may stop it (false).
    pub fn write_to(
        &self,
        file: &mut dyn RandomRead,
        out: &mut dyn Write,
        copied: &mut dyn FnMut(u64) -> bool,
    ) -> io::Result<()> {
        let mut done = 0;
        let mut buffer = vec![0; 1 << 20];
        for piece in &self.pieces {
            match piece.source {
                Source::Added => {
                    let from = usize::try_from(piece.start).unwrap_or(usize::MAX);
                    let to = usize::try_from(piece.start + piece.len).unwrap_or(usize::MAX);
                    out.write_all(&self.added[from..to])?;
                    done += piece.len;
                    // Heard after the typed pieces too: the save's progress moves on through them.
                    if !copied(done) {
                        return Err(io::Error::new(io::ErrorKind::Interrupted, "Stopped"));
                    }
                }
                Source::Original => {
                    let mut at = piece.start;
                    while at < piece.start + piece.len {
                        let want =
                            usize::try_from(piece.start + piece.len - at).unwrap_or(usize::MAX);
                        let read =
                            self.copy_original((&mut *file, &mut buffer), &mut *out, (at, want))?;
                        at += read as u64;
                        done += read as u64;
                        if !copied(done) {
                            return Err(io::Error::new(io::ErrorKind::Interrupted, "Stopped"));
                        }
                    }
                }
            }
        }
        out.flush()
    }
}

impl Bytes for PieceTable {
    fn len(&self) -> u64 {
        self.len
    }

    fn at(&self, pos: u64) -> Result<&[u8], Missing> {
        let Some((index, start)) = self.piece_at_read(pos) else {
            return Ok(&[]);
        };
        let piece = self.pieces[index];
        let into = pos - start;
        let left = usize::try_from(piece.len - into).unwrap_or(usize::MAX);
        let bytes = match piece.source {
            Source::Original => self.original.at(piece.start + into)?,
            Source::Added => {
                let from = usize::try_from(piece.start + into).unwrap_or(usize::MAX);
                &self.added[from..]
            }
        };
        Ok(&bytes[..left.min(bytes.len())])
    }

    fn before(&self, pos: u64) -> Result<&[u8], Missing> {
        let Some((index, start)) = self.piece_at_read(pos - 1) else {
            return Ok(&[]);
        };
        let piece = self.pieces[index];
        let into = pos - start;
        let bytes = match piece.source {
            Source::Original => self.original.before(piece.start + into)?,
            Source::Added => {
                let to = usize::try_from(piece.start + into).unwrap_or(usize::MAX);
                &self.added[..to]
            }
        };
        let from = bytes
            .len()
            .saturating_sub(usize::try_from(into).unwrap_or(usize::MAX));
        Ok(&bytes[from..])
    }
}

#[path = "pieces_doc_file.rs"]
mod doc_file;
pub use doc_file::DocFile;

#[path = "pieces_rebase.rs"]
mod rebase;

#[cfg(test)]
#[path = "pieces_tests.rs"]
mod tests;
