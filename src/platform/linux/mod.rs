//! Linux (ADR-0017, S7): the system's own calls where the port has got to (`rustix`, `libc` for what it
//! lacks: ADR-0029); everything else is still the stubs of `other`, taken whole and replaced subsystem
//! by subsystem (a stub replaced here is `cfg(not(target_os = "linux"))` there).

pub use super::other::*;

mod files;
mod text;
mod watch;

pub use files::{
    attributes, change_time, file_identity, name_count, open_file_identity, read_directory,
};
pub use text::{from_local_time, local_time};
pub use watch::watch_dir;
