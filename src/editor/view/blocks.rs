//! The blocks of a file read so far (ADR-0015 §1): positions are byte offsets, a block is read when a
//! row on screen needs it, and only a bounded number are kept.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// The bytes of one block (a 64 KiB read, the size of FAR's viewer cache line on a fast disk).
pub const BLOCK: u64 = 64 * 1024;
/// Blocks kept: a screen of the longest lines fits several times over.
const KEPT: usize = 64;
/// An editor's document no longer than this keeps every block read (FAR keeps its edited file whole):
/// its save takes the bytes it did not change from what was read, not from a file another program may
/// have rewritten in place since (S2d hardening: the save mixed the two).
pub const KEPT_WHOLE: u64 = 64 * 1024 * 1024;

/// A block not read yet: its number (its offset over [`BLOCK`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Missing(pub u64);

/// A document's bytes by offset as far as they are read: a file's blocks, or a piece table over them
/// (the editor's edits). What is not read yet is named by the file's block it needs.
pub trait Bytes {
    fn len(&self) -> u64;
    /// The bytes from `pos` on, as far as they are held together; empty at the end.
    fn at(&self, pos: u64) -> Result<&[u8], Missing>;
    /// The bytes up to `pos` (`pos` > 0), as far back as they are held together.
    fn before(&self, pos: u64) -> Result<&[u8], Missing>;
}

#[derive(Debug, Default, Clone)]
pub struct Blocks {
    len: u64,
    read: HashMap<u64, Arc<[u8]>>,
    /// The order they came in: the first goes when there are too many.
    order: VecDeque<u64>,
    /// How many are kept, when more than [`KEPT`].
    keep: usize,
}

impl Blocks {
    pub fn new(len: u64) -> Self {
        Self {
            len,
            ..Self::default()
        }
    }

    /// Every block kept once read, the file being at most [`KEPT_WHOLE`] long.
    #[must_use]
    pub fn keeping_whole(mut self) -> Self {
        if self.len <= KEPT_WHOLE {
            self.keep = usize::try_from(self.len.div_ceil(BLOCK)).unwrap_or(usize::MAX);
        }
        self
    }

    /// Whether every byte from `from` to `to` is read.
    pub fn holds(&self, from: u64, to: u64) -> bool {
        let mut pos = from;
        while pos < to.min(self.len) {
            match self.at(pos) {
                Ok(bytes) if !bytes.is_empty() => pos += bytes.len() as u64,
                _ => return false,
            }
        }
        to <= self.len
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// The file is now `len` long: what was read stays when it only grew (a block that was short keeps
    /// showing what it has, its rest asked for as missing), nothing does when it shrank.
    pub fn set_len(&mut self, len: u64) {
        if len < self.len {
            self.read.clear();
            self.order.clear();
        }
        self.len = len;
    }

    /// Block `index` as read; the oldest goes when too many are kept.
    pub fn insert(&mut self, index: u64, bytes: Vec<u8>) {
        if self.read.insert(index, bytes.into()).is_none() {
            self.order.push_back(index);
        }
        while self.order.len() > self.keep.max(KEPT) {
            if let Some(oldest) = self.order.pop_front() {
                self.read.remove(&oldest);
            }
        }
    }

    /// Bytes written into the file over the blocks read (ADR-0027's byte editing): those held changed
    /// as well, the others read anew when asked for.
    pub fn patch(&mut self, changed: &std::collections::BTreeMap<u64, u8>) {
        for (&pos, &byte) in changed {
            let at = usize::try_from(pos % BLOCK).unwrap_or(usize::MAX);
            if let Some(bytes) = self.read.get_mut(&(pos / BLOCK))
                && at < bytes.len()
            {
                let mut copy = bytes.to_vec();
                copy[at] = byte;
                *bytes = copy.into();
            }
        }
    }

    /// The bytes from `pos` to the end of its block (or of the file); empty at the end.
    pub fn at(&self, pos: u64) -> Result<&[u8], Missing> {
        if pos >= self.len {
            return Ok(&[]);
        }
        let index = pos / BLOCK;
        let bytes = self.read.get(&index).ok_or(Missing(index))?;
        let from = usize::try_from(pos % BLOCK).unwrap_or(usize::MAX);
        let end = usize::try_from((self.len - index * BLOCK).min(BLOCK)).unwrap_or(usize::MAX);
        // A block read before the file grew is short: the rest is not read yet.
        bytes
            .get(from..end.min(bytes.len()))
            .filter(|rest| !rest.is_empty())
            .ok_or(Missing(index))
    }

    /// The bytes from the start of the block that holds `pos - 1` up to `pos` (`pos` > 0, at most the
    /// length).
    pub fn before(&self, pos: u64) -> Result<&[u8], Missing> {
        let pos = pos.min(self.len);
        let index = (pos - 1) / BLOCK;
        let bytes = self.read.get(&index).ok_or(Missing(index))?;
        let end = usize::try_from(pos - index * BLOCK).unwrap_or(usize::MAX);
        bytes.get(..end).ok_or(Missing(index))
    }
}

impl Bytes for Blocks {
    fn len(&self) -> u64 {
        Blocks::len(self)
    }

