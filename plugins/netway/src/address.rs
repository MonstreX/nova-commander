//! The command line's prefixes (NetBox's `CommandPrefixes`, `OpenPluginEx` with `OPEN_COMMANDLINE` and
//! WinSCP's `TSessionData::ParseUrl`): `netway:` with nothing opens the panel of stored sessions;
//! `netway:name` names a stored session (by its name without its folders or its whole name, a folder of
//! the host after a `/`); `sftp://user:password@host:port/folder/file` and the other protocols' addresses
//! (after `netway:` too) a session made of the address. S6.5a connects to them.

use crate::session::{
    Encryption, FTP_PORT, FTPS_IMPLICIT_PORT, HTTP_PORT, HTTPS_PORT, Protocol, S3_HOST, SSH_PORT,
    Session,
};

/// The plugin's own prefix (NetBox's `netbox`).
pub const OWN_PREFIX: &str = "netway";

/// NetBox's `DefaultCommandPrefixes`, the plugin's name for NetBox's.
pub const PREFIXES: [&str; 7] = [OWN_PREFIX, "sftp", "scp", "ftp", "ftps", "webdav", "s3"];

/// What a command line names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The panel of stored sessions (an address with nothing in it: WinSCP's `DefaultsOnly`).
    Sessions,
    /// A session to connect to.
    Connect(Address),
}

/// A session named by a command line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Address {
    /// The stored session's whole name; none: a session made of the address.
    pub stored: Option<String>,
    /// The address's session (a stored one's settings are read when connecting).
    pub session: Session,
    /// The host's folder to open (`/var/log/`); empty: the session's own.
    pub folder: String,
    /// The file to put the cursor on: the address's last part without a `/` after it.
    pub file: String,
}

/// One of WinSCP's protocols in an address (`IsProtocolUrl`).
struct Scheme {
    name: &'static str,
    protocol: Protocol,
    encryption: Encryption,
    port: u16,
}

const fn scheme(
    name: &'static str,
    protocol: Protocol,
    encryption: Encryption,
    port: u16,
) -> Scheme {
    Scheme {
        name,
        protocol,
        encryption,
        port,
    }
}

/// WinSCP's protocols in `ParseUrl`'s order: `http` and `https` are WebDAV's, `ssh` is SFTP.
const SCHEMES: [Scheme; 13] = [
    scheme("scp", Protocol::Scp, Encryption::None, SSH_PORT),
    scheme("sftp", Protocol::Sftp, Encryption::None, SSH_PORT),
    scheme("ftp", Protocol::Ftp, Encryption::None, FTP_PORT),
    scheme(
        "ftps",
        Protocol::Ftp,
        Encryption::Implicit,
        FTPS_IMPLICIT_PORT,
    ),
    scheme("ftpes", Protocol::Ftp, Encryption::Explicit, FTP_PORT),
    scheme("dav", Protocol::WebDav, Encryption::None, HTTP_PORT),
    scheme("webdav", Protocol::WebDav, Encryption::None, HTTP_PORT),
    scheme("http", Protocol::WebDav, Encryption::None, HTTP_PORT),
    scheme("davs", Protocol::WebDav, Encryption::Implicit, HTTPS_PORT),
    scheme("https", Protocol::WebDav, Encryption::Implicit, HTTPS_PORT),
    scheme("s3plain", Protocol::S3, Encryption::None, HTTP_PORT),
    scheme("s3", Protocol::S3, Encryption::Implicit, HTTPS_PORT),
    scheme("ssh", Protocol::Sftp, Encryption::None, SSH_PORT),
];

/// The hosts a WebDAV address of which is S3's (WinSCP's `ParseUrl`).
const S3_DOMAINS: [&str; 4] = [
    S3_HOST,
    "digitaloceanspaces.com",
    "storage.googleapis.com",
    "r2.cloudflarestorage.com",
];

/// The command `command` typed after `prefix:`, with the stored sessions' whole names `stored` (sorted).
pub fn parse(prefix: &str, command: &str, stored: &[String]) -> Target {
    let command = unquoted(command.trim());
    let url = if prefix.eq_ignore_ascii_case(OWN_PREFIX) {
        command.strip_prefix("//").unwrap_or(command).to_string()
    } else {
        format!("{prefix}:{command}")
    };
    let (scheme, rest) = scheme_of(&url);
    let rest = match scheme {
        Some(_) => rest.strip_prefix("//").unwrap_or(rest),
        None => rest,
    };
    if rest.is_empty() {
        return Target::Sessions;
    }
    if let Some(mut address) = stored_session(rest, stored) {
        if let Some(scheme) = scheme {
            address.session.protocol = scheme.protocol;
        }
        return Target::Connect(address);
    }
    made_session(scheme, rest).map_or(Target::Sessions, Target::Connect)
}

/// A name in quotes (Ctrl+F's for a name with a space) without them.
fn unquoted(text: &str) -> &str {
    (text.strip_prefix('"'))
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(text)
}

