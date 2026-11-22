//! What a transfer does to a file's bytes besides copying them: WinSCP's text mode (`TFileBuffer::Convert`
//! from the session's end of line, LF, to Windows' CRLF and back) piece by piece, and its speed limit
//! (`CPSLimit`: a piece waited after when the file went faster).

use std::time::{Duration, Instant};

/// The text mode's conversion of a file, across its pieces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextMode {
    /// A download's: LF as CRLF (a CRLF kept); an upload's: CRLF as LF.
    to_crlf: bool,
    /// The last byte given was CR (a download), or a CR is held until the next byte (an upload).
    after_cr: bool,
}

impl TextMode {
    pub fn download() -> Self {
        Self {
            to_crlf: true,
            after_cr: false,
        }
    }

    pub fn upload() -> Self {
        Self {
            to_crlf: false,
            after_cr: false,
        }
    }

    /// The bytes to write for the next piece `data`.
    pub fn convert(&mut self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() + data.len() / 16);
        for &byte in data {
            if self.to_crlf {
                if byte == b'\n' && !self.after_cr {
                    out.push(b'\r');
                }
                out.push(byte);
                self.after_cr = byte == b'\r';
                continue;
            }
            if self.after_cr && byte != b'\n' {
                out.push(b'\r');
            }
            self.after_cr = byte == b'\r';
            if !self.after_cr {
                out.push(byte);
            }
        }
        out
    }

    /// The bytes still held at the file's end: an upload's last CR.
    pub fn finish(&mut self) -> Vec<u8> {
        let held = !self.to_crlf && self.after_cr;
        self.after_cr = false;
        if held { vec![b'\r'] } else { Vec::new() }
    }
}

/// WinSCP's speed limit of a file: bytes a second, 0 none.
#[derive(Debug, Clone, Copy)]
pub struct Throttle {
    limit: u32,
    started: Instant,
    sent: u64,
}

impl Throttle {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            started: Instant::now(),
            sent: 0,
        }
    }

    /// `bytes` more sent: how long to wait so the file goes no faster than the limit.
    pub fn after(&mut self, bytes: u64) -> Duration {
        self.sent += bytes;
        if self.limit == 0 {
            return Duration::ZERO;
        }
        let due = Duration::from_secs_f64(self.sent as f64 / f64::from(self.limit));
        due.saturating_sub(self.started.elapsed())
    }
}

#[cfg(test)]
#[path = "text_mode_tests.rs"]
mod tests;
