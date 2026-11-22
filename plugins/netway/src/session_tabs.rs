//! The session dialog's tabs after Session (NetBox's `TSessionDialog`, S6.5g): Environment (the server's
//! ends of lines, its time zone offset, daylight saving time with the Unix / Windows presets, the recycle
//! bin), Directories ("Remember last used directory", the remote directory, resolving links), SFTP (the
//! SFTP server's command), SCP/Shell (the shell, the listing command, "Ignore LS warnings", "Try to get
//! full timestamp", "Clear national variables"); what the protocol has no use for disabled
//! (`UpdateControls`).

use nc_plugin_sdk::{Controls, DialogItem, Value};

use crate::placed::{at, heading, line};
use crate::session::Protocol;
use crate::session_options::{DstMode, Options};

/// The tabs' names (NetBox's `NB_LOGIN_TAB_*`), in their items' order (the bar's order is `there`'s).
pub const NAMES: [&str; 15] = [
    "Session",
    "Environment",
    "Directories",
    "SFTP",
    "SCP/Shell",
    "Connection",
    "Proxy",
    "Tunnel",
    "SSH",
    "KEX",
    "Authentication",
    "FTP(S)",
    "TLS/SSL",
    "WebDAV",
    "S3",
];
pub const ENVIRONMENT: usize = 1;
pub const DIRECTORIES: usize = 2;
pub const SFTP: usize = 3;
pub const SCP: usize = 4;
pub const CONNECTION: usize = 5;
pub const PROXY: usize = 6;
pub const TUNNEL: usize = 7;
pub const SSH: usize = 8;
pub const KEX: usize = 9;
pub const AUTHENTICATION: usize = 10;
pub const FTP: usize = 11;
pub const TLS: usize = 12;
pub const WEBDAV: usize = 13;
pub const S3: usize = 14;

/// Each tab's first item; the items after the last tab's are the dialog's buttons.
pub const FIRST: [usize; 16] = [
    1, 22, 41, 47, 50, 60, 74, 95, 108, 115, 125, 130, 153, 155, 160, 166,
];

/// The dialog's buttons' line, the first item after the tabs.
pub const BUTTONS: usize = FIRST[15];

// The items, in their order.
const EOL: usize = 24;
const ZONE_LABEL: usize = 25;
const HOURS: usize = 26;
const HOURS_LABEL: usize = 27;
const MINUTES: usize = 28;
const MINUTES_LABEL: usize = 29;
const DST_UNIX: usize = 31;
const DST_WIN: usize = 32;
const DST_KEEP: usize = 33;
pub const UNIX: usize = 34;
pub const WINDOWS: usize = 35;
pub const DELETE_TO_BIN: usize = 37;
pub const OVERWRITTEN_TO_BIN: usize = 38;
const BIN_LABEL: usize = 39;
const BIN_PATH: usize = 40;
const UPDATE_DIRECTORIES: usize = 42;
const REMOTE_DIRECTORY: usize = 44;
const RESOLVE_SYMLINKS: usize = 46;
const SFTP_SERVER: usize = 49;
const SHELL: usize = 52;
const LISTING_LABEL: usize = 54;
const LISTING: usize = 55;
const IGNORE_WARNINGS: usize = 56;
const FULL_TIME: usize = 57;
const NATIONAL: usize = 59;

/// NetBox's "Default" of the SFTP server's and the shell's lines.
const DEFAULT: &str = "Default";

const LEFT: i32 = 5;
const WIDTH: i32 = 59;
const VALUES: i32 = 28;

/// The tab item `item` is on (none: the tab bar and the buttons).
pub fn tab_of(item: usize) -> Option<usize> {
    (FIRST.windows(2)).position(|range| (range[0]..range[1]).contains(&item))
}

/// The tabs a protocol has, `encrypted` or not, in NetBox's order (its tabs enabled by `UpdateControls`).
pub fn there(protocol: Protocol, encrypted: bool) -> Vec<usize> {
    let mut tabs = vec![0, ENVIRONMENT, DIRECTORIES];
    if protocol == Protocol::Sftp {
        tabs.push(SFTP);
    }
    if protocol.is_ssh() {
        tabs.push(SCP);
    }
    if protocol == Protocol::Ftp {
        tabs.push(FTP);
    }
    if protocol == Protocol::S3 {
        tabs.push(S3);
    }
    tabs.extend([CONNECTION, PROXY]);
    let tls = match protocol {
        Protocol::Ftp | Protocol::WebDav => encrypted,
        Protocol::S3 => true,
        Protocol::Sftp | Protocol::Scp => false,
    };
    if tls {
        tabs.push(TLS);
    }
    if protocol.is_ssh() {
        tabs.extend([TUNNEL, SSH, KEX, AUTHENTICATION]);
    }
    if protocol == Protocol::WebDav {
        tabs.push(WEBDAV);
    }
    tabs
}

