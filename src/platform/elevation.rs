//! The elevated helper's OS side (ADR-0007): the helper process started with the administrator's
//! rights and the named pipe the program talks to it over. What goes through the pipe is
//! `vfs::local::elevated`'s business.

pub use super::os::{
    HelperProcess, PipeListener, PipeStream, can_elevate, connect_pipe, decode_path, encode_path,
    needs_rights, new_pipe_name, start_helper,
};

/// Who may open a pipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeAccess {
    /// Only Administrators (FAR's `create_named_pipe`): what the program and its helper use.
    Administrators,
    /// The current user: tests run unelevated and cannot open the real kind.
    #[cfg(test)]
    CurrentUser,
}
