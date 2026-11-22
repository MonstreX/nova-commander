//! A plugin's files opened: read from the start or anywhere in them, written at their end, each chunk a
//! call on the plugin's thread (ADR-0016 §13, 5c-2).

use std::io;

use super::ffi::{FileAnswer, FileCall, MOST_READ};
use super::provider::{Line, unexpected};
use crate::vfs::{RandomRead, VfsPath};

/// `size` bytes of `path` from `offset`.
fn read(line: &Line, path: &VfsPath, offset: u64, size: usize) -> io::Result<Vec<u8>> {
    let call = FileCall::Read {
        offset,
        size: size.min(MOST_READ),
    };
    match line.file(path, call)? {
        FileAnswer::Data(data) => Ok(data),
        _ => Err(unexpected()),
    }
}

/// A file read from its start.
pub(super) struct Reader {
    line: Line,
    path: VfsPath,
    offset: u64,
}

impl Reader {
    pub(super) fn new(line: &Line, path: &VfsPath) -> Self {
        Self {
            line: line.clone(),
            path: path.clone(),
            offset: 0,
        }
    }
}

impl io::Read for Reader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let data = read(&self.line, &self.path, self.offset, buffer.len())?;
        let count = data.len().min(buffer.len());
        buffer[..count].copy_from_slice(&data[..count]);
        self.offset += count as u64;
        Ok(count)
    }
}

/// A file read anywhere in it.
pub(super) struct Random {
    line: Line,
    path: VfsPath,
}

impl Random {
    pub(super) fn new(line: &Line, path: &VfsPath) -> Self {
        Self {
            line: line.clone(),
            path: path.clone(),
        }
    }
}

impl RandomRead for Random {
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let data = read(&self.line, &self.path, offset, buffer.len())?;
        let count = data.len().min(buffer.len());
        buffer[..count].copy_from_slice(&data[..count]);
        Ok(count)
    }

    fn len(&mut self) -> io::Result<u64> {
        match self.line.file(&self.path, FileCall::Stat)? {
            FileAnswer::Entry(entry) => Ok(entry.size),
            _ => Err(unexpected()),
        }
    }
}

/// A file written at its end: whole once flushed (the copy's last step), abandoned when let go before.
pub(super) struct Writer {
    line: Line,
    path: VfsPath,
    /// The plugin was told the file is whole.
    ended: bool,
}

impl Writer {
    pub(super) fn new(line: &Line, path: &VfsPath) -> Self {
        Self {
            line: line.clone(),
            path: path.clone(),
            ended: false,
        }
    }
}

impl io::Write for Writer {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let call = FileCall::Write {
            append: true,
            data: data.to_vec(),
        };
        self.line.file(&self.path, call)?;
        Ok(data.len())
    }

    /// The copy's last step (its stream at its end): the file is whole - the plugin told (API 9's
    /// `WRITE_END`), so one that streams it to a server finishes it there, its failure the copy's.
    fn flush(&mut self) -> io::Result<()> {
        if self.ended {
            return Ok(());
        }
        self.line.file(&self.path, FileCall::End)?;
        self.ended = true;
        Ok(())
    }
}

impl Drop for Writer {
    /// Let go never flushed: the copy failed or stopped - the plugin told (API 9's `WRITE_ABORT`), so
    /// what it kept is let go and never sent as the file; told even after a stop (its cleanup).
    fn drop(&mut self) {
        if !self.ended {
            let _ = crate::vfs::stop::unstopped(|| self.line.file(&self.path, FileCall::Abort));
        }
    }
}
