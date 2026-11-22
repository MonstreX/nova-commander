//! The session dialog's FTP(S) and TLS/SSL tabs (NetBox's `TSessionDialog`, S6.6e): " Protocol options " -
//! "Use MLSD command for directory listing", "Support for listing of hidden files:", "Force IP address for
//! passive mode connections:", "Use HOST command to select host on the server" (Auto, Off, On), "Account:",
//! "Display all file revisions on VMS servers", "Passive mode", "Allow empty password", "Dup FF in
//! commands", "UnDup FF from PWD", "TLS client certificate:", "Post login commands:" in three lines; "TLS
//! options" - "Session ID re-use". The WebDAV and S3 tabs (S6.6f): " WebDAV protocol options " - "Enable
//! compression", "Tolerate non-encoded special characters in filenames", "TLS client certificate:";
//! " Protocol options " - "Default region:", "URL style:" (Virtual Host, Path), "Requester pays". What the
//! protocol has no use for disabled (`UpdateControls`).

use nc_plugin_sdk::{Controls, DialogItem, Value};

use crate::placed::{at, heading, line};
use crate::protocol_options::{Ftp, S3, Switch, Tls, WebDav};
use crate::session::Protocol;
use crate::session_options::Options;

/// The first item of FTP(S), TLS/SSL, WebDAV and S3, and the one after them.
pub const FIRST: [usize; 5] = [130, 153, 155, 160, 166];

// The items, in their order.
const USE_MLSD: usize = 132;
const LIST_ALL: usize = 134;
const FORCE_PASV_IP: usize = 136;
const HOST_COMMAND: usize = 138;
const ACCOUNT: usize = 140;
const VMS: usize = 141;
/// "Passive mode" (NetBox's `FtpPasvModeCheck`, on this tab).
pub const PASSIVE: usize = 142;
const EMPTY_PASSWORD: usize = 143;
const DUP_FF: usize = 144;
const UNDUP_FF: usize = 145;
const CERTIFICATE_LABEL: usize = 146;
const CERTIFICATE: usize = 147;
const COMMANDS: [usize; 3] = [150, 151, 152];
const SESSION_REUSE: usize = 154;
const COMPRESSION: usize = 156;
const LIBERAL_ESCAPING: usize = 157;
const WEBDAV_CERTIFICATE_LABEL: usize = 158;
const WEBDAV_CERTIFICATE: usize = 159;
const REGION: usize = 162;
const URL_STYLE: usize = 164;
const REQUESTER_PAYS: usize = 165;

/// NetBox's `S3URLStyles`.
const URL_STYLES: [&str; 2] = ["Virtual Host", "Path"];

/// Each tab's item the focus goes to.
pub const FOCUS: [usize; 4] = [USE_MLSD, SESSION_REUSE, COMPRESSION, REGION];

/// NetBox's `NB_FTP_PASV_MODE_REQUIRED`.
pub const PASSIVE_REQUIRED: &str =
    "Passive mode must be enabled when FTP connection through proxy is selected.";

const LEFT: i32 = 5;
const WIDTH: i32 = 59;
const SWITCHES: i32 = 53;

fn switch(label: &str, value: Switch, y: i32) -> [DialogItem; 2] {
    [
        at(DialogItem::text(label), LEFT, y),
        line(
            DialogItem::list(&Switch::LABELS, value.index()),
            SWITCHES,
            y,
            11,
        ),
    ]
}

