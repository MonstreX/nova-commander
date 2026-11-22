//! Two files' contents compared as FAR's compare does (`CompareFiles`): byte for byte in blocks, or byte for
//! byte ignoring the new line characters' differences (`\r\n`, `\r` and `\n` all one new line), or ignoring
//! white space (`isspace`: space, tab, new lines, vertical tab, form feed: all of it left out). Read in
//! blocks of at least 32 KiB and at most 64 MiB (FAR's `CompareBufferSize`), the stop asked at each.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// What is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ignore {
    Nothing,
    NewLines,
    Whitespace,
}

/// How a comparison of contents ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contents {
    Same,
    Different,
    /// A file could not be opened (FAR: "Some files couldn't been opened"; they count as different).
    NotOpened,
    Stopped,
}

/// FAR's smallest block.
pub const LEAST_BLOCK: usize = 32 * 1024;
/// The largest block taken from the settings (a bigger one could not be had in memory).
pub const MOST_BLOCK: usize = 64 * 1024 * 1024;

/// The files `a` and `b` compared in blocks of `block` bytes, `stop` asked at each.
pub fn compare(
    a: &Path,
    b: &Path,
    ignore: Ignore,
    block: usize,
    stop: &dyn Fn() -> bool,
) -> Contents {
    let (Ok(first), Ok(second)) = (File::open(a), File::open(b)) else {
        return Contents::NotOpened;
    };
    let block = block.clamp(LEAST_BLOCK, MOST_BLOCK);
    let same = match ignore {
        Ignore::Nothing => blocks(first, second, block, stop),
        _ => bytes(first, second, ignore, block, stop),
    };
    match same {
        Ok(Some(true)) => Contents::Same,
        Ok(Some(false)) | Err(_) => Contents::Different,
        Ok(None) => Contents::Stopped,
    }
}

/// As much of `reader` as fills `buffer` (less only at its end).
fn fill(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut done = 0;
    while done < buffer.len() {
        match reader.read(&mut buffer[done..])? {
            0 => break,
            read => done += read,
        }
    }
    Ok(done)
}

/// Byte for byte: whether the same; none when stopped.
fn blocks(
    mut a: File,
    mut b: File,
    block: usize,
    stop: &dyn Fn() -> bool,
) -> io::Result<Option<bool>> {
    let (mut first, mut second) = (vec![0u8; block], vec![0u8; block]);
    loop {
        if stop() {
            return Ok(None);
        }
        let (read_a, read_b) = (fill(&mut a, &mut first)?, fill(&mut b, &mut second)?);
        if read_a != read_b || first[..read_a] != second[..read_b] {
            return Ok(Some(false));
        }
        if read_a < block {
            return Ok(Some(true));
        }
    }
}

/// FAR's `isspace` in the C locale.
fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// What a file gives next as compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    Byte(u8),
    End,
    Stopped,
}

/// A file's bytes as compared (new lines as one `\n`, or no white space at all), read a block at a time
/// into its own buffer, the stop asked at each block.
struct Bytes<'a> {
    file: File,
    buffer: Vec<u8>,
    at: usize,
    length: usize,
    ignore: Ignore,
    /// A `\r` was given as `\n`: a `\n` right after it is the same new line.
    after_return: bool,
    stop: &'a dyn Fn() -> bool,
}

impl Bytes<'_> {
    fn raw(&mut self) -> io::Result<Next> {
        if self.at == self.length {
            if (self.stop)() {
                return Ok(Next::Stopped);
            }
            self.length = fill(&mut self.file, &mut self.buffer)?;
            self.at = 0;
            if self.length == 0 {
                return Ok(Next::End);
            }
        }
        let byte = self.buffer[self.at];
        self.at += 1;
        Ok(Next::Byte(byte))
    }

    fn next(&mut self) -> io::Result<Next> {
        loop {
            let byte = match self.raw()? {
                Next::Byte(byte) => byte,
                other => return Ok(other),
            };
            let after_return = std::mem::take(&mut self.after_return);
            match (self.ignore, byte) {
                (Ignore::Whitespace, byte) if is_space(byte) => {}
                (Ignore::NewLines, b'\n') if after_return => {}
                (Ignore::NewLines, b'\r') => {
                    self.after_return = true;
                    return Ok(Next::Byte(b'\n'));
                }
                (_, byte) => return Ok(Next::Byte(byte)),
            }
        }
    }
}

/// Byte for byte as `ignore` says: whether the same; none when stopped.
fn bytes(
    a: File,
    b: File,
    ignore: Ignore,
    block: usize,
    stop: &dyn Fn() -> bool,
) -> io::Result<Option<bool>> {
    let open = |file| Bytes {
        file,
        buffer: vec![0u8; block],
        at: 0,
        length: 0,
        ignore,
        after_return: false,
        stop,
    };
    let (mut first, mut second) = (open(a), open(b));
    loop {
        match (first.next()?, second.next()?) {
            (Next::Stopped, _) | (_, Next::Stopped) => return Ok(None),
            (Next::End, Next::End) => return Ok(Some(true)),
            (one, other) if one != other => return Ok(Some(false)),
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "contents_tests.rs"]
mod tests;
