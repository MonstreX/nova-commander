//! The session dialog's FTP(S) and TLS/SSL tabs' settings (S6.6e) under WinSCP's names and with WinSCP's
//! defaults (`TSessionData::DefaultSettings`): FTP's passive mode, MLSD, `LIST -a`, the passive address
//! forced, the HOST command, the account, VMS's revisions, an empty password, FF doubled and undoubled,
//! the commands sent after the login; TLS's client certificate and session reuse. Only what differs from
//! the default is written.

/// WinSCP's `TAutoSwitch` (its values: `asOn` 0, `asOff` 1, `asAuto` 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Switch {
    On,
    Off,
    #[default]
    Auto,
}

impl Switch {
    /// NetBox's lists' order (`NB_LOGIN_BUGS_AUTO`, `_OFF`, `_ON`): its index is `2 -` the value.
    pub const LABELS: [&'static str; 3] = ["Auto", "Off", "On"];
    pub const ALL_VALUES: [Self; 3] = [Self::On, Self::Off, Self::Auto];

    pub fn value(self) -> u8 {
        match self {
            Self::On => 0,
            Self::Off => 1,
            Self::Auto => 2,
        }
    }

    pub fn of(value: &str) -> Option<Self> {
        match value.trim() {
            "0" => Some(Self::On),
            "1" => Some(Self::Off),
            "2" => Some(Self::Auto),
            _ => None,
        }
    }

    /// The list's line.
    pub fn index(self) -> usize {
        usize::from(2 - self.value())
    }

    pub fn at(index: usize) -> Self {
        match index {
            1 => Self::Off,
            2 => Self::On,
            _ => Self::Auto,
        }
    }
}

/// The FTP(S) tab's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ftp {
    /// `FtpPasvMode`.
    pub passive: bool,
    /// `FtpUseMlsd`: MLSD when the server has it, never, always.
    pub use_mlsd: Switch,
    /// `FtpListAll`: `LIST -a`.
    pub list_all: Switch,
    /// `FtpForcePasvIp2`: the server's address in place of the passive reply's.
    pub force_pasv_ip: Switch,
    /// `FtpHost`: `HOST` before the login.
    pub host_command: Switch,
    /// `FtpAccount`: sent by `ACCT` when the server asks.
    pub account: String,
    /// `VMSAllRevisions`.
    pub vms_all_revisions: bool,
    /// `FtpAllowEmptyPassword`: a user without a password is not asked one.
    pub allow_empty_password: bool,
    /// `FtpDupFF`, `FtpUndupFF`.
    pub dup_ff: bool,
    pub undup_ff: bool,
    /// `PostLoginCommands`: sent after the login, each line.
    pub post_login_commands: Vec<String>,
}

impl Default for Ftp {
    fn default() -> Self {
        Self {
            passive: true,
            use_mlsd: Switch::Auto,
            list_all: Switch::Auto,
            force_pasv_ip: Switch::Auto,
            host_command: Switch::Auto,
            account: String::new(),
            vms_all_revisions: false,
            allow_empty_password: false,
            dup_ff: false,
            undup_ff: false,
            post_login_commands: Vec::new(),
        }
    }
}

/// The TLS/SSL tab's (and the client certificate the FTP(S) tab names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tls {
    /// `TlsCertificateFile`: the client certificate and its key, PEM.
    pub certificate_file: String,
    /// `SslSessionReuse`.
    pub session_reuse: bool,
}

impl Default for Tls {
    fn default() -> Self {
        Self {
            certificate_file: String::new(),
            session_reuse: true,
        }
    }
}

/// The S3 tab's (S6.6f).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct S3 {
    /// `S3DefaultRegion`: empty, libs3's `us-east-1`.
    pub default_region: String,
    /// `S3UrlStyle` 1: path-style addresses (else virtual-host ones, 0).
    pub path_style: bool,
    /// `S3RequesterPays`: `x-amz-request-payer: requester`.
    pub requester_pays: bool,
}

/// The WebDAV tab's (S6.6f; its compression is the session's `Compression`, the SSH tab's too).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebDav {
    /// `WebDavLiberalEscaping`.
    pub liberal_escaping: bool,
}

impl S3 {
    /// The S3 settings in the session's other keys.
    pub fn read(lines: &[(String, String)]) -> Self {
        let mut s3 = Self::default();
        for (key, value) in lines {
            match key.as_str() {
                "S3DefaultRegion" => s3.default_region = value.trim().to_string(),
                "S3UrlStyle" => set(&mut s3.path_style, flag_of(value)),
                "S3RequesterPays" => set(&mut s3.requester_pays, flag_of(value)),
                _ => {}
            }
        }
        s3
    }

    /// The lines of what differs from the default.
    pub fn lines(&self) -> Vec<(String, String)> {
        let mut lines = Vec::new();
        if !self.default_region.is_empty() {
            lines.push(("S3DefaultRegion".to_string(), self.default_region.clone()));
        }
        for (on, key) in [
            (self.path_style, "S3UrlStyle"),
            (self.requester_pays, "S3RequesterPays"),
        ] {
            if on {
                lines.push((key.to_string(), flag(true)));
            }
        }
        lines
    }
}

