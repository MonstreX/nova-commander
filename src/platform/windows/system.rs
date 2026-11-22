//! The system as FAR's info panel reads it (`far/infolist.cpp`): the computer and user names and
//! comments (`GetComputerNameEx`, `NetServerGetInfo`, `GetUserName`, `NetUserGetInfo`), the rights the
//! program runs with (the token's elevation), memory (`GlobalMemoryStatusEx`) and power
//! (`GetSystemPowerStatus`).

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::NetworkManagement::NetManagement::{
    NERR_Success, NetApiBufferFree, NetServerGetInfo, NetUserGetInfo, SERVER_INFO_101, USER_INFO_1,
    USER_PRIV_ADMIN, USER_PRIV_GUEST, USER_PRIV_USER,
};
use windows_sys::Win32::Security::Authentication::Identity::{
    EXTENDED_NAME_FORMAT, GetUserNameExW, NameCanonical, NameDisplay, NameDnsDomain,
    NameFullyQualifiedDN, NameGivenName, NameSamCompatible, NameServicePrincipal, NameSurname,
    NameUniqueId, NameUserPrincipal,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation, TokenElevationType,
    TokenElevationTypeFull, TokenElevationTypeLimited,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::SystemInformation::{
    COMPUTER_NAME_FORMAT, ComputerNameDnsDomain, ComputerNameDnsFullyQualified,
    ComputerNameDnsHostname, ComputerNameNetBIOS, ComputerNamePhysicalDnsDomain,
    ComputerNamePhysicalDnsFullyQualified, ComputerNamePhysicalDnsHostname,
    ComputerNamePhysicalNetBIOS, GetComputerNameExW, GetPhysicallyInstalledSystemMemory,
    GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
use windows_sys::Win32::UI::WindowsAndMessaging::MB_OK;

use super::drive::text_at;
use super::wide;
use crate::platform::system::{
    AcLine, AccessLevel, Amount, Battery, Charge, ComputerNameFormat, Elevation, MemoryFacts,
    NameFormats, PowerFacts, SystemFacts, UserNameFormat,
};

/// The system's default sound (`MessageBeep(0)`, as FAR's plugins sound it).
pub fn beep() {
    // SAFETY: no pointers; no sound (no device) is nothing to tell.
    unsafe { MessageBeep(MB_OK) };
}

/// Everything the info panel shows of the system (slow parts: the network API calls), the names in the
/// formats asked for (FAR's `InfoComputerNameFormat`, `InfoUserNameFormat`).
pub fn system_facts(names: NameFormats) -> SystemFacts {
    let user_name = user_name();
    let account = (!user_name.is_empty())
        .then(|| account(&user_name))
        .flatten();
    let (elevated, elevation) = elevation();
    SystemFacts {
        // FAR: the plain NetBIOS name when the format asked for has none.
        computer_name: computer_name(computer_format(names.computer))
            .or_else(|| computer_name(ComputerNameNetBIOS))
            .unwrap_or_default(),
        computer_description: computer_description(),
        user_description: account.as_ref().and_then(|account| account.0.clone()),
        access_level: account.map(|account| account.1),
        user_name: user_format(names.user)
            .and_then(extended_user_name)
            .unwrap_or(user_name),
        elevated,
        elevation,
        memory: memory(),
        power: power(),
    }
}

fn computer_format(format: ComputerNameFormat) -> COMPUTER_NAME_FORMAT {
    match format {
        ComputerNameFormat::NetBios => ComputerNameNetBIOS,
        ComputerNameFormat::DnsHostname => ComputerNameDnsHostname,
        ComputerNameFormat::DnsDomain => ComputerNameDnsDomain,
        ComputerNameFormat::DnsFullyQualified => ComputerNameDnsFullyQualified,
        ComputerNameFormat::PhysicalNetBios => ComputerNamePhysicalNetBIOS,
        ComputerNameFormat::PhysicalDnsHostname => ComputerNamePhysicalDnsHostname,
        ComputerNameFormat::PhysicalDnsDomain => ComputerNamePhysicalDnsDomain,
        ComputerNameFormat::PhysicalDnsFullyQualified => ComputerNamePhysicalDnsFullyQualified,
    }
}

/// `GetComputerNameExW`; `None` when the system has no such name (an empty DNS domain is a name).
fn computer_name(format: COMPUTER_NAME_FORMAT) -> Option<String> {
    let mut buffer = vec![0u16; 256];
    let mut length = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: `buffer` holds `length` characters; `length` is a valid in/out value.
    let ok = unsafe { GetComputerNameExW(format, buffer.as_mut_ptr(), &raw mut length) };
    if ok == 0 {
        return None;
    }
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

/// The extended name's format; `None` for the logon name (FAR's `NameUnknown`).
fn user_format(format: UserNameFormat) -> Option<EXTENDED_NAME_FORMAT> {
    Some(match format {
        UserNameFormat::Logon => return None,
        UserNameFormat::FullyQualifiedDn => NameFullyQualifiedDN,
        UserNameFormat::SamCompatible => NameSamCompatible,
        UserNameFormat::Display => NameDisplay,
        UserNameFormat::UniqueId => NameUniqueId,
        UserNameFormat::Canonical => NameCanonical,
        UserNameFormat::UserPrincipal => NameUserPrincipal,
        UserNameFormat::ServicePrincipal => NameServicePrincipal,
        UserNameFormat::DnsDomain => NameDnsDomain,
        UserNameFormat::GivenName => NameGivenName,
        UserNameFormat::Surname => NameSurname,
    })
}

/// `GetUserNameExW`; `None` when the account has no such name (a local account outside a domain has
/// few of them).
fn extended_user_name(format: EXTENDED_NAME_FORMAT) -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let mut length = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: `buffer` holds `length` characters; `length` is a valid in/out value.
    let ok = unsafe { GetUserNameExW(format, buffer.as_mut_ptr(), &raw mut length) };
    if !ok {
        return None;
    }
    // The length given back leaves the closing NUL out.
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn user_name() -> String {
    let mut buffer = vec![0u16; 257];
    let mut length = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: `buffer` holds `length` characters; `length` is a valid in/out value.
    let ok = unsafe { GetUserNameW(buffer.as_mut_ptr(), &raw mut length) };
    if ok == 0 {
        return String::new();
    }
    // The length counts the closing NUL.
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..length.saturating_sub(1)])
}

/// A network API buffer, freed once.
struct NetBuffer(*mut u8);

impl Drop for NetBuffer {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the buffer came from a NetApi call and is freed only here.
            unsafe { NetApiBufferFree(self.0.cast()) };
        }
    }
}

