//! Who holds a file open, as FAR's `get_locking_processes`: the Restart Manager's list, named by the
//! snapshot of processes, which also names those this process may not open.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::Path;
use std::ptr::null;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_MORE_DATA, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRebootReasonCriticalProcess,
    RmRebootReasonCriticalService, RmRebootReasonDetectedSelf, RmRebootReasonSessionMismatch,
    RmRegisterResources, RmStartSession,
};

use super::wide_os;
use crate::platform::files::{HoldNote, Holders};

/// FAR's `MaxProcesses`.
const MAX_NAMED: usize = 5;
/// How many entries the first look has room for (FAR's).
const FIRST_LOOK: u32 = 128;

/// Access denied, a sharing or lock violation, a device or drive in use, files open on a share: the
/// errors FAR looks for a holder after.
pub fn may_be_held(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33 | 108 | 2401 | 2404))
}

/// Who holds any of `paths`; empty when nobody can be named. Asks the system, which takes a moment.
pub fn file_holders(paths: &[&Path]) -> Holders {
    let Some((infos, reasons)) = restart_manager_list(paths) else {
        return Holders::default();
    };
    if infos.is_empty() {
        return Holders {
            notes: notes(reasons),
            ..Holders::default()
        };
    }
    let names = process_names();
    // The Restart Manager gives a service apart from its process: collapsed, as FAR.
    let mut unique = BTreeMap::new();
    for info in &infos {
        let id = info.Process.dwProcessId;
        unique
            .entry(id)
            .or_insert_with(|| describe(id, &names, info));
        if unique.len() == MAX_NAMED {
            break;
        }
    }
    Holders {
        programs: unique.into_values().collect(),
        more: infos.len().saturating_sub(MAX_NAMED),
        notes: notes(reasons),
    }
}

fn notes(reasons: u32) -> Vec<HoldNote> {
    let has = |reason: i32| reasons & u32::try_from(reason).unwrap_or(0) != 0;
    [
        (RmRebootReasonSessionMismatch, HoldNote::OtherSession),
        (RmRebootReasonCriticalProcess, HoldNote::CriticalProcess),
        (RmRebootReasonCriticalService, HoldNote::CriticalService),
        (RmRebootReasonDetectedSelf, HoldNote::ThisProcess),
    ]
    .into_iter()
    .filter(|(reason, _)| has(*reason))
    .map(|(_, note)| note)
    .collect()
}

/// FAR's `format_process_name`: `name (PID 7, application, [service])`.
fn describe(id: u32, names: &HashMap<u32, String>, info: &RM_PROCESS_INFO) -> String {
    let name = names.get(&id).map_or("Unknown", String::as_str);
    let application = text_of(&info.strAppName);
    let service = text_of(&info.strServiceShortName);
    let mut described = format!("{name} (PID {id}");
    if !application.is_empty() {
        described.push_str(&format!(", {application}"));
    }
    if !service.is_empty() {
        described.push_str(&format!(", [{service}]"));
    }
    described.push(')');
    described
}

