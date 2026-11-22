//! A stored session (NetBox's `TSessionData`, its Session tab's part in S6.4b): the protocol, the host and
//! port, the login, the private key, kept as a file of `Key=Value` lines (`store`). Keys the plugin does
//! not know (a later version's tabs) are kept as they were. The password is the plain text here: the store
//! protects it when it writes the file.

/// NetBox's "File protocol" list, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Protocol {
    #[default]
    Sftp,
    Scp,
    Ftp,
    WebDav,
    S3,
}

impl Protocol {
    pub const ALL: [Self; 5] = [Self::Sftp, Self::Scp, Self::Ftp, Self::WebDav, Self::S3];

    /// NetBox's label in the list.
    pub fn label(self) -> &'static str {
        match self {
            Self::Sftp => "SFTP",
            Self::Scp => "SCP",
            Self::Ftp => "FTP",
            Self::WebDav => "WebDAV",
            Self::S3 => "Amazon S3",
        }
    }

    /// Its name in a session's file and as a prefix (`sftp:`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Sftp => "sftp",
            Self::Scp => "scp",
            Self::Ftp => "ftp",
            Self::WebDav => "webdav",
            Self::S3 => "s3",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|protocol| protocol.key().eq_ignore_ascii_case(key))
    }

    /// SFTP and SCP: over SSH.
    pub fn is_ssh(self) -> bool {
        matches!(self, Self::Sftp | Self::Scp)
    }
}

/// NetBox's "Encryption" list (FTP, WebDAV, S3), in its order; S3 has only the first two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encryption {
    #[default]
    None,
    Implicit,
    Explicit,
}

impl Encryption {
    pub const ALL: [Self; 3] = [Self::None, Self::Implicit, Self::Explicit];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No encryption",
            Self::Implicit => "TLS/SSL Implicit encryption",
            Self::Explicit => "TLS/SSL Explicit encryption",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Implicit => "implicit",
            Self::Explicit => "explicit",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|encryption| encryption.key().eq_ignore_ascii_case(key))
    }
}

/// The ports NetBox switches between (WinSCP's `SshPortNumber` and the others).
pub const SSH_PORT: u16 = 22;
pub const FTP_PORT: u16 = 21;
pub const FTPS_IMPLICIT_PORT: u16 = 990;
pub const HTTP_PORT: u16 = 80;
pub const HTTPS_PORT: u16 = 443;
/// WinSCP's `S3HostName`: the host an S3 session starts with.
pub const S3_HOST: &str = "s3.amazonaws.com";

/// A session's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub protocol: Protocol,
    /// SFTP's "Allow SCP fallback".
    pub scp_fallback: bool,
    pub encryption: Encryption,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub private_key: String,
    /// The lines of keys this version does not know, as they were.
    pub other: Vec<(String, String)>,
}

impl Default for Session {
    /// WinSCP's new session: SFTP on port 22.
    fn default() -> Self {
        Self {
            protocol: Protocol::Sftp,
            scp_fallback: false,
            encryption: Encryption::None,
            host: String::new(),
            port: SSH_PORT,
            user: String::new(),
            password: String::new(),
            private_key: String::new(),
            other: Vec::new(),
        }
    }
}

/// The keys of a session's file.
pub mod keys {
    pub const PROTOCOL: &str = "Protocol";
    pub const SCP_FALLBACK: &str = "ScpFallback";
    pub const ENCRYPTION: &str = "Encryption";
    pub const HOST: &str = "Host";
    pub const PORT: &str = "Port";
    pub const USER: &str = "User";
    pub const PASSWORD: &str = "Password";
    pub const PRIVATE_KEY: &str = "PrivateKey";
}

impl Session {
    /// The other tabs' settings (S6.5g), kept among the other keys under WinSCP's names.
    pub fn options(&self) -> crate::session_options::Options {
        crate::session_options::Options::read(&self.other)
    }

    pub fn set_options(&mut self, options: &crate::session_options::Options) {
        self.other = options.merged(&self.other);
    }

    /// WinSCP's `GetDefaultSessionName`: `user@host`, or the host alone.
    pub fn default_name(&self) -> String {
        let host = self.host.trim();
        match self.user.trim() {
            "" => host.to_string(),
            user => format!("{user}@{host}"),
        }
    }

