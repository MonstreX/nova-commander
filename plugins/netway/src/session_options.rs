//! The session dialog's other tabs' settings (S6.5g: Environment, Directories, SFTP, SCP/Shell) under
//! WinSCP's names and with WinSCP's defaults (`TSessionData::DefaultSettings`), kept in the session's file
//! with its other keys; only what differs from the default is written (WinSCP's `WRITE_DATA`).

/// WinSCP's `TDSTMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DstMode {
    Win,
    #[default]
    Unix,
    Keep,
}

impl DstMode {
    pub const ALL: [Self; 3] = [Self::Unix, Self::Win, Self::Keep];

    /// WinSCP's `ConsiderDST` value.
    fn value(self) -> u8 {
        match self {
            Self::Win => 0,
            Self::Unix => 1,
            Self::Keep => 2,
        }
    }

    fn of(value: &str) -> Option<Self> {
        match value.trim() {
            "0" => Some(Self::Win),
            "1" => Some(Self::Unix),
            "2" => Some(Self::Keep),
            _ => None,
        }
    }
}

/// The settings, as WinSCP's session keeps them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `EOLType`: the server's ends of lines are CR/LF (else LF).
    pub eol_crlf: bool,
    /// `TimeDifference` in minutes (WinSCP keeps days): added to the server's times read without a zone.
    pub time_difference: i32,
    /// `ConsiderDST`.
    pub dst_mode: DstMode,
    pub delete_to_recycle_bin: bool,
    pub overwritten_to_recycle_bin: bool,
    pub recycle_bin_path: String,
    /// "Remember last used directory".
    pub update_directories: bool,
    pub remote_directory: String,
    pub resolve_symlinks: bool,
    /// The SFTP server's command, empty: the `sftp` subsystem.
    pub sftp_server: String,
    /// The shell the commands run in, empty: the user's.
    pub shell: String,
    pub listing_command: String,
    pub ignore_ls_warnings: bool,
    /// `SCPLsFullTime`: "Try to get full timestamp" (auto, else off).
    pub ls_full_time: bool,
    pub unset_national_vars: bool,
    /// The FTP(S) tab's (S6.6e).
    pub ftp: crate::protocol_options::Ftp,
    /// The TLS/SSL tab's and the client certificate (S6.6e).
    pub tls: crate::protocol_options::Tls,
    /// The S3 and WebDAV tabs' (S6.6f).
    pub s3: crate::protocol_options::S3,
    pub webdav: crate::protocol_options::WebDav,
    /// The Connection, Proxy and Tunnel tabs' (S6.5h).
    pub link: crate::link_options::Link,
    /// The SSH, KEX and Authentication tabs' (S6.5i).
    pub ssh: crate::ssh_options::Ssh,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            eol_crlf: false,
            time_difference: 0,
            dst_mode: DstMode::Unix,
            delete_to_recycle_bin: false,
            overwritten_to_recycle_bin: false,
            recycle_bin_path: String::new(),
            update_directories: true,
            remote_directory: String::new(),
            resolve_symlinks: true,
            sftp_server: String::new(),
            shell: String::new(),
            listing_command: LISTING.to_string(),
            ignore_ls_warnings: true,
            ls_full_time: true,
            unset_national_vars: true,
            ftp: crate::protocol_options::Ftp::default(),
            tls: crate::protocol_options::Tls::default(),
            s3: crate::protocol_options::S3::default(),
            webdav: crate::protocol_options::WebDav::default(),
            link: crate::link_options::Link::default(),
            ssh: crate::ssh_options::Ssh::default(),
        }
    }
}

/// WinSCP's default listing command.
pub const LISTING: &str = "ls -la";

/// The keys under WinSCP's names.
pub mod keys {
    pub const EOL_TYPE: &str = "EOLType";
    pub const TIME_DIFFERENCE: &str = "TimeDifference";
    pub const CONSIDER_DST: &str = "ConsiderDST";
    pub const DELETE_TO_RECYCLE_BIN: &str = "DeleteToRecycleBin";
    pub const OVERWRITTEN_TO_RECYCLE_BIN: &str = "OverwrittenToRecycleBin";
    pub const RECYCLE_BIN_PATH: &str = "RecycleBinPath";
    pub const UPDATE_DIRECTORIES: &str = "UpdateDirectories";
    pub const REMOTE_DIRECTORY: &str = "RemoteDirectory";
    pub const RESOLVE_SYMLINKS: &str = "ResolveSymlinks";
    pub const SFTP_SERVER: &str = "SftpServer";
    pub const SHELL: &str = "Shell";
    pub const LISTING_COMMAND: &str = "ListingCommand";
    pub const IGNORE_LS_WARNINGS: &str = "IgnoreLsWarnings";
    pub const SCP_LS_FULL_TIME: &str = "SCPLsFullTime";
    pub const UNSET_NATIONAL_VARS: &str = "UnsetNationalVars";