/// The address's protocol (`sftp:`) and the rest after it; none: the whole is the rest.
fn scheme_of(url: &str) -> (Option<&'static Scheme>, &str) {
    for scheme in &SCHEMES {
        let length = scheme.name.len();
        let named = url
            .get(..length)
            .is_some_and(|start| start.eq_ignore_ascii_case(scheme.name));
        if named && url[length..].starts_with(':') {
            return (Some(scheme), &url[length + 1..]);
        }
    }
    (None, url)
}

/// The stored session `rest` names (WinSCP's lookup, the protocol given or not): its name without its
/// folders or its whole name, alone or with the host's folder after a `/`, whatever their case.
fn stored_session(rest: &str, stored: &[String]) -> Option<Address> {
    let decoded = decode(rest);
    stored.iter().find_map(|whole| {
        let local = whole.rsplit('/').next().unwrap_or(whole);
        let after = after_name(&decoded, local).or_else(|| after_name(&decoded, whole))?;
        let (folder, file) = folder_and_file(after);
        Some(Address {
            stored: Some(whole.clone()),
            session: Session::default(),
            folder,
            file,
        })
    })
}

/// What follows `name` at the start of `text` (whatever their case): nothing, or a `/` and the rest.
fn after_name<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.get(..name.len())?;
    let rest = &text[name.len()..];
    let named = start.to_lowercase() == name.to_lowercase();
    (named && (rest.is_empty() || rest.starts_with('/'))).then_some(rest)
}

/// The host's path split as WinSCP does: its folder (`/` at its end) and, without a `/` at the end, the
/// file after it; the root alone or nothing: neither.
fn folder_and_file(path: &str) -> (String, String) {
    let path = path.trim_end();
    if path.is_empty() || path == "/" {
        return (String::new(), String::new());
    }
    match path.rsplit_once('/') {
        Some((folder, file)) => (format!("{folder}/"), file.to_string()),
        None => (String::new(), path.to_string()),
    }
}

/// A session made of the address `rest` (WinSCP's ad hoc URL): `user:password@host:port/folder/`; none
/// when it names no host.
fn made_session(scheme: Option<&Scheme>, rest: &str) -> Option<Address> {
    let (connect, path) = rest.find('/').map_or((rest, ""), |at| rest.split_at(at));
    let (user_info, host_info) = connect.rsplit_once('@').unwrap_or(("", connect));
    let (host, port) = host_and_port(host_info);
    if host.is_empty() {
        return None;
    }
    let mut session = Session::default();
    if let Some(scheme) = scheme {
        session.protocol = scheme.protocol;
        session.encryption = scheme.encryption;
    }
    session.port = match (port, scheme) {
        (Some(port), _) => port,
        (None, Some(scheme)) => {
            if scheme.protocol == Protocol::WebDav && is_s3_host(&host) {
                session.protocol = Protocol::S3;
            }
            scheme.port
        }
        (None, None) => session.port,
    };
    session.host = host;
    // The connection's parameters after `;` (`;fingerprint=`) come with the SSH tabs.
    let user_info = user_info.split(';').next().unwrap_or_default();
    let (user, password) = user_info.split_once(':').unwrap_or((user_info, ""));
    session.user = decode(user);
    session.password = decode(password);
    let path = decode(path.split(';').next().unwrap_or_default());
    let (folder, file) = folder_and_file(&path);
    Some(Address {
        stored: None,
        session,
        folder,
        file,
    })
}

/// The host and the port of `host:port` or `[v6 address]:port`; a port not between 1 and 65535 is none.
fn host_and_port(info: &str) -> (String, Option<u16>) {
    let bracketed = (info.strip_prefix('['))
        .and_then(|inner| inner.split_once(']'))
        .map(|(host, rest)| (host.to_string(), rest.strip_prefix(':').unwrap_or(rest)));
    let (host, port) = bracketed.unwrap_or_else(|| {
        let (host, port) = info.split_once(':').unwrap_or((info, ""));
        (decode(host), port)
    });
    let port = decode(port).parse::<u16>().ok().filter(|port| *port > 0);
    (host, port)
}

/// WinSCP's `IsDomainOrSubdomain` for the S3 hosts.
fn is_s3_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    (S3_DOMAINS.iter()).any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// WinSCP's `DecodeUrlChars`: `+` a space, `%XX` runs their UTF-8 characters.
pub fn decode(text: &str) -> String {
    let mut bytes = Vec::with_capacity(text.len());
    let raw = text.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        let hex = (raw[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        match (raw[index], hex) {
            (_, Some(byte)) => {
                bytes.push(byte);
                index += 3;
            }
            (b'+', None) => {
                bytes.push(b' ');
                index += 1;
            }
            (byte, None) => {
                bytes.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
#[path = "address_tests.rs"]
mod tests;
