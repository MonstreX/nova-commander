//! End-to-end smoke test of the portable program: copy the executable into an empty folder (that is the
//! whole installation), start it in a pseudo console as Windows Terminal would, check the classic screen,
//! quit with F10 + Enter, expect a clean exit and the settings file next to the copied executable.

use std::io::{self, Read, Write};
use std::num::NonZeroU16;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const ROWS: u16 = 30;
const COLS: u16 = 100;
const F10: &[u8] = b"\x1b[21~";
/// Put first into the program's PATH.
const PATH_MARKER: &str = "C:\\vc-smoke-path";
const TIMEOUT: Duration = Duration::from_secs(30);

struct Console {
    parser: vt100::Parser,
    output: mpsc::Receiver<Vec<u8>>,
    writer: Box<dyn Write + Send>,
}

impl Console {
    /// Blocks on the output channel until `text` is on screen; answers cursor position requests.
    /// Text already shown counts: an idle program sends nothing more.
    fn wait_for(&mut self, text: &str) -> io::Result<bool> {
        self.wait_until(|screen| screen.contents().contains(text))
    }

    /// Blocks until rows of the screen, one after another, are `lines` (trailing spaces aside).
    fn wait_for_rows(&mut self, lines: &[&str]) -> io::Result<bool> {
        self.wait_until(|screen| {
            let rows: Vec<String> = screen
                .rows(0, COLS)
                .map(|row| row.trim_end().to_string())
                .collect();
            rows.windows(lines.len()).any(|window| window == lines)
        })
    }

    fn wait_until(&mut self, shown: impl Fn(&vt100::Screen) -> bool) -> io::Result<bool> {
        if shown(self.parser.screen()) {
            return Ok(true);
        }
        let deadline = Instant::now() + TIMEOUT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(chunk) = self.output.recv_timeout(remaining) else {
                return Ok(false);
            };
            self.parser.process(&chunk);
            if chunk.windows(4).any(|window| window == b"\x1b[6n") {
                let (row, col) = self.parser.screen().cursor_position();
                self.send(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes())?;
            }
            if shown(self.parser.screen()) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }
}

/// The last record of an Alt code typing `ch` (Alt up carrying the character), in the pseudo
/// console's win32-input-mode: `ESC [ Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`.
fn alt_code(ch: char) -> String {
    format!("\x1b[18;56;{};0;0;1_", u32::from(ch))
}

fn install_portable_copy(folder: &Path) -> io::Result<std::path::PathBuf> {
    let exe = folder.join("nova-commander.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_nova-commander"), &exe)?;
    Ok(exe)
}

/// Command lines run through the program's own runner (`--run`) with the program's environment, and
/// every character typed reaches them; the panels come back afterwards.
fn run_commands(console: &mut Console) -> io::Result<()> {
    // A command line runs through the program's own runner (`--run`) with the program's environment:
    // `cmd /k` keeps the output shown until `exit` reaches it.
    console.send(b"cmd /k set PATH\r")?;
    assert!(
        console.wait_for(&format!("={PATH_MARKER};"))?,
        "the command did not run with the program's PATH:\n{}",
        console.parser.screen().contents()
    );
    // Characters the current keyboard layout lacks (`&`, `~`, `$` with a Russian one) come from the
    // pseudo console as Alt codes; they reach the running command too. Sent as the Alt up record
    // itself, so the check does not depend on the layout of the machine running it.
    let line: String = [
        "echo vc",
        &alt_code('&'),
        "echo ",
        &alt_code('~'),
        &alt_code('$'),
        "\r",
    ]
    .concat();
    console.send(line.as_bytes())?;
    assert!(
        console.wait_for_rows(&["vc", "~$"])?,
        "characters were lost on the way to the command:\n{}",
        console.parser.screen().contents()
    );
    console.send(b"exit\r")?;
    assert!(
        console.wait_for("10Quit")?,
        "panels did not come back after the command"
    );
    Ok(())
}

#[test]
fn portable_copy_starts_shows_panels_quits_and_saves_settings() {
    let install_dir = tempfile::tempdir().unwrap();
    let exe = install_portable_copy(install_dir.path()).unwrap();
    let start_dir = tempfile::tempdir().unwrap();
    std::fs::write(start_dir.path().join("smoke-marker.txt"), "x").unwrap();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(&exe);
    command.cwd(start_dir.path());
    // A PATH of this start only (not the one in the registry): commands must see it.
    let path = std::env::var("PATH").unwrap_or_default();
    command.env("PATH", format!("{PATH_MARKER};{path}"));
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let (exit_sender, exit) = mpsc::channel();
    thread::spawn(move || exit_sender.send(child.wait()));

    let mut reader = pair.master.try_clone_reader().unwrap();
    let (sender, output) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut console = Console {
        parser: vt100::Parser::new(
            NonZeroU16::new(ROWS).unwrap(),
            NonZeroU16::new(COLS).unwrap(),
            0,
        ),
        output,
        writer: pair.master.take_writer().unwrap(),
    };

    assert!(
        console.wait_for("smoke-marker.txt").unwrap(),
        "panels did not appear"
    );
    assert!(
        console.wait_for("10Quit").unwrap(),
        "key bar did not appear"
    );
    run_commands(&mut console).unwrap();
    console.send(F10).unwrap();
    assert!(
        console.wait_for("Do you want to quit").unwrap(),
        "quit dialog did not appear"
    );
    console.send(b"\r").unwrap();

    let status = exit.recv_timeout(TIMEOUT).unwrap().unwrap();
    assert!(status.success(), "exit status: {status:?}");
    let settings = install_dir.path().join("settings").join("settings.json");
    let saved = std::fs::read_to_string(settings).unwrap();
    assert!(saved.contains("\"terminal.shell\""));
}