impl WebDav {
    /// The WebDAV settings in the session's other keys.
    pub fn read(lines: &[(String, String)]) -> Self {
        let mut webdav = Self::default();
        for (key, value) in lines {
            if key == "WebDavLiberalEscaping" {
                set(&mut webdav.liberal_escaping, flag_of(value));
            }
        }
        webdav
    }

    /// The lines of what differs from the default.
    pub fn lines(&self) -> Vec<(String, String)> {
        match self.liberal_escaping {
            true => vec![("WebDavLiberalEscaping".to_string(), flag(true))],
            false => Vec::new(),
        }
    }
}

pub const KEYS: [&str; 17] = [
    "S3DefaultRegion",
    "S3UrlStyle",
    "S3RequesterPays",
    "WebDavLiberalEscaping",
    "FtpPasvMode",
    "FtpUseMlsd",
    "FtpListAll",
    "FtpForcePasvIp2",
    "FtpHost",
    "FtpAccount",
    "VMSAllRevisions",
    "FtpAllowEmptyPassword",
    "FtpDupFF",
    "FtpUndupFF",
    "PostLoginCommands",
    "TlsCertificateFile",
    "SslSessionReuse",
];

fn flag_of(value: &str) -> Option<bool> {
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn flag(value: bool) -> String {
    u8::from(value).to_string()
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

/// The commands' lines as one value: `%` and the line breaks escaped (`%25`, `%0A`).
pub fn joined(commands: &[String]) -> String {
    let escaped: Vec<String> = (commands.iter())
        .map(|command| command.replace('%', "%25").replace('\n', "%0A"))
        .collect();
    escaped.join("%0A")
}

/// The commands' lines of a value (empty lines left out).
pub fn split(value: &str) -> Vec<String> {
    (value.split("%0A"))
        .map(|line| line.replace("%25", "%"))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

impl Ftp {
    /// The FTP settings in the session's other keys.
    pub fn read(lines: &[(String, String)]) -> Self {
        let mut ftp = Self::default();
        for (key, value) in lines {
            ftp.take(key, value);
        }
        ftp
    }

    fn take(&mut self, key: &str, value: &str) {
        match key {
            "FtpPasvMode" => set(&mut self.passive, flag_of(value)),
            "FtpUseMlsd" => set(&mut self.use_mlsd, Switch::of(value)),
            "FtpListAll" => set(&mut self.list_all, Switch::of(value)),
            "FtpForcePasvIp2" => set(&mut self.force_pasv_ip, Switch::of(value)),
            "FtpHost" => set(&mut self.host_command, Switch::of(value)),
            "FtpAccount" => self.account = value.to_string(),
            "VMSAllRevisions" => set(&mut self.vms_all_revisions, flag_of(value)),
            "FtpAllowEmptyPassword" => set(&mut self.allow_empty_password, flag_of(value)),
            "FtpDupFF" => set(&mut self.dup_ff, flag_of(value)),
            "FtpUndupFF" => set(&mut self.undup_ff, flag_of(value)),
            "PostLoginCommands" => self.post_login_commands = split(value),
            _ => {}
        }
    }

    /// The lines of what differs from the default.
    pub fn lines(&self) -> Vec<(String, String)> {
        let base = Self::default();
        let mut lines = Vec::new();
        let mut put = |differs: bool, key: &str, value: String| {
            if differs {
                lines.push((key.to_string(), value));
            }
        };
        put(
            self.passive != base.passive,
            "FtpPasvMode",
            flag(self.passive),
        );
        for (switch, key) in [
            (self.use_mlsd, "FtpUseMlsd"),
            (self.list_all, "FtpListAll"),
            (self.force_pasv_ip, "FtpForcePasvIp2"),
            (self.host_command, "FtpHost"),
        ] {
            put(switch != Switch::Auto, key, switch.value().to_string());
        }
        put(!self.account.is_empty(), "FtpAccount", self.account.clone());
        for (on, key) in [
            (self.vms_all_revisions, "VMSAllRevisions"),
            (self.allow_empty_password, "FtpAllowEmptyPassword"),
            (self.dup_ff, "FtpDupFF"),
            (self.undup_ff, "FtpUndupFF"),
        ] {
            put(on, key, flag(true));
        }
        put(
            !self.post_login_commands.is_empty(),
            "PostLoginCommands",
            joined(&self.post_login_commands),
        );
        lines
    }
}

impl Tls {
    /// The TLS settings in the session's other keys.
    pub fn read(lines: &[(String, String)]) -> Self {
        let mut tls = Self::default();
        for (key, value) in lines {
            match key.as_str() {
                "TlsCertificateFile" => tls.certificate_file = value.to_string(),
                "SslSessionReuse" => set(&mut tls.session_reuse, flag_of(value)),
                _ => {}
            }
        }
        tls
    }

    /// The lines of what differs from the default.
    pub fn lines(&self) -> Vec<(String, String)> {
        let mut lines = Vec::new();
        if !self.certificate_file.is_empty() {
            lines.push((
                "TlsCertificateFile".to_string(),
                self.certificate_file.clone(),
            ));
        }
        if !self.session_reuse {
            lines.push(("SslSessionReuse".to_string(), flag(false)));
        }
        lines
    }
}

#[cfg(test)]
#[path = "protocol_options_tests.rs"]
mod tests;