fn comment(text: *const u16) -> Option<String> {
    // SAFETY: the callers pass a field of a NetApi structure: null or a NUL-terminated string.
    let text = unsafe { text_at(text) };
    (!text.is_empty()).then_some(text)
}

/// The local server's comment (FAR's `NetServerGetInfo(nullptr, 101)`).
fn computer_description() -> Option<String> {
    let mut buffer = NetBuffer(std::ptr::null_mut());
    // SAFETY: the local server (null name); `buffer.0` receives a buffer the system allocates.
    if unsafe { NetServerGetInfo(std::ptr::null(), 101, &raw mut buffer.0) } != NERR_Success
        || buffer.0.is_null()
    {
        return None;
    }
    // SAFETY: level 101 fills a SERVER_INFO_101 that lives as long as `buffer`.
    let info = unsafe { &*buffer.0.cast::<SERVER_INFO_101>() };
    comment(info.sv101_comment)
}

/// The local account's comment and level (FAR's `NetUserGetInfo(nullptr, name, 1)`); `None` for an
/// account the local computer does not keep (a domain or a Microsoft account may still be found).
fn account(name: &str) -> Option<(Option<String>, AccessLevel)> {
    let name = wide(name);
    let mut buffer = NetBuffer(std::ptr::null_mut());
    // SAFETY: the local server; `name` is NUL-terminated; `buffer.0` receives the system's buffer.
    if unsafe { NetUserGetInfo(std::ptr::null(), name.as_ptr(), 1, &raw mut buffer.0) }
        != NERR_Success
        || buffer.0.is_null()
    {
        return None;
    }
    // SAFETY: level 1 fills a USER_INFO_1 that lives as long as `buffer`.
    let info = unsafe { &*buffer.0.cast::<USER_INFO_1>() };
    let level = match info.usri1_priv {
        USER_PRIV_GUEST => AccessLevel::Guest,
        USER_PRIV_USER => AccessLevel::User,
        USER_PRIV_ADMIN => AccessLevel::Administrator,
        _ => AccessLevel::Unknown,
    };
    Some((comment(info.usri1_comment), level))
}