fn shown_default(text: &str) -> &str {
    if text.is_empty() { DEFAULT } else { text }
}

fn typed_default(text: &str) -> String {
    match text.trim() {
        DEFAULT => String::new(),
        text => text.to_string(),
    }
}

/// The Environment tab's items.
fn environment(options: &Options) -> Vec<DialogItem> {
    let dst = |mode: DstMode| options.dst_mode == mode;
    let (hours, minutes) = (options.time_difference / 60, options.time_difference % 60);
    vec![
        heading("Server environment", LEFT - 1, 3, WIDTH + 2),
        at(
            DialogItem::text("&End-of-line characters (if not known):"),
            LEFT,
            4,
        ),
        line(
            DialogItem::list(&["LF", "CR/LF"], usize::from(options.eol_crlf)),
            56,
            4,
            7,
        ),
        at(DialogItem::text("Server time&zone offset:"), LEFT, 5),
        line(DialogItem::fixed("###", &hours.to_string()), 31, 5, 3),
        at(DialogItem::text("hours"), 35, 5),
        line(DialogItem::fixed("###", &minutes.to_string()), 41, 5, 3),
        at(DialogItem::text("minutes"), 45, 5),
        heading("Daylight saving time", LEFT - 1, 6, WIDTH + 2),
        at(
            DialogItem::radio(
                "Adjust remote timestamp to local co&nventions (Unix)",
                dst(DstMode::Unix),
            )
            .group(),
            LEFT,
            7,
        ),
        at(
            DialogItem::radio(
                "Adjust remote timestamp with &DST (Windows)",
                dst(DstMode::Win),
            ),
            LEFT,
            8,
        ),
        at(
            DialogItem::radio("Preser&ve remote timestamp (Unix)", dst(DstMode::Keep)),
            LEFT,
            9,
        ),
        at(DialogItem::button("Uni&x").no_close(), LEFT, 10),
        at(DialogItem::button("&Windows").no_close(), LEFT + 9, 10),
        heading("Recycle bin", LEFT - 1, 11, WIDTH + 2),
        at(
            DialogItem::check(
                "&Preserve deleted remote files to recycle bin",
                options.delete_to_recycle_bin,
            ),
            LEFT,
            12,
        ),
        at(
            DialogItem::check(
                "Preserve &overwritten remote files to bin (SFTP only)",
                options.overwritten_to_recycle_bin,
            ),
            LEFT,
            13,
        ),
        at(DialogItem::text("&Remote recycle bin:"), LEFT, 14),
        line(DialogItem::edit(&options.recycle_bin_path), LEFT, 15, WIDTH),
    ]
}

/// The Directories, SFTP and SCP/Shell tabs' items.
fn others(options: &Options) -> Vec<DialogItem> {
    vec![
        heading("Directories", LEFT - 1, 3, WIDTH + 2),
        at(
            DialogItem::check("Re&member last used directory", options.update_directories),
            LEFT,
            4,
        ),
        at(DialogItem::text("&Remote directory:"), LEFT, 5),
        line(
            DialogItem::edit(&options.remote_directory).history("WinscpRemoteDirectory", false),
            LEFT,
            6,
            WIDTH,
        ),
        heading("Directory reading options", LEFT - 1, 7, WIDTH + 2),
        at(
            DialogItem::check("Resolve &symbolic links", options.resolve_symlinks),
            LEFT,
            8,
        ),
        heading("Protocol options", LEFT - 1, 3, WIDTH + 2),
        at(DialogItem::text("SFTP ser&ver:"), LEFT, 4),
        line(
            DialogItem::edit(shown_default(&options.sftp_server)),
            VALUES,
            4,
            36,
        ),
        heading("Shell", LEFT - 1, 3, WIDTH + 2),
        at(DialogItem::text("S&hell:"), LEFT, 4),
        line(
            DialogItem::edit(shown_default(&options.shell)),
            VALUES,
            4,
            36,
        ),
        heading("Directory listing", LEFT - 1, 5, WIDTH + 2),
        at(DialogItem::text("Listing &command:"), LEFT, 6),
        line(DialogItem::edit(&options.listing_command), VALUES, 6, 36),
        at(
            DialogItem::check("Ignore LS &warnings", options.ignore_ls_warnings),
            LEFT,
            7,
        ),
        at(
            DialogItem::check("Try to get &full timestamp", options.ls_full_time),
            LEFT,
            8,
        ),
        heading("Other options", LEFT - 1, 9, WIDTH + 2),
        at(
            DialogItem::check("Clear &national variables", options.unset_national_vars),
            LEFT,
            10,
        ),
    ]
}

