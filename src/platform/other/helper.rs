//! The elevated helper's stubs (ADR-0007): `sudo` is the port's decision.

use super::unsupported;
use std::io;
use std::path::{Path, PathBuf};

pub fn can_elevate() -> bool {
    false
}

pub fn needs_rights(_error: &io::Error) -> bool {
    false
}

pub fn new_pipe_name() -> String {
    String::new()
}

pub fn encode_path(_path: &Path) -> Vec<u8> {
    Vec::new()
}

pub fn decode_path(_bytes: &[u8]) -> Option<PathBuf> {
    None
}

#[derive(Debug)]
pub struct PipeListener;

impl PipeListener {
    pub fn create(
        _name: &str,
        _access: crate::platform::elevation::PipeAccess,
    ) -> io::Result<Self> {
        unsupported()
    }

    pub fn accept_while(
        self,
        _expected_client: u32,
        _timeout: std::time::Duration,
        _alive: &dyn Fn() -> bool,
    ) -> io::Result<PipeStream> {
        unsupported()
    }
}

#[derive(Debug)]
pub struct PipeStream;

impl io::Read for PipeStream {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }
}

impl io::Write for PipeStream {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        unsupported()
    }

    fn flush(&mut self) -> io::Result<()> {
        unsupported()
    }
}

pub fn connect_pipe(_name: &str, _server_pid: u32) -> io::Result<PipeStream> {
    unsupported()
}

#[derive(Debug)]
pub struct HelperProcess;

impl HelperProcess {
    pub fn process_id(&self) -> u32 {
        0
    }

    pub fn is_running(&self) -> bool {
        false
    }
}

pub fn start_helper(_arguments: &str) -> io::Result<HelperProcess> {
    unsupported()
}