/// Whether this process has the administrator's rights.
pub(super) fn is_elevated() -> bool {
    elevation().0
}

/// FAR's `is_admin` and `elevation_type`: whether the token is elevated, and how UAC split it.
fn elevation() -> (bool, Option<Elevation>) {
    let mut raw: HANDLE = std::ptr::null_mut();
    // SAFETY: the pseudo handle of this process; `raw` is a valid out value.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw) } == 0 {
        return (false, None);
    }
    // SAFETY: a new handle owned by nothing else.
    let token = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
    let mut elevated = TOKEN_ELEVATION::default();
    let mut kind = 0i32;
    let read = |class, out: *mut core::ffi::c_void, size: usize| {
        let mut returned = 0u32;
        let size = u32::try_from(size).unwrap_or(0);
        // SAFETY: an open token with query access; `out` holds `size` writable bytes.
        unsafe {
            GetTokenInformation(
                token.as_raw_handle() as HANDLE,
                class,
                out,
                size,
                &raw mut returned,
            ) != 0
        }
    };
    let is_elevated = read(
        TokenElevation,
        (&raw mut elevated).cast(),
        size_of::<TOKEN_ELEVATION>(),
    ) && elevated.TokenIsElevated != 0;
    let split = read(TokenElevationType, (&raw mut kind).cast(), size_of::<i32>());
    let elevation = if !split {
        None
    } else if kind == TokenElevationTypeLimited {
        Some(Elevation::Limited)
    } else if kind == TokenElevationTypeFull {
        Some(Elevation::Full)
    } else {
        None
    };
    (is_elevated, elevation)
}

fn memory() -> Option<MemoryFacts> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).ok()?,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: `status` is a MEMORYSTATUSEX with its length set.
    if unsafe { GlobalMemoryStatusEx(&raw mut status) } == 0 {
        return None;
    }
    let mut kilobytes = 0u64;
    // SAFETY: `kilobytes` is a valid out value.
    let installed = (unsafe { GetPhysicallyInstalledSystemMemory(&raw mut kilobytes) } != 0)
        .then(|| kilobytes.saturating_mul(1024));
    let amount = |total, available| Amount { total, available };
    Some(MemoryFacts {
        committable: amount(status.ullTotalPageFile, status.ullAvailPageFile),
        addressable: amount(status.ullTotalVirtual, status.ullAvailVirtual),
        physical: amount(status.ullTotalPhys, status.ullAvailPhys),
        installed,
    })
}