/// The tabs' items, hidden (the dialog opens on Session).
pub fn items(options: &Options) -> Vec<DialogItem> {
    let mut items = environment(options);
    items.extend(others(options));
    let items: Vec<DialogItem> = items.into_iter().map(|item| item.hidden(true)).collect();
    let mut items = items;
    items.extend(crate::session_link_tabs::items(&options.link));
    items.extend(crate::session_ssh_tabs::items(&options.ssh));
    items.extend(crate::session_protocol_tabs::items(options));
    items
}

/// The options the dialog's `values` give, over `base`.
pub fn options_of(values: &[Value], base: &Options) -> Options {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let number = |item: usize| value(item).text.trim().parse::<i32>().unwrap_or(0);
    let dst_mode = match () {
        () if value(DST_WIN).checked() => DstMode::Win,
        () if value(DST_KEEP).checked() => DstMode::Keep,
        () if value(DST_UNIX).checked() => DstMode::Unix,
        () => base.dst_mode,
    };
    let order = crate::session_ssh_tabs::Order::of(&base.ssh);
    let mut ssh = crate::session_ssh_tabs::ssh_of(values, &base.ssh, &order);
    let compression = (base.ssh.compression, ssh.compression);
    ssh.compression = crate::session_protocol_tabs::compression_of(values, compression);
    Options {
        eol_crlf: value(EOL).state == 1,
        time_difference: number(HOURS) * 60 + number(MINUTES),
        dst_mode,
        delete_to_recycle_bin: value(DELETE_TO_BIN).checked(),
        overwritten_to_recycle_bin: value(OVERWRITTEN_TO_BIN).checked(),
        recycle_bin_path: value(BIN_PATH).text.trim().to_string(),
        update_directories: value(UPDATE_DIRECTORIES).checked(),
        remote_directory: value(REMOTE_DIRECTORY).text.trim().to_string(),
        resolve_symlinks: value(RESOLVE_SYMLINKS).checked(),
        sftp_server: typed_default(&value(SFTP_SERVER).text),
        shell: typed_default(&value(SHELL).text),
        listing_command: match value(LISTING).text.trim() {
            "" => base.listing_command.clone(),
            typed => typed.to_string(),
        },
        ignore_ls_warnings: value(IGNORE_WARNINGS).checked(),
        ls_full_time: value(FULL_TIME).checked(),
        unset_national_vars: value(NATIONAL).checked(),
        ftp: crate::session_protocol_tabs::ftp_of(values, &base.ftp),
        tls: crate::session_protocol_tabs::tls_of(values, &base.tls),
        s3: crate::session_protocol_tabs::s3_of(values),
        webdav: crate::session_protocol_tabs::webdav_of(values),
        link: crate::session_link_tabs::link_of(values, &base.link),
        ssh,
    }
}

/// What the protocol has no use for disabled (NetBox's `UpdateControls`): the time zone offset but for
/// SCP (and FTP), the overwritten files' bin but for SFTP, the bin's folder while no bin is used, the
/// listing's and the shell's options but for SCP; then the Connection, Proxy and Tunnel tabs' (`tab` shown),
/// the SSH tabs' and the FTP(S) and TLS/SSL tabs'.
pub fn update(controls: &mut Controls, (protocol, encrypted): (Protocol, bool), tab: usize) {
    crate::session_link_tabs::update(controls, protocol, tab == PROXY);
    crate::session_ssh_tabs::update(controls);
    crate::session_protocol_tabs::update(controls, protocol, encrypted);
    let zone = matches!(protocol, Protocol::Scp | Protocol::Ftp);
    for item in [ZONE_LABEL, HOURS, HOURS_LABEL, MINUTES, MINUTES_LABEL] {
        controls.enable(item, zone);
    }
    controls.enable(OVERWRITTEN_TO_BIN, protocol == Protocol::Sftp);
    let bin = controls.value(DELETE_TO_BIN).checked()
        || (protocol == Protocol::Sftp && controls.value(OVERWRITTEN_TO_BIN).checked());
    controls.enable(BIN_LABEL, bin);
    controls.enable(BIN_PATH, bin);
    let scp = protocol == Protocol::Scp;
    for item in [LISTING_LABEL, LISTING, IGNORE_WARNINGS, FULL_TIME, NATIONAL] {
        controls.enable(item, scp);
    }
}

/// NetBox's Unix / Windows presets of the environment.
pub fn preset(controls: &mut Controls, windows: bool) {
    let mut options = Options::default();
    if windows {
        options.windows();
    } else {
        options.unix();
    }
    controls.set_state(EOL, i32::from(options.eol_crlf));
    controls.set_checked(DST_UNIX, options.dst_mode == DstMode::Unix);
    controls.set_checked(DST_WIN, options.dst_mode == DstMode::Win);
    controls.set_checked(DST_KEEP, false);
}

#[cfg(test)]
#[path = "session_tabs_tests.rs"]
mod tests;
