//! The session dialog's SSH, KEX and Authentication tabs' settings (S6.5i) under WinSCP's names and
//! defaults: compression, the ciphers' and the key exchanges' order (WinSCP's names, "WARN" the line below
//! which PuTTY warns), the rekey limits, bypassing authentication, Pageant, keyboard-interactive and its
//! password; and what `russh` is given of them (its preferred algorithms and limits).

use std::borrow::Cow;
use std::time::Duration;

use russh::{Preferred, cipher, compression, kex};

/// WinSCP's `CipherNames` with NetBox's labels.
pub const CIPHERS: [(&str, &str); 8] = [
    ("aes", "AES"),
    ("chacha20", "ChaCha20"),
    ("aesgcm", "AES-GCM"),
    ("3des", "3DES"),
    ("WARN", "-- warn below here --"),
    ("des", "DES"),
    ("blowfish", "Blowfish"),
    ("arcfour", "Arcfour"),
];

/// WinSCP's `KexNames` with NetBox's labels, in WinSCP's default order.
pub const KEXES: [(&str, &str); 13] = [
    ("ntru-curve25519", "NTRU Prime / Curve25519 hybrid kex"),
    ("mlkem-curve25519", "ML-KEM / Curve25519 hybrid kex"),
    ("mlkem-nist", "ML-KEM / NIST ECDH hybrid kex"),
    ("ecdh", "ECDH key exchange"),
    ("dh-gex-sha1", "Diffie-Hellman group exchange"),
    ("dh-group18-sha512", "Diffie-Hellman group 18 (8192-bit)"),
    ("dh-group17-sha512", "Diffie-Hellman group 17 (6144-bit)"),
    ("dh-group16-sha512", "Diffie-Hellman group 16 (4096-bit)"),
    ("dh-group15-sha512", "Diffie-Hellman group 15 (3072-bit)"),
    ("dh-group14-sha1", "Diffie-Hellman group 14 (2048-bit)"),
    ("rsa", "RSA-based key exchange"),
    ("WARN", "-- warn below here --"),
    ("dh-group1-sha1", "Diffie-Hellman group 1 (1024-bit)"),
];

/// The settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ssh {
    pub compression: bool,
    /// WinSCP's cipher names in their order.
    pub ciphers: Vec<String>,
    pub kexes: Vec<String>,
    /// `RekeyTime` in minutes, 0: none.
    pub rekey_time: u32,
    /// `RekeyBytes` as WinSCP writes it ("1G", "500M", "0").
    pub rekey_data: String,
    pub no_user_auth: bool,
    pub try_agent: bool,
    pub keyboard_interactive: bool,
    pub ki_password: bool,
}

fn names(table: &[(&str, &str)]) -> Vec<String> {
    table.iter().map(|(name, _)| name.to_string()).collect()
}

impl Default for Ssh {
    fn default() -> Self {
        Self {
            compression: false,
            ciphers: names(&CIPHERS),
            kexes: names(&KEXES),
            rekey_time: 60,
            rekey_data: "1G".to_string(),
            no_user_auth: false,
            try_agent: true,
            keyboard_interactive: true,
            ki_password: true,
        }
    }
}

pub const KEYS: [&str; 9] = [
    "Compression",
    "Cipher",
    "KEX",
    "RekeyTime",
    "RekeyBytes",
    "SshNoUserAuth",
    "TryAgent",
    "AuthKI",
    "AuthKIPassword",
];

/// A list as WinSCP's `SetAlgoList` keeps it: the known names in the order given, each once, those left
/// out after them in the default order.
pub fn algo_list(text: &str, table: &[(&str, &str)]) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    for name in text.split(',').map(str::trim) {
        let known = table
            .iter()
            .any(|(known, _)| known.eq_ignore_ascii_case(name));
        if known && !list.iter().any(|kept| kept.eq_ignore_ascii_case(name)) {
            let (canonical, _) = table
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .copied()
                .unwrap_or_default();
            list.push(canonical.to_string());
        }
    }
    for (name, _) in table {
        if !list.iter().any(|kept| kept == name) {
            list.push(name.to_string());
        }
    }
    list
}

/// The label of `name` in `table`.
pub fn label(table: &[(&str, &str)], name: &str) -> String {
    (table.iter().find(|(known, _)| *known == name))
        .map_or_else(|| name.to_string(), |(_, label)| label.to_string())
}

fn flag_of(value: &str) -> Option<bool> {
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// WinSCP's `RekeyBytes` ("1G", "500M", "100K", a number of bytes) in bytes; none: not understood.
pub fn bytes_of(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, unit) = match text.char_indices().last() {
        Some((at, ch)) if ch.is_ascii_alphabetic() => (&text[..at], ch.to_ascii_uppercase()),
        _ => (text, 'B'),
    };
    let number: u64 = digits.trim().parse().ok()?;
    let factor: u64 = match unit {
        'B' => 1,
        'K' => 1 << 10,
        'M' => 1 << 20,
        'G' => 1 << 30,
        _ => return None,
    };
    number.checked_mul(factor)
}