fn power() -> Option<PowerFacts> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` is a valid out structure.
    if unsafe { GetSystemPowerStatus(&raw mut status) } == 0 {
        // FAR shows the lines as unknown.
        status.ACLineStatus = 255;
        status.BatteryFlag = 255;
        status.BatteryLifePercent = 255;
        status.BatteryLifeTime = u32::MAX;
        status.BatteryFullLifeTime = u32::MAX;
    }
    Some(power_facts(&status))
}

/// FAR's reading of `SYSTEM_POWER_STATUS`.
fn power_facts(status: &SYSTEM_POWER_STATUS) -> PowerFacts {
    let ac_line = match status.ACLineStatus {
        0 => AcLine::Offline,
        1 => AcLine::Online,
        2 => AcLine::BackupPower,
        _ => AcLine::Unknown,
    };
    // BATTERY_FLAG_HIGH 1, LOW 2, CRITICAL 4, CHARGING 8, NO_BATTERY 128, UNKNOWN 255.
    let battery = (status.BatteryFlag != 255).then(|| Battery {
        charge: match status.BatteryFlag & (1 | 2 | 4 | 128) {
            0 => Charge::Medium,
            1 => Charge::High,
            2 => Charge::Low,
            4 => Charge::Critical,
            128 => Charge::NoBattery,
            _ => Charge::Unknown,
        },
        percent: (status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent),
        charging: status.BatteryFlag & 8 != 0,
    });
    let seconds = |value: u32| (value != u32::MAX).then_some(value);
    PowerFacts {
        ac_line,
        battery,
        life: seconds(status.BatteryLifeTime),
        full_life: seconds(status.BatteryFullLifeTime),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_is_read() {
        let facts = system_facts(NameFormats::default());
        assert!(!facts.computer_name.is_empty());
        assert!(!facts.user_name.is_empty());
        let memory = facts.memory.unwrap();
        assert!(memory.physical.total > 0 && memory.physical.available <= memory.physical.total);
        assert!(facts.power.is_some());
    }

    #[test]
    fn the_names_come_in_the_format_asked_for() {
        let logon = system_facts(NameFormats::default()).user_name;
        let sam = system_facts(NameFormats {
            computer: ComputerNameFormat::DnsHostname,
            user: UserNameFormat::SamCompatible,
        });
        assert!(!sam.computer_name.is_empty());
        assert!(
            sam.user_name.ends_with(&format!(r"\{logon}")),
            r"DOMAIN\user: {}",
            sam.user_name
        );
        // A name the account has not (no domain): the logon name, as FAR shows it.
        let unique = extended_user_name(NameUniqueId).unwrap_or(logon.clone());
        let shown = system_facts(NameFormats {
            user: UserNameFormat::UniqueId,
            ..NameFormats::default()
        });
        assert_eq!(shown.user_name, unique);
    }

    #[test]
    fn power_status_reads_as_far_reads_it() {
        let status = |ac, flag, percent, life| SYSTEM_POWER_STATUS {
            ACLineStatus: ac,
            BatteryFlag: flag,
            BatteryLifePercent: percent,
            SystemStatusFlag: 0,
            BatteryLifeTime: life,
            BatteryFullLifeTime: u32::MAX,
        };
        let desktop = power_facts(&status(1, 128, 255, u32::MAX));
        assert_eq!(desktop.ac_line, AcLine::Online);
        assert_eq!(
            desktop.battery,
            Some(Battery {
                charge: Charge::NoBattery,
                percent: None,
                charging: false
            })
        );
        assert_eq!((desktop.life, desktop.full_life), (None, None));
        let laptop = power_facts(&status(0, 1 | 8, 80, 3600));
        assert_eq!(laptop.ac_line, AcLine::Offline);
        let battery = laptop.battery.unwrap();
        assert_eq!(
            (battery.charge, battery.percent, battery.charging),
            (Charge::High, Some(80), true)
        );
        assert_eq!(laptop.life, Some(3600));
        assert_eq!(
            power_facts(&status(1, 0, 50, 0)).battery.unwrap().charge,
            Charge::Medium
        );
        assert_eq!(power_facts(&status(9, 255, 0, 0)).battery, None);
        assert_eq!(power_facts(&status(9, 255, 0, 0)).ac_line, AcLine::Unknown);
        assert_eq!(
            power_facts(&status(1, 2 | 4, 5, 0)).battery.unwrap().charge,
            Charge::Unknown
        );
    }
}