    /// The port NetBox's `TransferProtocolComboChange` switches to for the protocol and the encryption
    /// now chosen, from the port typed: one of another protocol's defaults changes, any other stays.
    pub fn port_for(protocol: Protocol, encryption: Encryption, port: u16) -> u16 {
        let (target, others): (u16, &[u16]) = match (protocol, encryption) {
            (Protocol::Sftp | Protocol::Scp, _) => (SSH_PORT, &[FTP_PORT]),
            (Protocol::Ftp, Encryption::Implicit) => (
                FTPS_IMPLICIT_PORT,
                &[SSH_PORT, FTP_PORT, HTTP_PORT, HTTPS_PORT],
            ),
            (Protocol::Ftp, _) => (
                FTP_PORT,
                &[SSH_PORT, FTPS_IMPLICIT_PORT, HTTP_PORT, HTTPS_PORT],
            ),
            (Protocol::WebDav, Encryption::None) => {
                (HTTP_PORT, &[FTP_PORT, FTPS_IMPLICIT_PORT, HTTPS_PORT])
            }
            (Protocol::WebDav, _) => (HTTPS_PORT, &[FTP_PORT, FTPS_IMPLICIT_PORT, HTTP_PORT]),
            (Protocol::S3, _) => (
                HTTPS_PORT,
                &[FTP_PORT, SSH_PORT, HTTP_PORT, FTPS_IMPLICIT_PORT],
            ),
        };
        if others.contains(&port) { target } else { port }
    }

    /// The session's file: `Key=Value` lines, the password as `kept_password` gives it (the store's
    /// protected text; none: not written), the keys not known after the others.
    pub fn text(&self, kept_password: Option<&str>) -> String {
        let mut lines = vec![
            (keys::PROTOCOL, self.protocol.key().to_string()),
            (keys::HOST, self.host.clone()),
            (keys::PORT, self.port.to_string()),
            (keys::USER, self.user.clone()),
        ];
        if let Some(kept) = kept_password {
            lines.push((keys::PASSWORD, kept.to_string()));
        }
        lines.push((keys::PRIVATE_KEY, self.private_key.clone()));
        lines.push((keys::SCP_FALLBACK, flag(self.scp_fallback).to_string()));
        lines.push((keys::ENCRYPTION, self.encryption.key().to_string()));
        let known = lines.iter().map(|(key, value)| format!("{key}={value}\n"));
        let other = (self.other.iter()).map(|(key, value)| format!("{key}={value}\n"));
        known.chain(other).collect()
    }

    /// A session read from its file's `text`, with the password's kept text (none: no password kept).
    /// Lines without `=` and comments (`;`, `#`) are passed by; a value not understood leaves the default.
    pub fn parse(text: &str) -> (Self, Option<String>) {
        let mut session = Self::default();
        let mut kept = None;
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}');
            if line.starts_with([';', '#']) {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key == keys::PASSWORD {
                kept = Some(value.to_string());
            } else {
                session.read(key.trim(), value);
            }
        }
        (session, kept)
    }

    /// Whether `text` is a session's file (NetBox's "contains session data"): a line naming a protocol
    /// it knows, as every file it writes has.
    pub fn is_session_text(text: &str) -> bool {
        (text.lines().map(|line| line.trim_start_matches('\u{feff}')))
            .filter_map(|line| line.split_once('='))
            .any(|(key, value)| {
                key.trim() == keys::PROTOCOL && Protocol::from_key(value.trim()).is_some()
            })
    }

    /// One line's value taken.
    fn read(&mut self, key: &str, value: &str) {
        match key {
            keys::PROTOCOL => self.protocol = Protocol::from_key(value.trim()).unwrap_or_default(),
            keys::SCP_FALLBACK => self.scp_fallback = value.trim() == "1",
            keys::ENCRYPTION => {
                self.encryption = Encryption::from_key(value.trim()).unwrap_or_default();
            }
            keys::HOST => self.host = value.to_string(),
            keys::PORT => self.port = value.trim().parse().unwrap_or(self.port),
            keys::USER => self.user = value.to_string(),
            keys::PRIVATE_KEY => self.private_key = value.to_string(),
            _ => self.other.push((key.to_string(), value.to_string())),
        }
    }
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

/// The characters a session's or a folder's name cannot have: it is a file's name on the disk.
pub const RESTRICTED: &[char] = &['<', '>', ':', '"', '\\', '|', '?', '*'];

/// Why `name` (a folder's or a session's, without `/`) cannot be one; none: it can.
pub fn name_error(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("The session name can not be empty".to_string());
    }
    let bad = name.contains(RESTRICTED)
        || name.contains('/')
        || name.chars().any(char::is_control)
        || name.ends_with(['.', ' '])
        || name == "..";
    bad.then(|| "The session name contains restricted symbols (<>:\"/\\|?*)".to_string())
}

/// Why the path `name` (folders and the session's name joined by `/`) cannot be one; none: it can.
pub fn path_error(name: &str) -> Option<String> {
    name.split('/').find_map(name_error)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
