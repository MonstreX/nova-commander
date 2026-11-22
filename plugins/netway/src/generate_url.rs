//! NetBox's "Generate URL" (`TGenerateUrlDialog`'s URL tab, WinSCP's `TSessionData::GenerateSessionUrl`): the
//! connected session's address - the protocol's scheme (`http(s)://` for WebDAV with "Save extension", as
//! NetBox's `sufHttpForWebDAV`), the user, the password, the host key (`;fingerprint=`, URL-safe base64),
//! the host, a port other than the protocol's default, the remote folder ("/" without it) - shown as it
//! changes, copied by "Copy to clipboard". NetBox's Script tab writes commands for its own console program,
//! which the program has not: left out; "Create session" and "Include raw settings" do nothing in NetBox
//! (WinSCP's raw settings are not built into it): shown disabled.

use nc_plugin_sdk::{Controls, Dialog, DialogEvent, DialogItem, Host, Value};

use crate::session::{
    Encryption, FTP_PORT, FTPS_IMPLICIT_PORT, HTTP_PORT, HTTPS_PORT, Protocol, SSH_PORT,
};

/// The session's parts of an address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parts {
    pub protocol: Protocol,
    pub encryption: Encryption,
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    /// The host key's SHA-256 fingerprint ("SHA256:base64").
    pub fingerprint: Option<String>,
    pub folder: String,
}

/// What the address includes (WinSCP's `sufUserName`, `sufPassword`, `sufHostKey`, `sufSpecific`,
/// `sufHttpForWebDAV`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Include {
    pub user: bool,
    pub password: bool,
    pub host_key: bool,
    pub folder: bool,
    pub http_for_webdav: bool,
}

/// WinSCP's `GetProtocolUrl`.
pub fn scheme(protocol: Protocol, encryption: Encryption, http_for_webdav: bool) -> &'static str {
    let implicit = encryption == Encryption::Implicit;
    match (protocol, encryption) {
        (Protocol::Sftp, _) => "sftp",
        (Protocol::Scp, _) => "scp",
        (Protocol::Ftp, Encryption::Implicit) => "ftps",
        (Protocol::Ftp, Encryption::Explicit) => "ftpes",
        (Protocol::Ftp, _) => "ftp",
        (Protocol::WebDav, _) if http_for_webdav => ["http", "https"][usize::from(implicit)],
        (Protocol::WebDav, _) => ["dav", "davs"][usize::from(implicit)],
        (Protocol::S3, _) => ["s3plain", "s3"][usize::from(implicit)],
    }
}

/// WinSCP's `GetDefaultPort`.
pub fn default_port(protocol: Protocol, encryption: Encryption) -> u16 {
    match (protocol, encryption) {
        (Protocol::Sftp | Protocol::Scp, _) => SSH_PORT,
        (Protocol::Ftp, Encryption::Implicit) => FTPS_IMPLICIT_PORT,
        (Protocol::Ftp, _) => FTP_PORT,
        (Protocol::WebDav | Protocol::S3, Encryption::None) => HTTP_PORT,
        (Protocol::WebDav | Protocol::S3, _) => HTTPS_PORT,
    }
}

/// WinSCP's `EncodeUrlString`: all but the unreserved characters percent-encoded.
fn encoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// WinSCP's `Base64ToUrlSafe` of a SHA-256 fingerprint ("SHA256:" left out).
fn url_safe(fingerprint: &str) -> String {
    let base64 = fingerprint.strip_prefix("SHA256:").unwrap_or(fingerprint);
    base64
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

/// WinSCP's `GenerateSessionUrl`.
pub fn url(parts: &Parts, include: Include) -> String {
    let scheme = scheme(parts.protocol, parts.encryption, include.http_for_webdav);
    let mut url = format!("{scheme}://");
    if include.user && !parts.user.is_empty() {
        url.push_str(&encoded(&parts.user));
        if include.password && !parts.password.is_empty() {
            url.push_str(&format!(":{}", encoded(&parts.password)));
        }
        if let Some(fingerprint) = parts.fingerprint.as_deref().filter(|_| include.host_key) {
            url.push_str(&format!(";fingerprint={}", url_safe(fingerprint)));
        }
        url.push('@');
    }
    match parts.host.contains(':') {
        true => url.push_str(&format!("[{}]", parts.host)),
        false => url.push_str(&encoded(&parts.host)),
    }
    if parts.port != default_port(parts.protocol, parts.encryption) {
        url.push_str(&format!(":{}", parts.port));
    }
    match (include.folder, parts.folder.trim()) {
        (true, folder) if !folder.is_empty() => {
            let folder = match folder.starts_with('/') {
                true => folder.to_string(),
                false => format!("/{folder}"),
            };
            url.push_str(&crate::webdav_xml::encoded(&folder));
            if !url.ends_with('/') {
                url.push('/');
            }
        }
        _ => url.push('/'),
    }
    url
}

// The dialog's items, in their order.
const USER: usize = 0;
const PASSWORD: usize = 1;
const HOST_KEY: usize = 2;
const FOLDER: usize = 3;
const SAVE_EXTENSION: usize = 6;
const RESULT: usize = 8;
const CLIPBOARD: usize = 9;

/// The checks' state as the dialog's `values` give it.
pub fn include_of(values: &[Value]) -> Include {
    let checked = |item: usize| values.get(item).is_some_and(Value::checked);
    Include {
        user: checked(USER),
        password: checked(PASSWORD),
        host_key: checked(HOST_KEY),
        folder: checked(FOLDER),
        http_for_webdav: checked(SAVE_EXTENSION),
    }
}

/// NetBox's URL tab: the checks, the result line read only, "Copy to clipboard", OK.
pub fn items(parts: &Parts) -> Vec<DialogItem> {
    let include = Include {
        user: true,
        ..Include::default()
    };
    vec![
        DialogItem::check("Include &username", include.user),
        DialogItem::check("Include &password", false).same_row(),
        DialogItem::check("Include &host key", false).same_row(),
        DialogItem::check("Include &remote directory", false),
        DialogItem::check("Create &session", false)
            .same_row()
            .disabled(true),
        DialogItem::check("Include &raw settings", false).disabled(true),
        DialogItem::check("Save &extension", false).same_row(),
        DialogItem::text("&Result:"),
        DialogItem::edit(&url(parts, include)).read_only().width(60),
        DialogItem::button("Copy to &clipboard").no_close(),
        DialogItem::separator(),
        DialogItem::button("OK").default_button(),
    ]
}

fn refreshed(controls: &mut Controls, parts: &Parts) {
    let values: Vec<Value> = (0..=SAVE_EXTENSION)
        .map(|item| controls.value(item))
        .collect();
    controls.set_text(RESULT, &url(parts, include_of(&values)));
}

/// The dialog for the session's `parts`.
pub fn show(host: &Host, parts: &Parts) {
    let dialog = Dialog::new("Generate URL", items(parts));
    host.dialog_with(&dialog, |event, controls| {
        match event {
            DialogEvent::Changed(item) if item <= SAVE_EXTENSION => refreshed(controls, parts),
            DialogEvent::Button(CLIPBOARD) => {
                host.set_clipboard(&controls.value(RESULT).text);
            }
            _ => {}
        }
        true
    });
}

#[cfg(test)]
#[path = "generate_url_tests.rs"]
mod tests;
