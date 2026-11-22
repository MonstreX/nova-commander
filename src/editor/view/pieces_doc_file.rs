//! The document read by offset on a worker (a search, the replace's "All"): its edits from the
//! table, what was read, the rest from the file.

use std::io;

use super::{PieceTable, Source};
use crate::vfs::RandomRead;

/// The document read by offset on a worker (a search): the edits from the table, the original's
/// bytes from the file on the disk.
pub struct DocFile {
    pub doc: PieceTable,
    pub original: Box<dyn RandomRead>,
}

impl RandomRead for DocFile {
    fn read_at(&mut self, pos: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let Some((index, start)) = self.doc.piece_at_read(pos) else {
            return Ok(0);
        };
        let piece = self.doc.pieces[index];
        let into = pos - start;
        let take = usize::try_from(piece.len - into)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        match piece.source {
            Source::Added => {
                let from = usize::try_from(piece.start + into).unwrap_or(usize::MAX);
                buffer[..take].copy_from_slice(&self.doc.added[from..from + take]);
                Ok(take)
            }
            // What was read first, as the save takes it (a file rewritten in place since is not mixed
            // in, R7 review F6), the file for the rest.
            Source::Original => match self.doc.original.at(piece.start + into) {
                Ok(read) if !read.is_empty() => {
                    let count = read.len().min(take);
                    buffer[..count].copy_from_slice(&read[..count]);
                    Ok(count)
                }
                _ => (self.original).read_at(piece.start + into, &mut buffer[..take]),
            },
        }
    }

    fn len(&mut self) -> io::Result<u64> {
        Ok(self.doc.len)
    }
}
