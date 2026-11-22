//! Secrets off Windows: no store of the system's yet (the Linux keyring is the port's, ADR-0023 §3,
//! `docs/tech-debt.md` [S7]): both calls fail as unsupported, so a plugin keeps no secret there.

use std::io;

pub fn protect(_data: &[u8], _entropy: &[u8]) -> io::Result<Vec<u8>> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

pub fn unprotect(_kept: &[u8], _entropy: &[u8]) -> io::Result<Vec<u8>> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}
