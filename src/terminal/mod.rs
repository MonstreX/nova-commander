//! Command execution (ADR-0006): the user screen where every command line runs as its own process,
//! the runner those processes are, the program's environment changes, and the guest shell (shell
//! profiles, a ConPTY session and its VT screen state).
//! This module has no UI dependencies; the UI renders `vt100::Screen` itself.

mod builtin;
mod environment;
pub mod mouse;
mod pty;
pub mod runner;
mod session;
mod shell;
mod shells;
mod user_screen;

pub use builtin::{Builtin, parse as parse_builtin};
pub use environment::Environment;
pub use runner::{StartMode, detached as detached_runner, failure_of as runner_failure};
pub use session::{Session, ShellView};
pub use shell::Dialect;
pub use shells::{Shell, available_shells};
pub use user_screen::UserScreen;

/// Renames `from` to `to`, trying again for up to 2 s: Windows releases the folder of a process that
/// has just exited a moment after its exit is reported.
#[cfg(test)]
pub fn rename_soon(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match std::fs::rename(from, to) {
            Err(_) if std::time::Instant::now() < deadline => std::thread::yield_now(),
            result => return result,
        }
    }
}
