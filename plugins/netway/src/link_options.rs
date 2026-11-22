//! The session dialog's Connection, Proxy and Tunnel tabs' settings (S6.5h) under WinSCP's names and with
//! WinSCP's defaults (`TSessionData::DefaultSettings`): the server's response timeout, keepalives, the IP
//! version, the proxy and the SSH tunnel. The proxy's and the tunnel's passwords are kept as the session's
//! (protected for the user, ADR-0023 §3); only what differs from the default is written.

use crate::secret;

/// WinSCP's `TPingType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ping {
    #[default]
    Off,
    NullPacket,
    DummyCommand,
}

/// WinSCP's `TAddressFamily`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Family {
    #[default]
    Auto,
    V4,
    V6,
}

/// WinSCP's `TProxyMethod` (`pmCmd` is the local command).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyMethod {
    #[default]
    None,
    Socks4,
    Socks5,
    Http,
    Telnet,
    Local,
}

impl ProxyMethod {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Socks4,
        Self::Socks5,
        Self::Http,
        Self::Telnet,
        Self::Local,
    ];

    /// NetBox's label.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Socks4 => "SOCKS4",
            Self::Socks5 => "SOCKS5",
            Self::Http => "HTTP",
            Self::Telnet => "Telnet",
            Self::Local => "Local",
        }
    }

    /// WinSCP's `ProxyMethodMapping` names.
    fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Socks4 => "socks4",
            Self::Socks5 => "socks5",
            Self::Http => "http",
            Self::Telnet => "telnet",
            Self::Local => "cmd",
        }
    }

    fn of(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        (Self::ALL.iter().copied())
            .find(|method| method.key() == value)
            .or_else(|| {
                let index: usize = value.parse().ok()?;
                Self::ALL.get(index).copied()
            })
    }
}

/// WinSCP's `TAutoSwitch` of "Do DNS name lookup at proxy end".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyDns {
    No,
    #[default]
    Auto,
    Yes,
}

/// The settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// `Timeout` in seconds, 0: none.
    pub timeout: u32,
    pub ping: Ping,
    /// `PingIntervalSecs`.
    pub ping_interval: u32,
    pub family: Family,
    pub proxy: ProxyMethod,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub proxy_user: String,
    pub proxy_password: String,
    pub telnet_command: String,
    pub local_command: String,
    pub proxy_dns: ProxyDns,
    pub proxy_localhost: bool,
    pub tunnel: bool,
    pub tunnel_host: String,
    pub tunnel_port: u16,
    pub tunnel_user: String,
    pub tunnel_password: String,
    pub tunnel_key: String,
}

/// WinSCP's `ProxyTelnetCommand` default (its `\n` written as such).
pub const TELNET_COMMAND: &str = "connect %host %port\\n";

impl Default for Link {
    fn default() -> Self {
        Self {
            timeout: 15,
            ping: Ping::Off,
            ping_interval: 30,
            family: Family::Auto,
            proxy: ProxyMethod::None,
            proxy_host: "proxy".to_string(),
            proxy_port: 80,
            proxy_user: String::new(),
            proxy_password: String::new(),
            telnet_command: TELNET_COMMAND.to_string(),
            local_command: String::new(),
            proxy_dns: ProxyDns::Auto,
            proxy_localhost: false,
            tunnel: false,
            tunnel_host: String::new(),
            tunnel_port: 22,
            tunnel_user: String::new(),
            tunnel_password: String::new(),
            tunnel_key: String::new(),
        }
    }
}

pub const KEYS: [&str; 19] = [
    "Timeout",
    "PingType",
    "PingIntervalSecs",
    "AddressFamily",
    "ProxyMethod",
    "ProxyHost",
    "ProxyPort",
    "ProxyUsername",
    "ProxyPassword",
    "ProxyTelnetCommand",
    "ProxyLocalCommand",
    "ProxyDNS",
    "ProxyLocalhost",
    "Tunnel",
    "TunnelHostName",
    "TunnelPortNumber",
    "TunnelUserName",
    "TunnelPassword",
    "TunnelPublicKeyFile",
];

fn number<T: std::str::FromStr>(value: &str) -> Option<T> {
    value.trim().parse().ok()
}

