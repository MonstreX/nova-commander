//! The document moved onto the file it was saved to, its undo and redo kept (FAR's editor keeps its
//! undo over a save; S2d.6e). The saved file is the document's bytes, so it becomes the original and
//! the document one piece of it; the steps' pieces of the old original go to where the saved file has
//! those bytes, and the bytes it has no more (text deleted before the save) are read from the old file
//! into the buffer while it is still there.

use std::io;

use super::{Blocks, Piece, PieceTable, Source, Step};
use crate::vfs::RandomRead;

/// Where a run of the old original's bytes is now: its first byte, its bytes, and the piece they make.
#[derive(Debug, Clone, Copy)]
struct Moved {
    old: u64,
    len: u64,
    to: Piece,
}

/// A part of a run of the old original: on a moved run, or in a gap none covers.
enum Part {
    Moved(Piece),
    Gap(u64, u64),
}

impl PieceTable {
    /// The table of the saved file with the steps kept; none when the bytes to read from `old` would be
    /// more than `keep` (the steps are then let go, the file read anew).
    pub fn rebased(&self, old: &mut dyn RandomRead, keep: u64) -> io::Result<Option<PieceTable>> {
        let mut moved = self.saved_runs();
        let mut gaps = Vec::new();
        for piece in self.step_pieces() {
            parts(&moved, piece.start, piece.len, &mut |part| {
                if let Part::Gap(at, len) = part {
                    gaps.push((at, len));
                }
            });
        }
        let gaps = merged(gaps);
        if gaps.iter().map(|gap| gap.1).sum::<u64>() > keep {
            return Ok(None);
        }
        let mut added = self.added.clone();
        for &(at, len) in &gaps {
            let start = added.len() as u64;
            read_into(old, at, len, &mut added)?;
            moved.push(Moved {
                old: at,
                len,
                to: Piece {
                    source: Source::Added,
                    start,
                    len,
                },
            });
        }
        moved.sort_unstable_by_key(|run| run.old);
        let step = |step: &Step| Step {
            pieces: remapped(&moved, &step.pieces),
            ..step.clone()
        };
        let mut table = PieceTable::new(self.len);
        table.added = added;
        table.undo = self.undo.iter().map(step).collect();
        table.redo = self.redo.iter().map(step).collect();
        table.saved = Some(table.undo.len());
        table.original = Blocks::new(self.len);
        Ok(Some(table))
    }

    /// The document's pieces of the original: where the saved file has their bytes.
    fn saved_runs(&self) -> Vec<Moved> {
        let mut runs = Vec::new();
        let mut at = 0;
        for piece in &self.pieces {
            if piece.source == Source::Original {
                runs.push(Moved {
                    old: piece.start,
                    len: piece.len,
                    to: Piece {
                        source: Source::Original,
                        start: at,
                        len: piece.len,
                    },
                });
            }
            at += piece.len;
        }
        runs.sort_unstable_by_key(|run| run.old);
        runs
    }

    /// The undo and redo steps' pieces of the old original.
    fn step_pieces(&self) -> impl Iterator<Item = &Piece> {
        (self.undo.iter().chain(&self.redo))
            .flat_map(|step| &step.pieces)
            .filter(|piece| piece.source == Source::Original)
    }
}

/// The run `start`, `len` of the old original cut by the moved runs (sorted by `old`).
fn parts(moved: &[Moved], start: u64, len: u64, each: &mut dyn FnMut(Part)) {
    let end = start + len;
    let mut pos = start;
    while pos < end {
        let next = moved.partition_point(|run| run.old <= pos);
        let on = next
            .checked_sub(1)
            .map(|index| moved[index])
            .filter(|run| pos < run.old + run.len);
        if let Some(run) = on {
            let take = end.min(run.old + run.len) - pos;
            each(Part::Moved(Piece {
                start: run.to.start + (pos - run.old),
                len: take,
                ..run.to
            }));
            pos += take;
        } else {
            let past = moved.get(next).map_or(end, |run| run.old.min(end));
            each(Part::Gap(pos, past - pos));
            pos = past;
        }
    }
}

/// The gaps sorted and joined where they meet or overlap.
fn merged(mut gaps: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    gaps.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::new();
    for (at, len) in gaps {
        match out.last_mut() {
            Some(last) if at <= last.0 + last.1 => last.1 = last.1.max(at + len - last.0),
            _ => out.push((at, len)),
        }
    }
    out
}

/// A step's pieces with the old original's ones where their bytes are now (every byte covered).
fn remapped(moved: &[Moved], pieces: &[Piece]) -> Vec<Piece> {
    let mut out = Vec::with_capacity(pieces.len());
    for piece in pieces {
        if piece.source == Source::Added {
            out.push(*piece);
            continue;
        }
        parts(moved, piece.start, piece.len, &mut |part| {
            if let Part::Moved(piece) = part {
                out.push(piece);
            }
        });
    }
    out
}

/// `len` bytes of `old` from `at` appended to `buffer`.
fn read_into(old: &mut dyn RandomRead, at: u64, len: u64, buffer: &mut Vec<u8>) -> io::Result<()> {
    let mut chunk = vec![0; usize::try_from(len.min(1 << 20)).unwrap_or(1 << 20)];
    let mut pos = at;
    while pos < at + len {
        let want = usize::try_from(at + len - pos)
            .unwrap_or(usize::MAX)
            .min(chunk.len());
        let read = old.read_at(pos, &mut chunk[..want])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The file became shorter while it was edited",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
        pos += read as u64;
    }
    Ok(())
}