impl Ssh {
    pub fn read(lines: &[(String, String)]) -> Self {
        let mut ssh = Self::default();
        for (key, value) in lines {
            match key.as_str() {
                "Compression" => set(&mut ssh.compression, flag_of(value)),
                "Cipher" => ssh.ciphers = algo_list(value, &CIPHERS),
                "KEX" => ssh.kexes = algo_list(value, &KEXES),
                "RekeyTime" => set(&mut ssh.rekey_time, value.trim().parse().ok()),
                "RekeyBytes" if bytes_of(value).is_some() => {
                    ssh.rekey_data = value.trim().to_string()
                }
                "SshNoUserAuth" => set(&mut ssh.no_user_auth, flag_of(value)),
                "TryAgent" => set(&mut ssh.try_agent, flag_of(value)),
                "AuthKI" => set(&mut ssh.keyboard_interactive, flag_of(value)),
                "AuthKIPassword" => set(&mut ssh.ki_password, flag_of(value)),
                _ => {}
            }
        }
        ssh
    }

    /// The lines of what differs from the default.
    pub fn lines(&self) -> Vec<(String, String)> {
        let base = Self::default();
        let flag = |value: bool| u8::from(value).to_string();
        let all = [
            (
                self.compression != base.compression,
                "Compression",
                flag(self.compression),
            ),
            (
                self.ciphers != base.ciphers,
                "Cipher",
                self.ciphers.join(","),
            ),
            (self.kexes != base.kexes, "KEX", self.kexes.join(",")),
            (
                self.rekey_time != base.rekey_time,
                "RekeyTime",
                self.rekey_time.to_string(),
            ),
            (
                self.rekey_data != base.rekey_data,
                "RekeyBytes",
                self.rekey_data.clone(),
            ),
            (self.no_user_auth, "SshNoUserAuth", flag(true)),
            (!self.try_agent, "TryAgent", flag(false)),
            (!self.keyboard_interactive, "AuthKI", flag(false)),
            (!self.ki_password, "AuthKIPassword", flag(false)),
        ];
        (all.into_iter())
            .filter(|(differs, _, _)| *differs)
            .map(|(_, key, value)| (key.to_string(), value))
            .collect()
    }

    /// The algorithms `russh` is to offer, in the session's order (WinSCP's names the `russh` ones they
    /// stand for; those `russh` has none of left out, "WARN" passed by).
    pub fn preferred(&self) -> Preferred {
        let ciphers: Vec<cipher::Name> = (self.ciphers.iter())
            .flat_map(|name| russh_ciphers(name))
            .collect();
        let mut kexes: Vec<kex::Name> = (self.kexes.iter())
            .flat_map(|name| russh_kexes(name))
            .collect();
        // The extensions' pseudo-algorithms `russh` offers with its own.
        kexes.extend((Preferred::DEFAULT.kex.iter()).filter(|name| {
            let name = name.as_ref();
            name.starts_with("ext-info") || name.starts_with("kex-strict")
        }));
        let compressions: &[compression::Name] = match self.compression {
            true => &[
                compression::ZLIB_LEGACY,
                compression::ZLIB,
                compression::NONE,
            ],
            false => &[compression::NONE],
        };
        Preferred {
            cipher: Cow::Owned(ciphers),
            kex: Cow::Owned(kexes),
            compression: Cow::Owned(compressions.to_vec()),
            ..Preferred::DEFAULT
        }
    }

    /// The rekey limits (0: none; `russh` rekeys after 1 GB at most).
    pub fn limits(&self) -> russh::Limits {
        let most = 1usize << 30;
        let data = bytes_of(&self.rekey_data).unwrap_or(1 << 30);
        let data = match data {
            0 => most,
            data => usize::try_from(data).unwrap_or(most).min(most),
        };
        let time = match self.rekey_time {
            0 => Duration::from_secs(u64::from(u32::MAX)),
            minutes => Duration::from_secs(u64::from(minutes) * 60),
        };
        russh::Limits::new(data, data, time)
    }
}

/// The `russh` ciphers a WinSCP cipher name stands for (PuTTY's order within each).
fn russh_ciphers(name: &str) -> Vec<cipher::Name> {
    match name {
        "aes" => vec![
            cipher::AES_256_CTR,
            cipher::AES_256_CBC,
            cipher::AES_192_CTR,
            cipher::AES_192_CBC,
            cipher::AES_128_CTR,
            cipher::AES_128_CBC,
        ],
        "chacha20" => vec![cipher::CHACHA20_POLY1305],
        "aesgcm" => vec![cipher::AES_256_GCM, cipher::AES_128_GCM],
        _ => Vec::new(),
    }
}

/// The `russh` key exchanges a WinSCP name stands for.
fn russh_kexes(name: &str) -> Vec<kex::Name> {
    match name {
        "mlkem-curve25519" => vec![kex::MLKEM768X25519_SHA256],
        "ecdh" => vec![
            kex::CURVE25519,
            kex::CURVE25519_PRE_RFC_8731,
            kex::ECDH_SHA2_NISTP256,
            kex::ECDH_SHA2_NISTP384,
            kex::ECDH_SHA2_NISTP521,
        ],
        "dh-gex-sha1" => vec![kex::DH_GEX_SHA256, kex::DH_GEX_SHA1],
        "dh-group18-sha512" => vec![kex::DH_G18_SHA512],
        "dh-group17-sha512" => vec![kex::DH_G17_SHA512],
        "dh-group16-sha512" => vec![kex::DH_G16_SHA512],
        "dh-group15-sha512" => vec![kex::DH_G15_SHA512],
        "dh-group14-sha1" => vec![kex::DH_G14_SHA256, kex::DH_G14_SHA1],
        "dh-group1-sha1" => vec![kex::DH_G1_SHA1],
        _ => Vec::new(),
    }
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

#[cfg(test)]
#[path = "ssh_options_tests.rs"]
mod tests;