    fn at(&self, pos: u64) -> Result<&[u8], Missing> {
        Blocks::at(self, pos)
    }

    fn before(&self, pos: u64) -> Result<&[u8], Missing> {
        Blocks::before(self, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(index: u64, len: usize) -> Vec<u8> {
        (0..len).map(|at| (index as usize + at) as u8).collect()
    }

    #[test]
    fn bytes_come_from_their_block_to_its_end() {
        let mut blocks = Blocks::new(BLOCK + 10);
        assert_eq!(blocks.at(5), Err(Missing(0)));
        blocks.insert(0, block(0, BLOCK as usize));
        assert_eq!(blocks.at(5).unwrap().len(), BLOCK as usize - 5);
        assert_eq!(blocks.at(BLOCK + 2), Err(Missing(1)));
        blocks.insert(1, block(1, 10));
        assert_eq!(blocks.at(BLOCK + 2).unwrap().len(), 8);
        assert!(blocks.at(BLOCK + 10).unwrap().is_empty(), "the end");
        assert_eq!(blocks.before(BLOCK + 3).unwrap().len(), 3);
        assert_eq!(blocks.before(BLOCK).unwrap().len(), BLOCK as usize);
    }

    #[test]
    fn a_file_that_grew_reads_its_short_block_again() {
        let mut blocks = Blocks::new(10);
        blocks.insert(0, block(0, 10));
        blocks.set_len(20);
        assert_eq!(blocks.at(4).unwrap().len(), 6, "what it had stays shown");
        assert_eq!(blocks.at(12), Err(Missing(0)), "the rest is asked for");
        blocks.insert(0, block(0, 20));
        assert_eq!(blocks.at(12).unwrap().len(), 8);
    }

    #[test]
    fn a_block_read_short_leaves_the_rest_missing() {
        let mut blocks = Blocks::new(20);
        blocks.insert(0, block(0, 10));
        assert_eq!(blocks.at(4).unwrap().len(), 6);
        assert_eq!(blocks.at(12), Err(Missing(0)));
    }

    #[test]
    fn a_file_that_shrank_forgets_what_was_read() {
        let mut blocks = Blocks::new(30);
        blocks.insert(0, block(0, 30));
        blocks.set_len(5);
        assert_eq!(blocks.at(0), Err(Missing(0)));
    }

    #[test]
    fn a_document_kept_whole_keeps_every_block_read() {
        let count = KEPT as u64 + 1;
        let mut blocks = Blocks::new(count * BLOCK).keeping_whole();
        assert!(!blocks.holds(0, 1));
        for index in 0..count {
            blocks.insert(index, block(index, BLOCK as usize));
        }
        assert!(blocks.holds(0, count * BLOCK), "the first stays");
        assert!(!blocks.holds(0, count * BLOCK + 1), "past the end");
        let big = Blocks::new(KEPT_WHOLE + 1).keeping_whole();
        assert_eq!(big.keep, 0, "a bigger one keeps the usual number");
    }

    #[test]
    fn only_so_many_blocks_are_kept() {
        let count = KEPT as u64 + 1;
        let mut blocks = Blocks::new(count * BLOCK);
        for index in 0..count {
            blocks.insert(index, block(index, BLOCK as usize));
        }
        assert_eq!(blocks.at(0), Err(Missing(0)), "the first went");
        assert!(blocks.at(count * BLOCK - 1).is_ok());
    }
}
