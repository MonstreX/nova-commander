//! The OS layer (ADR-0004 §1, ADR-0012 §2): everything that depends on the operating system, split by
//! subsystem. Every public type is the same on every OS; the implementation is chosen at compile time:
//! `windows` now, stubs answering "unsupported" elsewhere (`other`) until the port, so the interface
//! stays portable (`cargo check --target x86_64-unknown-linux-gnu --all-targets`). This is the only
//! module with `unsafe` (CG-RS-002) and with `cfg(windows)`.

pub mod clipboard;
pub mod elevation;
pub mod files;
pub mod input;
pub mod libraries;
pub mod locale;
pub mod process;
pub mod system;
pub mod volumes;

mod date_text;

#[cfg(not(windows))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use other as os;
#[cfg(windows)]
use windows as os;

/// The system's line break: a new file's, the clipboard's lists', the program's own text files' (R7
/// review E10). Windows' `\r\n`; the port's `\n`.
#[cfg(windows)]
pub const LINE_BREAK: &str = "\r\n";
#[cfg(not(windows))]
pub const LINE_BREAK: &str = "\n";
