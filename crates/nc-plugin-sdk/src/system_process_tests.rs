//! The processes' wrappers on this computer: the tests' own process found and read, a child of theirs
//! (a `ping` waiting) moved in priority and ended; a system file's version read.

use std::process::{Child, Command, Stdio};

use super::*;
use crate::system::{
    KillError, PRIORITY_CLASSES, file_version, process_details, step_priority, terminate,
    top_windows,
};

#[test]
fn the_tests_own_process_is_among_the_processes_with_its_parent_and_memory() {
    let all = processes().unwrap();
    let me = std::process::id();
    let own = all
        .iter()
        .find(|record| record.pid == me)
        .expect("the tests' process");
    let exe = std::env::current_exe().unwrap();
    let name = exe.file_name().unwrap().to_string_lossy();
    assert!(own.name.eq_ignore_ascii_case(&name), "{}", own.name);
    assert!(own.threads >= 1 && own.handles > 0);
    assert!(own.working_set > 0 && own.private_bytes > 0 && own.virtual_bytes >= own.working_set);
    assert!(own.created > 0);
    assert!(
        all.iter().any(|record| record.pid == own.parent),
        "its parent runs"
    );
    let idle = all
        .iter()
        .find(|record| record.pid == 0)
        .expect("the idle process");
    assert!(idle.name.is_empty());
    assert!(all.iter().any(|record| record.pid == 4), "System");
}

#[test]
fn the_tests_own_process_tells_its_path_command_line_and_owner() {
    let details = process_details(std::process::id()).expect("opened");
    let exe = std::env::current_exe().unwrap();
    let path = details.path.expect("its path");
    assert!(path.eq_ignore_ascii_case(&exe.to_string_lossy()), "{path}");
    let line = details.command_line.expect("its command line");
    assert!(
        line.to_lowercase()
            .contains(&exe.file_name().unwrap().to_string_lossy().to_lowercase()),
        "{line}"
    );
    assert!(!details.wow64);
    assert!(details.created.is_some());
    let owner = details.owner.expect("its owner");
    let user = std::env::var("USERNAME").unwrap();
    assert!(owner.user.eq_ignore_ascii_case(&user), "{owner:?}");
    assert!(owner.sid.starts_with("S-1-5-"), "{owner:?}");
}

/// A child of the tests' that waits, ended when dropped (none left running).
struct Waiting(Child);

impl Drop for Waiting {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn waiting() -> Waiting {
    use std::os::windows::process::CommandExt;
    // Started normal whatever the tests' own class (a CI runner's is below normal, a child inherits it).
    let child = Command::new("ping")
        .creation_flags(PRIORITY_CLASSES[2].0)
        .args(["-n", "60", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Waiting(child)
}

fn base_priority(pid: u32) -> i32 {
    let all = processes().unwrap();
    all.iter()
        .find(|record| record.pid == pid)
        .unwrap()
        .base_priority
}

#[test]
fn a_childs_priority_steps_down_and_up_and_stops_at_the_scale_s_end() {
    let child = waiting();
    let pid = child.0.id();
    assert_eq!(base_priority(pid), PRIORITY_CLASSES[2].1, "normal");
    assert!(step_priority(pid, false).unwrap());
    assert_eq!(base_priority(pid), PRIORITY_CLASSES[1].1, "below normal");
    assert!(step_priority(pid, false).unwrap());
    assert!(!step_priority(pid, false).unwrap(), "idle is the lowest");
    assert_eq!(base_priority(pid), PRIORITY_CLASSES[0].1);
    assert!(step_priority(pid, true).unwrap());
    assert_eq!(base_priority(pid), PRIORITY_CLASSES[1].1);
}

#[test]
fn a_child_is_ended_and_a_process_gone_is_an_error() {
    let mut child = waiting();
    let pid = child.0.id();
    terminate(pid, false).unwrap();
    let status = child.0.wait().unwrap();
    assert_eq!(status.code(), Some(1067), "ERROR_PROCESS_ABORTED");
    drop(child);
    // No such process to open: one that never runs (Windows' process numbers are multiples of 4).
    // Not the child's number again: once its last handle is closed the system may give it to another
    // process at once (the tests side by side start some), which this would end.
    assert!(matches!(terminate(1, false), Err(KillError::Other(_))));
}

#[test]
fn the_system_process_may_not_be_ended_without_the_debug_privilege() {
    // Only asked to open it for ending: System (4) refuses an ordinary user.
    match terminate(4, false) {
        Err(KillError::Denied) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_system_file_tells_its_version_and_description() {
    let system = std::env::var("SystemRoot").unwrap();
    let kernel = std::path::Path::new(&system).join(r"System32\kernel32.dll");
    let version = file_version(&kernel).expect("a version resource");
    assert!(version.version.is_some_and(|text| text.contains('.')));
    assert!(version.description.is_some_and(|text| !text.is_empty()));
    assert!(file_version(std::path::Path::new(r"C:\no such file.dll")).is_none());
}

#[test]
fn the_windows_and_the_processors_are_counted() {
    let windows = top_windows();
    for window in windows.values().take(20) {
        assert!(
            window.title().encode_utf16().count() < 260,
            "FAR's MAX_PATH"
        );
        let (style, _) = window.styles();
        assert_eq!(
            style & 0x4000_0000,
            0,
            "no child window (WS_CHILD) is a top one"
        );
    }
    assert!(logical_processors() >= 1);
}

/// The longest command line Windows takes (32 767 units with its end) is read whole: the buffer grows to
/// what the system says.
#[test]
fn the_longest_command_line_is_read_whole() {
    let comment = "x".repeat(32_000);
    let child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Start-Sleep 30; #{comment}"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let child = Waiting(child);
    let details = process_details(child.0.id()).expect("the child opened");
    let line = details.command_line.expect("its command line");
    assert!(line.len() > 32_000, "{}", line.len());
    assert!(line.ends_with(&format!("{comment}\"")), "the whole text");
}
