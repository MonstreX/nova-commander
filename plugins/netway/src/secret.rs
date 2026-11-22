//! Passwords as a session's file keeps them (ADR-0023 §3): protected for the user by the system (Windows'
//! DPAPI through the SDK, the plugin's own entropy), written as hex. NetBox keeps WinSCP's reversible
//! obfuscation instead (a documented difference: only the user who saved it reads it, on that computer).

use std::io;

use nc_plugin_sdk::system;

/// The plugin's entropy: another program protecting for the same user does not read its passwords.
// The program's first name (Voyager Commander), kept: the passwords stored under it open still.
const ENTROPY: &[u8] = b"voyager-commander NetWay session";

/// `password` as kept: the system's protected bytes as hex. An error where the system keeps no secrets
/// (off Windows, until the port's keyring): the password is not kept then.
pub fn keep(password: &str) -> io::Result<String> {
    system::protect(password.as_bytes(), ENTROPY).map(|bytes| hex(&bytes))
}

/// The password `kept` holds; an error when it is not hex, was protected by another user or on another
/// computer, or is not UTF-8.
pub fn read(kept: &str) -> io::Result<String> {
    let bytes = unhex(kept.trim()).ok_or_else(|| invalid("not a kept password"))?;
    let plain = system::unprotect(&bytes, ENTROPY)?;
    String::from_utf8(plain).map_err(|_| invalid("not a text"))
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
#[path = "secret_tests.rs"]
mod tests;
