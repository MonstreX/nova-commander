//! Every OS but Windows until the port (ADR-0012 §2): the same interface as `windows`, each call
//! answering "unsupported" or an empty value. Not an implementation - only the proof that the interface
//! stays portable (`cargo check --target x86_64-unknown-linux-gnu --all-targets`). What `std` already
//! does the same everywhere (a link's target, "hidden" as a leading dot) is done here already. Linux
//! takes these whole and puts its own in place of the ones it has ported (`platform/linux`); those
//! stubs are in `replaced`, left out there.

use std::io;

/// What a stub answers for a call it cannot make.
fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "not available on this system yet",
    ))
}

// Each subsystem's stubs.
mod clipboard;
mod files;
mod helper;
mod input;
mod libraries;
mod process;
mod properties;
#[cfg(not(target_os = "linux"))]
mod replaced;
mod text;
mod volumes;
pub use clipboard::*;
pub use files::*;
pub use helper::*;
pub use input::*;
pub use libraries::Library;
pub use process::*;
pub use properties::*;
#[cfg(not(target_os = "linux"))]
pub use replaced::*;
pub use text::*;
pub use volumes::*;