fn ftp_items(ftp: &Ftp, tls: &Tls) -> Vec<DialogItem> {
    let mut items = vec![heading("Protocol options", LEFT - 1, 3, WIDTH + 2)];
    items.extend(switch(
        "Use &MLSD command for directory listing",
        ftp.use_mlsd,
        4,
    ));
    items.extend(switch(
        "&Support for listing of hidden files:",
        ftp.list_all,
        5,
    ));
    items.extend(switch(
        "&Force IP address for passive mode connections:",
        ftp.force_pasv_ip,
        6,
    ));
    items.extend(switch(
        "Use &HOST command to select host on the server",
        ftp.host_command,
        7,
    ));
    items.extend([
        at(DialogItem::text("&Account:"), LEFT, 8),
        line(DialogItem::edit(&ftp.account), 15, 8, 49),
    ]);
    let checks = [
        (
            "Display all file &revisions on VMS servers",
            ftp.vms_all_revisions,
        ),
        ("&Passive mode", ftp.passive),
        ("&Allow empty password", ftp.allow_empty_password),
        ("Du&p FF in commands", ftp.dup_ff),
        ("&UnDup FF from PWD", ftp.undup_ff),
    ];
    for (y, (label, checked)) in (9..).zip(checks) {
        items.push(at(DialogItem::check(label, checked), LEFT, y));
    }
    items.extend([
        at(DialogItem::text("TLS client &certificate:"), LEFT, 14),
        line(DialogItem::edit(&tls.certificate_file), 30, 14, 34),
        heading("", LEFT - 1, 15, WIDTH + 2),
        at(DialogItem::text("Post login &commands:"), LEFT, 16),
    ]);
    for (y, at_line) in (17..).zip(0..COMMANDS.len()) {
        let command = ftp.post_login_commands.get(at_line).cloned();
        items.push(line(
            DialogItem::edit(&command.unwrap_or_default()),
            LEFT,
            y,
            WIDTH,
        ));
    }
    items
}

/// The WebDAV tab's items (its compression the session's, the SSH tab's too).
fn webdav_items(options: &Options) -> [DialogItem; 5] {
    [
        heading("WebDAV protocol options", LEFT - 1, 3, WIDTH + 2),
        at(
            DialogItem::check("Enable &compression", options.ssh.compression),
            LEFT,
            4,
        ),
        at(
            DialogItem::check(
                "&Tolerate non-encoded special characters in filenames",
                options.webdav.liberal_escaping,
            ),
            LEFT,
            5,
        ),
        at(DialogItem::text("TLS client &certificate:"), LEFT, 6),
        line(DialogItem::edit(&options.tls.certificate_file), 30, 6, 34),
    ]
}

/// The S3 tab's items.
fn s3_items(s3: &S3) -> [DialogItem; 6] {
    [
        heading("Protocol options", LEFT - 1, 3, WIDTH + 2),
        at(DialogItem::text("&Default region:"), LEFT, 4),
        line(
            DialogItem::edit(&s3.default_region).history("S3DefaultRegion", false),
            26,
            4,
            20,
        ),
        at(DialogItem::text("&URL style:"), LEFT, 5),
        line(
            DialogItem::list(&URL_STYLES, usize::from(s3.path_style)),
            26,
            5,
            20,
        ),
        at(
            DialogItem::check("Requester &pays", s3.requester_pays),
            LEFT,
            6,
        ),
    ]
}

/// The four tabs' items, hidden.
pub fn items(options: &Options) -> Vec<DialogItem> {
    let mut items = ftp_items(&options.ftp, &options.tls);
    items.extend([
        heading("TLS options", LEFT - 1, 3, WIDTH + 2),
        at(
            DialogItem::check("&Session ID re-use", options.tls.session_reuse),
            LEFT,
            4,
        ),
    ]);
    items.extend(webdav_items(options));
    items.extend(s3_items(&options.s3));
    items.into_iter().map(|item| item.hidden(true)).collect()
}

