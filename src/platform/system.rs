//! The system as FAR's info panel shows it (`far/infolist.cpp`, ADR-0012 §4): the computer and the user,
//! memory and power. One call reads it all; the panel calls it off the UI thread. And the system's
//! default sound (`beep`).

pub use super::os::{beep, system_facts};

/// What the info panel shows of the system.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemFacts {
    /// The computer's name in the format asked for (FAR's: the plain name when the system has none so).
    pub computer_name: String,
    /// The comment the computer is given (Windows: the server comment).
    pub computer_description: Option<String>,
    /// The user's name in the format asked for (the logon name when the system has none so).
    pub user_name: String,
    /// The comment the account is given.
    pub user_description: Option<String>,
    /// The account's level, where the system says it (a local account on Windows).
    pub access_level: Option<AccessLevel>,
    /// The program runs with an administrator's rights.
    pub elevated: bool,
    /// How the rights were split at logon (Windows' UAC), when they were.
    pub elevation: Option<Elevation>,
    pub memory: Option<MemoryFacts>,
    pub power: Option<PowerFacts>,
}

/// FAR's `USER_PRIV_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessLevel {
    Guest,
    User,
    Administrator,
    Unknown,
}

/// FAR's `TokenElevationTypeLimited` / `TokenElevationTypeFull`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    Limited,
    Full,
}

/// The computer's name as FAR's "Computer name format" chooses it (Windows' `COMPUTER_NAME_FORMAT`, in
/// its order); FAR's default is the physical NetBIOS name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComputerNameFormat {
    NetBios,
    DnsHostname,
    DnsDomain,
    DnsFullyQualified,
    #[default]
    PhysicalNetBios,
    PhysicalDnsHostname,
    PhysicalDnsDomain,
    PhysicalDnsFullyQualified,
}

impl ComputerNameFormat {
    pub const ALL: [Self; 8] = [
        Self::NetBios,
        Self::DnsHostname,
        Self::DnsDomain,
        Self::DnsFullyQualified,
        Self::PhysicalNetBios,
        Self::PhysicalDnsHostname,
        Self::PhysicalDnsDomain,
        Self::PhysicalDnsFullyQualified,
    ];

    /// Its name in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Self::NetBios => "netBios",
            Self::DnsHostname => "dnsHostname",
            Self::DnsDomain => "dnsDomain",
            Self::DnsFullyQualified => "dnsFullyQualified",
            Self::PhysicalNetBios => "physicalNetBios",
            Self::PhysicalDnsHostname => "physicalDnsHostname",
            Self::PhysicalDnsDomain => "physicalDnsDomain",
            Self::PhysicalDnsFullyQualified => "physicalDnsFullyQualified",
        }
    }

    /// An unknown name is the default.
    pub fn from_id(id: &str) -> Self {
        (Self::ALL.into_iter())
            .find(|format| format.id() == id)
            .unwrap_or_default()
    }
}

/// The user's name as FAR's "User name format" chooses it (Windows' `EXTENDED_NAME_FORMAT`, the ones FAR
/// lists); the logon name is FAR's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserNameFormat {
    #[default]
    Logon,
    FullyQualifiedDn,
    SamCompatible,
    Display,
    UniqueId,
    Canonical,
    UserPrincipal,
    ServicePrincipal,
    DnsDomain,
    GivenName,
    Surname,
}

impl UserNameFormat {
    pub const ALL: [Self; 11] = [
        Self::Logon,
        Self::FullyQualifiedDn,
        Self::SamCompatible,
        Self::Display,
        Self::UniqueId,
        Self::Canonical,
        Self::UserPrincipal,
        Self::ServicePrincipal,
        Self::DnsDomain,
        Self::GivenName,
        Self::Surname,
    ];

    /// Its name in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Self::Logon => "logon",
            Self::FullyQualifiedDn => "fullyQualifiedDn",
            Self::SamCompatible => "samCompatible",
            Self::Display => "display",
            Self::UniqueId => "uniqueId",
            Self::Canonical => "canonical",
            Self::UserPrincipal => "userPrincipal",
            Self::ServicePrincipal => "servicePrincipal",
            Self::DnsDomain => "dnsDomain",
            Self::GivenName => "givenName",
            Self::Surname => "surname",
        }
    }

    /// An unknown name is the default.
    pub fn from_id(id: &str) -> Self {
        (Self::ALL.into_iter())
            .find(|format| format.id() == id)
            .unwrap_or_default()
    }
}

/// The names' formats the info panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NameFormats {
    pub computer: ComputerNameFormat,
    pub user: UserNameFormat,
}

/// A total and what of it is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Amount {
    pub total: u64,
    pub available: u64,
}

/// FAR's memory lines (`GlobalMemoryStatusEx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryFacts {
    /// Memory that can be committed: physical memory and the page file.
    pub committable: Amount,
    /// The program's own address space.
    pub addressable: Amount,
    pub physical: Amount,
    /// The memory installed in the computer, part of it kept by the hardware.
    pub installed: Option<u64>,
}

/// FAR's power lines (`GetSystemPowerStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerFacts {
    pub ac_line: AcLine,
    /// `None`: the system does not know the battery's state.
    pub battery: Option<Battery>,
    /// Seconds the battery lasts, from now and from full; `None`: unknown.
    pub life: Option<u32>,
    pub full_life: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcLine {
    Offline,
    Online,
    BackupPower,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub charge: Charge,
    /// `None`: unknown.
    pub percent: Option<u8>,
    pub charging: bool,
}

/// FAR's `MInfoPowerStatusBC*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    High,
    Medium,
    Low,
    Critical,
    NoBattery,
    Unknown,
}