    pub const ALL: [&str; 15] = [
        EOL_TYPE,
        TIME_DIFFERENCE,
        CONSIDER_DST,
        DELETE_TO_RECYCLE_BIN,
        OVERWRITTEN_TO_RECYCLE_BIN,
        RECYCLE_BIN_PATH,
        UPDATE_DIRECTORIES,
        REMOTE_DIRECTORY,
        RESOLVE_SYMLINKS,
        SFTP_SERVER,
        SHELL,
        LISTING_COMMAND,
        IGNORE_LS_WARNINGS,
        SCP_LS_FULL_TIME,
        UNSET_NATIONAL_VARS,
    ];
}

/// WinSCP's `1` / `0` (also `true` / `false`, `on` / `off`).
fn flag_of(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

fn flag(value: bool) -> String {
    u8::from(value).to_string()
}

/// WinSCP's `TimeDifference` (days, a float) as minutes.
fn minutes_of(days: &str) -> Option<i32> {
    let days: f64 = days.trim().parse().ok()?;
    let minutes = (days * 24.0 * 60.0).round();
    (minutes.abs() < f64::from(i32::MAX)).then_some(minutes as i32)
}

/// Minutes as WinSCP's days.
fn days_of(minutes: i32) -> String {
    let days = f64::from(minutes) / (24.0 * 60.0);
    format!("{days}")
}

/// WinSCP's `SCPLsFullTime`: `asOn` 0, `asOff` 1, `asAuto` 2.
fn full_time_of(value: &str) -> Option<bool> {
    match value.trim() {
        "0" | "2" => Some(true),
        "1" => Some(false),
        _ => None,
    }
}

impl Options {
    /// The settings in `lines` (the session's other keys); a value not understood keeps the default.
    pub fn read(lines: &[(String, String)]) -> Self {
        let mut options = Self::default();
        for (key, value) in lines {
            options.take(key, value);
        }
        options.link = crate::link_options::Link::read(lines);
        options.ssh = crate::ssh_options::Ssh::read(lines);
        options.ftp = crate::protocol_options::Ftp::read(lines);
        options.tls = crate::protocol_options::Tls::read(lines);
        options.s3 = crate::protocol_options::S3::read(lines);
        options.webdav = crate::protocol_options::WebDav::read(lines);
        options
    }

    fn take(&mut self, key: &str, value: &str) {
        let text = || value.to_string();
        match key {
            keys::EOL_TYPE => self.eol_crlf = value.trim() == "1",
            keys::TIME_DIFFERENCE => set(&mut self.time_difference, minutes_of(value)),
            keys::CONSIDER_DST => set(&mut self.dst_mode, DstMode::of(value)),
            keys::DELETE_TO_RECYCLE_BIN => set(&mut self.delete_to_recycle_bin, flag_of(value)),
            keys::OVERWRITTEN_TO_RECYCLE_BIN => {
                set(&mut self.overwritten_to_recycle_bin, flag_of(value));
            }
            keys::RECYCLE_BIN_PATH => self.recycle_bin_path = text(),
            keys::UPDATE_DIRECTORIES => set(&mut self.update_directories, flag_of(value)),
            keys::REMOTE_DIRECTORY => self.remote_directory = text(),
            keys::RESOLVE_SYMLINKS => set(&mut self.resolve_symlinks, flag_of(value)),
            keys::SFTP_SERVER => self.sftp_server = text(),
            keys::SHELL => self.shell = text(),
            keys::LISTING_COMMAND => self.listing_command = text(),
            keys::IGNORE_LS_WARNINGS => set(&mut self.ignore_ls_warnings, flag_of(value)),
            keys::SCP_LS_FULL_TIME => set(&mut self.ls_full_time, full_time_of(value)),
            keys::UNSET_NATIONAL_VARS => set(&mut self.unset_national_vars, flag_of(value)),
            _ => {}
        }
    }

    /// The lines of what differs from the default, under WinSCP's names.
    pub fn lines(&self) -> Vec<(String, String)> {
        let base = Self::default();
        let mut lines = Vec::new();
        let mut put = |differs: bool, key: &str, value: String| {
            if differs {
                lines.push((key.to_string(), value));
            }
        };
        put(
            self.eol_crlf != base.eol_crlf,
            keys::EOL_TYPE,
            flag(self.eol_crlf),
        );
        put(
            self.time_difference != 0,
            keys::TIME_DIFFERENCE,
            days_of(self.time_difference),
        );
        put(
            self.dst_mode != base.dst_mode,
            keys::CONSIDER_DST,
            self.dst_mode.value().to_string(),
        );
        self.recycle_lines(&mut put);
        put(
            self.update_directories != base.update_directories,
            keys::UPDATE_DIRECTORIES,
            flag(self.update_directories),
        );
        put(
            !self.remote_directory.is_empty(),
            keys::REMOTE_DIRECTORY,
            self.remote_directory.clone(),
        );
        put(
            self.resolve_symlinks != base.resolve_symlinks,
            keys::RESOLVE_SYMLINKS,
            flag(self.resolve_symlinks),
        );
        self.shell_lines(&base, &mut put);
        lines.extend(self.ftp.lines());
        lines.extend(self.tls.lines());
        lines.extend(self.s3.lines());
        lines.extend(self.webdav.lines());
        lines.extend(self.link.lines());
        lines.extend(self.ssh.lines());
        lines
    }

    fn recycle_lines(&self, put: &mut impl FnMut(bool, &str, String)) {
        put(
            self.delete_to_recycle_bin,
            keys::DELETE_TO_RECYCLE_BIN,
            flag(true),
        );
        put(
            self.overwritten_to_recycle_bin,
            keys::OVERWRITTEN_TO_RECYCLE_BIN,
            flag(true),
        );
        put(
            !self.recycle_bin_path.is_empty(),
            keys::RECYCLE_BIN_PATH,
            self.recycle_bin_path.clone(),
        );
    }

    fn shell_lines(&self, base: &Self, put: &mut impl FnMut(bool, &str, String)) {
        put(
            !self.sftp_server.is_empty(),
            keys::SFTP_SERVER,
            self.sftp_server.clone(),
        );
        put(!self.shell.is_empty(), keys::SHELL, self.shell.clone());
        put(
            self.listing_command != base.listing_command,
            keys::LISTING_COMMAND,
            self.listing_command.clone(),
        );
        put(
            self.ignore_ls_warnings != base.ignore_ls_warnings,
            keys::IGNORE_LS_WARNINGS,
            flag(self.ignore_ls_warnings),
        );
        put(
            self.ls_full_time != base.ls_full_time,
            keys::SCP_LS_FULL_TIME,
            if self.ls_full_time { "2" } else { "1" }.to_string(),
        );
        put(
            self.unset_national_vars != base.unset_national_vars,
            keys::UNSET_NATIONAL_VARS,
            flag(self.unset_national_vars),
        );
    }

    /// `other` with these settings in place of the ones it had (its other keys kept, in their order).
    pub fn merged(&self, other: &[(String, String)]) -> Vec<(String, String)> {
        let mut merged: Vec<(String, String)> = (other.iter())
            .filter(|(key, _)| !keys::ALL.contains(&key.as_str()))
            .filter(|(key, _)| !crate::link_options::KEYS.contains(&key.as_str()))
            .filter(|(key, _)| !crate::ssh_options::KEYS.contains(&key.as_str()))
            .filter(|(key, _)| !crate::protocol_options::KEYS.contains(&key.as_str()))
            .cloned()
            .collect();
        merged.extend(self.lines());
        merged
    }

    /// WinSCP's "Unix" preset of the environment: LF, Unix's DST.
    pub fn unix(&mut self) {
        self.eol_crlf = false;
        self.dst_mode = DstMode::Unix;
    }

    /// WinSCP's "Windows" preset: CR/LF, Windows's DST.
    pub fn windows(&mut self) {
        self.eol_crlf = true;
        self.dst_mode = DstMode::Win;
    }
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

#[cfg(test)]
#[path = "session_options_tests.rs"]
mod tests;