/// The FTP settings the dialog's `values` give, over `base` (the commands after the third kept).
pub fn ftp_of(values: &[Value], base: &Ftp) -> Ftp {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let checked = |item: usize| value(item).checked();
    let switch = |item: usize| Switch::at(usize::try_from(value(item).state).unwrap_or(0));
    let mut commands: Vec<String> = (COMMANDS.iter())
        .map(|item| value(*item).text.trim().to_string())
        .collect();
    commands.extend(
        base.post_login_commands
            .iter()
            .skip(COMMANDS.len())
            .cloned(),
    );
    commands.retain(|command| !command.is_empty());
    Ftp {
        passive: checked(PASSIVE),
        use_mlsd: switch(USE_MLSD),
        list_all: switch(LIST_ALL),
        force_pasv_ip: switch(FORCE_PASV_IP),
        host_command: switch(HOST_COMMAND),
        account: value(ACCOUNT).text.trim().to_string(),
        vms_all_revisions: checked(VMS),
        allow_empty_password: checked(EMPTY_PASSWORD),
        dup_ff: checked(DUP_FF),
        undup_ff: checked(UNDUP_FF),
        post_login_commands: commands,
    }
}

/// The TLS settings the dialog's `values` give over `base`: the client certificate of the FTP(S) tab or of
/// the WebDAV tab, whichever was changed (both show the one setting).
pub fn tls_of(values: &[Value], base: &Tls) -> Tls {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let typed = |item: usize| value(item).text.trim().to_string();
    let webdav = typed(WEBDAV_CERTIFICATE);
    Tls {
        certificate_file: match webdav != base.certificate_file {
            true => webdav,
            false => typed(CERTIFICATE),
        },
        session_reuse: value(SESSION_REUSE).checked(),
    }
}

/// The session's compression from what it was (`base`) and the SSH tab's (`ssh`): the WebDAV tab's when
/// it was changed, else the SSH tab's (both show the one setting).
pub fn compression_of(values: &[Value], (base, ssh): (bool, bool)) -> bool {
    let webdav = values.get(COMPRESSION).is_some_and(Value::checked);
    if webdav != base { webdav } else { ssh }
}

/// The WebDAV settings the dialog's `values` give.
pub fn webdav_of(values: &[Value]) -> WebDav {
    WebDav {
        liberal_escaping: values.get(LIBERAL_ESCAPING).is_some_and(Value::checked),
    }
}

/// The S3 settings the dialog's `values` give.
pub fn s3_of(values: &[Value]) -> S3 {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    S3 {
        default_region: value(REGION).text.trim().to_string(),
        path_style: value(URL_STYLE).state == 1,
        requester_pays: value(REQUESTER_PAYS).checked(),
    }
}

/// What the protocol has no use for disabled (NetBox's `UpdateControls`): the FTP tab but for FTP, its
/// client certificate and the session's reuse but for FTPS.
pub fn update(controls: &mut Controls, protocol: Protocol, encrypted: bool) {
    let ftp = protocol == Protocol::Ftp;
    for item in FIRST[0]..FIRST[1] {
        controls.enable(item, ftp);
    }
    for item in [CERTIFICATE_LABEL, CERTIFICATE, SESSION_REUSE] {
        controls.enable(item, ftp && encrypted);
    }
    let webdav = protocol == Protocol::WebDav;
    for item in FIRST[2]..FIRST[3] {
        controls.enable(item, webdav);
    }
    for item in [WEBDAV_CERTIFICATE_LABEL, WEBDAV_CERTIFICATE] {
        controls.enable(item, webdav && encrypted);
    }
    for item in FIRST[3]..FIRST[4] {
        controls.enable(item, protocol == Protocol::S3);
    }
    // Kept, not applied: no VMS listing is read, UTF-8 commands and replies have no 0xFF to double, and
    // a server's names are always taken as they come, encoded or not.
    for item in [VMS, DUP_FF, UNDUP_FF, LIBERAL_ESCAPING] {
        controls.enable(item, false);
    }
}

/// FTP through a proxy needs passive mode (NetBox's `UpdateControls`): checked when it is not; whether it
/// was (to tell the user).
pub fn passive_made(controls: &mut Controls, protocol: Protocol, proxied: bool) -> bool {
    let needed = protocol == Protocol::Ftp && proxied && !controls.value(PASSIVE).checked();
    if needed {
        controls.set_checked(PASSIVE, true);
    }
    needed
}

#[cfg(test)]
#[path = "session_protocol_tabs_tests.rs"]
mod tests;
