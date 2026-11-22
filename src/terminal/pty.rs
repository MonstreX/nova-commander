//! A pseudo console (ConPTY) with its emulated screen: output is read on a thread and fed to a VT
//! parser; the console's cursor-position requests are answered. Shared by the user screen and the
//! guest shell.

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::num::NonZeroU16;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, MasterPty, PtySize, SlavePty, native_pty_system};

const SCROLLBACK_LINES: usize = 1000;
/// ConPTY asks for the cursor position (DSR) at start-up and waits for the answer.
const CURSOR_POSITION_REQUEST: &[u8] = b"\x1b[6n";

pub(super) struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    parser: vt100::Parser,
    output: Receiver<Vec<u8>>,
    last_output: Instant,
    closed: bool,
}

impl Pty {
    /// Opens a pseudo console; processes are started on the returned slave.
    pub(super) fn open(rows: u16, cols: u16) -> io::Result<(Self, Box<dyn SlavePty + Send>)> {
        let size = pty_size(rows, cols);
        let pair = native_pty_system().openpty(size).map_err(to_io)?;
        let reader = pair.master.try_clone_reader().map_err(to_io)?;
        let writer = pair.master.take_writer().map_err(to_io)?;
        let pty = Self {
            master: pair.master,
            writer,
            parser: vt100::Parser::new(non_zero(size.rows), non_zero(size.cols), SCROLLBACK_LINES),
            output: spawn_reader(reader),
            last_output: Instant::now(),
            closed: false,
        };
        Ok((pty, pair.slave))
    }

    pub(super) fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Feeds pending output into the screen. Returns true if anything arrived.
    pub(super) fn pump(&mut self) -> io::Result<bool> {
        let mut received = false;
        loop {
            match self.output.try_recv() {
                Ok(chunk) => {
                    received = true;
                    self.receive(&chunk)?;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }
        Ok(received)
    }

    /// Waits up to `timeout` for output and feeds it. Returns false when none arrived.
    #[cfg(test)]
    pub(super) fn pump_waiting(&mut self, timeout: Duration) -> io::Result<bool> {
        match self.output.recv_timeout(timeout) {
            Ok(chunk) => {
                self.receive(&chunk)?;
                Ok(true)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(false),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.closed = true;
                Ok(false)
            }
        }
    }

    fn receive(&mut self, chunk: &[u8]) -> io::Result<()> {
        self.parser.process(chunk);
        self.last_output = Instant::now();
        if contains(chunk, CURSOR_POSITION_REQUEST) {
            let (row, col) = self.parser.screen().cursor_position();
            let answer = format!("\x1b[{};{}R", u32::from(row) + 1, u32::from(col) + 1);
            self.send(answer.as_bytes())?;
        }
        Ok(())
    }

    pub(super) fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    pub(super) fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        let size = pty_size(rows, cols);
        self.parser
            .set_size(non_zero(size.rows), non_zero(size.cols));
        self.master.resize(size).map_err(to_io)
    }

    /// Whether output arrived after `instant`.
    pub(super) fn output_since(&self, instant: Instant) -> bool {
        self.last_output > instant
    }

    pub(super) fn quiet_for(&self, duration: Duration) -> bool {
        self.last_output.elapsed() >= duration
    }

    /// True once the console's output ended (its last process is gone and the slave is closed).
    pub(super) fn is_closed(&self) -> bool {
        self.closed
    }
}

fn spawn_reader(mut reader: Box<dyn Read + Send>) -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    // Without a thread the sender goes with the closure: the output reads as ended (review R24).
    let _ = thread::Builder::new()
        .name("console output".to_string())
        .spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if sender.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    receiver
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn non_zero(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN)
}

/// A command for `program` with exactly this process's environment, as `std::process::Command` gives.
/// portable-pty's `CommandBuilder::new` lays the variables stored in the registry over it, which would
/// lose a `PATH` or variables the program was started with (a developer prompt, a venv).
pub(super) fn command(program: impl AsRef<OsStr>) -> CommandBuilder {
    let mut command = CommandBuilder::new(program);
    command.env_clear();
    for (name, value) in std::env::vars_os() {
        command.env(name, value);
    }
    command
}

pub(super) fn to_io(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cursor_position_request() {
        assert!(contains(b"ab\x1b[6ncd", CURSOR_POSITION_REQUEST));
        assert!(!contains(b"\x1b[6", CURSOR_POSITION_REQUEST));
    }

    #[test]
    fn commands_get_this_process_environment_not_the_registry_one() {
        let built = command("cmd.exe");
        // cargo puts its own folders into the test's PATH; the registry's PATH has none of them.
        assert_eq!(built.get_env("PATH"), std::env::var_os("PATH").as_deref());
        assert_eq!(
            built.get_env("CARGO_PKG_NAME"),
            Some(OsStr::new("nova-commander"))
        );
        assert_eq!(
            built.iter_full_env_as_str().count(),
            std::env::vars().count()
        );
    }
}