fn flag_of(value: &str) -> Option<bool> {
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// A kept password read back (none kept, or not readable here: empty).
fn password_of(value: &str) -> String {
    secret::read(value).unwrap_or_default()
}

impl Link {
    /// The settings in the session's other keys.
    pub fn read(lines: &[(String, String)]) -> Self {
        let mut link = Self::default();
        for (key, value) in lines {
            link.take(key, value);
        }
        link
    }

    fn take(&mut self, key: &str, value: &str) {
        let text = || value.to_string();
        match key {
            "Timeout" => set(&mut self.timeout, number(value)),
            "PingType" => set(&mut self.ping, ping_of(value)),
            "PingIntervalSecs" => set(&mut self.ping_interval, number(value)),
            "AddressFamily" => set(&mut self.family, family_of(value)),
            "ProxyMethod" => set(&mut self.proxy, ProxyMethod::of(value)),
            "ProxyHost" => self.proxy_host = text(),
            "ProxyPort" => set(&mut self.proxy_port, number(value)),
            "ProxyUsername" => self.proxy_user = text(),
            "ProxyPassword" => self.proxy_password = password_of(value),
            "ProxyTelnetCommand" => self.telnet_command = text(),
            "ProxyLocalCommand" => self.local_command = text(),
            "ProxyDNS" => set(&mut self.proxy_dns, dns_of(value)),
            "ProxyLocalhost" => set(&mut self.proxy_localhost, flag_of(value)),
            "Tunnel" => set(&mut self.tunnel, flag_of(value)),
            "TunnelHostName" => self.tunnel_host = text(),
            "TunnelPortNumber" => set(&mut self.tunnel_port, number(value)),
            "TunnelUserName" => self.tunnel_user = text(),
            "TunnelPassword" => self.tunnel_password = password_of(value),
            "TunnelPublicKeyFile" => self.tunnel_key = text(),
            _ => {}
        }
    }

    /// The lines of what differs from the default (passwords as kept, none where none can be kept).
    pub fn lines(&self) -> Vec<(String, String)> {
        let base = Self::default();
        let mut lines = Vec::new();
        let mut put = |differs: bool, key: &str, value: String| {
            if differs {
                lines.push((key.to_string(), value));
            }
        };
        put(
            self.timeout != base.timeout,
            "Timeout",
            self.timeout.to_string(),
        );
        put(
            self.ping != base.ping,
            "PingType",
            ping_value(self.ping).to_string(),
        );
        put(
            self.ping_interval != base.ping_interval,
            "PingIntervalSecs",
            self.ping_interval.to_string(),
        );
        put(
            self.family != base.family,
            "AddressFamily",
            family_value(self.family).to_string(),
        );
        self.proxy_lines(&base, &mut put);
        self.tunnel_lines(&base, &mut put);
        lines
    }

    fn proxy_lines(&self, base: &Self, put: &mut impl FnMut(bool, &str, String)) {
        put(
            self.proxy != base.proxy,
            "ProxyMethod",
            self.proxy.key().to_string(),
        );
        put(
            self.proxy_host != base.proxy_host,
            "ProxyHost",
            self.proxy_host.clone(),
        );
        put(
            self.proxy_port != base.proxy_port,
            "ProxyPort",
            self.proxy_port.to_string(),
        );
        put(
            !self.proxy_user.is_empty(),
            "ProxyUsername",
            self.proxy_user.clone(),
        );
        if let Some(kept) = kept(&self.proxy_password) {
            put(true, "ProxyPassword", kept);
        }
        put(
            self.telnet_command != base.telnet_command,
            "ProxyTelnetCommand",
            self.telnet_command.clone(),
        );
        put(
            !self.local_command.is_empty(),
            "ProxyLocalCommand",
            self.local_command.clone(),
        );
        put(
            self.proxy_dns != base.proxy_dns,
            "ProxyDNS",
            dns_value(self.proxy_dns).to_string(),
        );
        put(self.proxy_localhost, "ProxyLocalhost", "1".to_string());
    }

    fn tunnel_lines(&self, base: &Self, put: &mut impl FnMut(bool, &str, String)) {
        put(self.tunnel, "Tunnel", "1".to_string());
        put(
            !self.tunnel_host.is_empty(),
            "TunnelHostName",
            self.tunnel_host.clone(),
        );
        put(
            self.tunnel_port != base.tunnel_port,
            "TunnelPortNumber",
            self.tunnel_port.to_string(),
        );
        put(
            !self.tunnel_user.is_empty(),
            "TunnelUserName",
            self.tunnel_user.clone(),
        );
        if let Some(kept) = kept(&self.tunnel_password) {
            put(true, "TunnelPassword", kept);
        }
        put(
            !self.tunnel_key.is_empty(),
            "TunnelPublicKeyFile",
            self.tunnel_key.clone(),
        );
    }
}

/// A password as kept; none for an empty one or where the system keeps none.
fn kept(password: &str) -> Option<String> {
    (!password.is_empty())
        .then(|| secret::keep(password).ok())
        .flatten()
}

fn ping_of(value: &str) -> Option<Ping> {
    match value.trim() {
        "0" => Some(Ping::Off),
        "1" => Some(Ping::NullPacket),
        "2" => Some(Ping::DummyCommand),
        _ => None,
    }
}

fn ping_value(ping: Ping) -> u8 {
    match ping {
        Ping::Off => 0,
        Ping::NullPacket => 1,
        Ping::DummyCommand => 2,
    }
}

fn family_of(value: &str) -> Option<Family> {
    match value.trim() {
        "0" => Some(Family::Auto),
        "1" => Some(Family::V4),
        "2" => Some(Family::V6),
        _ => None,
    }
}

fn family_value(family: Family) -> u8 {
    match family {
        Family::Auto => 0,
        Family::V4 => 1,
        Family::V6 => 2,
    }
}

/// WinSCP keeps `ProxyDNS` as `(switch + 2) % 3` of `asOn` 0, `asOff` 1, `asAuto` 2: Yes 2, No 0, Auto 1.
fn dns_of(value: &str) -> Option<ProxyDns> {
    match value.trim() {
        "0" => Some(ProxyDns::No),
        "1" => Some(ProxyDns::Auto),
        "2" => Some(ProxyDns::Yes),
        _ => None,
    }
}

fn dns_value(dns: ProxyDns) -> u8 {
    match dns {
        ProxyDns::No => 0,
        ProxyDns::Auto => 1,
        ProxyDns::Yes => 2,
    }
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

#[cfg(test)]
#[path = "link_options_tests.rs"]
mod tests;