fn text_of(units: &[u16]) -> String {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// The processes the Restart Manager finds holding `paths`, and its reasons they cannot be restarted.
fn restart_manager_list(paths: &[&Path]) -> Option<(Vec<RM_PROCESS_INFO>, u32)> {
    let mut session = 0;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `session` is a live local; `key` has the room the call requires.
    if unsafe { RmStartSession(&raw mut session, 0, key.as_mut_ptr()) } != 0 {
        return None;
    }
    let list = registered_list(session, paths);
    // SAFETY: the session was started above and is ended once.
    unsafe { RmEndSession(session) };
    list
}

fn registered_list(session: u32, paths: &[&Path]) -> Option<(Vec<RM_PROCESS_INFO>, u32)> {
    let wide: Vec<Vec<u16>> = paths.iter().map(|path| wide_os(path.as_os_str())).collect();
    let names: Vec<*const u16> = wide.iter().map(|name| name.as_ptr()).collect();
    let count = u32::try_from(names.len()).ok()?;
    // SAFETY: `names` holds `count` NUL-terminated strings for the call; no applications or services.
    let registered =
        unsafe { RmRegisterResources(session, count, names.as_ptr(), 0, null(), 0, null()) };
    if registered != 0 {
        return None;
    }
    let mut room = FIRST_LOOK;
    loop {
        // SAFETY: RM_PROCESS_INFO is plain data, all zero bytes being a valid value.
        let mut infos = vec![unsafe { std::mem::zeroed::<RM_PROCESS_INFO>() }; room as usize];
        let (mut needed, mut count, mut reasons) = (0, room, 0);
        // SAFETY: `infos` holds `count` entries; the other three are live locals.
        let listed = unsafe {
            RmGetList(
                session,
                &raw mut needed,
                &raw mut count,
                infos.as_mut_ptr(),
                &raw mut reasons,
            )
        };
        if listed == ERROR_MORE_DATA {
            room = needed;
            continue;
        }
        if listed != 0 {
            return None;
        }
        infos.truncate(count as usize);
        return Some((infos, reasons));
    }
}

/// Every process's file name by its id.
fn process_names() -> HashMap<u32, String> {
    let mut names = HashMap::new();
    // SAFETY: plain values.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return names;
    }
    // SAFETY: PROCESSENTRY32W is plain data, all zero bytes being a valid value.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `snapshot` is live; `entry` is live and its size is set.
    let mut found = unsafe { Process32FirstW(snapshot, &raw mut entry) } != 0;
    while found {
        names.insert(entry.th32ProcessID, text_of(&entry.szExeFile));
        // SAFETY: as above.
        found = unsafe { Process32NextW(snapshot, &raw mut entry) } != 0;
    }
    // SAFETY: the snapshot is owned here and closed once.
    unsafe { CloseHandle(snapshot) };
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn only_errors_a_holder_can_cause_are_looked_into() {
        for code in [5, 32, 33, 108, 2401, 2404] {
            assert!(may_be_held(&io::Error::from_raw_os_error(code)), "{code}");
        }
        assert!(!may_be_held(&io::Error::from_raw_os_error(2)));
        assert!(!may_be_held(&io::Error::from(io::ErrorKind::InvalidInput)));
    }

    #[test]
    fn nobody_holds_a_file_nobody_opened() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("free.txt");
        std::fs::write(&path, "").unwrap();
        let holders = file_holders(&[&path]);
        assert!(holders.programs.is_empty(), "{holders:?}");
        assert_eq!(holders.more, 0);
    }

    #[test]
    fn a_missing_file_has_no_holders() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            file_holders(&[&temp.path().join("none")]),
            Holders::default()
        );
    }

    /// A program of the test's own holding a file of the test's own.
    #[test]
    fn a_program_holding_the_file_is_named_with_its_process_id() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("held.txt");
        let mut holder = crate::platform::process::without_console(
            Command::new("cmd.exe")
                .args(["/d", "/k"])
                .stdin(Stdio::piped())
                .stdout(File::create(&path).unwrap())
                .stderr(Stdio::null()),
        )
        .spawn()
        .unwrap();

        let wanted = format!("cmd.exe (PID {}", holder.id());
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut holders = file_holders(&[&path]);
        while !holders
            .programs
            .iter()
            .any(|name| name.starts_with(&wanted))
            && Instant::now() < deadline
        {
            std::thread::yield_now();
            holders = file_holders(&[&path]);
        }
        holder.kill().unwrap();
        holder.wait().unwrap();

        assert!(
            holders
                .programs
                .iter()
                .any(|name| name.starts_with(&wanted)),
            "{holders:?}"
        );
    }

    #[test]
    fn a_process_is_described_as_far_does() {
        // SAFETY: RM_PROCESS_INFO is plain data, all zero bytes being a valid value.
        let mut info: RM_PROCESS_INFO = unsafe { std::mem::zeroed() };
        let names = HashMap::from([(7, "tool.exe".to_string())]);
        assert_eq!(describe(7, &names, &info), "tool.exe (PID 7)");
        assert_eq!(describe(8, &names, &info), "Unknown (PID 8)");
        let wide = |text: &str| text.encode_utf16().collect::<Vec<_>>();
        let app = wide("Tool");
        info.strAppName[..app.len()].copy_from_slice(&app);
        let service = wide("Svc");
        info.strServiceShortName[..service.len()].copy_from_slice(&service);
        assert_eq!(describe(7, &names, &info), "tool.exe (PID 7, Tool, [Svc])");
    }

    #[test]
    fn the_reasons_become_notes() {
        let reasons = u32::try_from(RmRebootReasonCriticalService | RmRebootReasonDetectedSelf);
        assert_eq!(
            notes(reasons.unwrap()),
            [HoldNote::CriticalService, HoldNote::ThisProcess]
        );
        assert!(notes(0).is_empty());
    }
}
