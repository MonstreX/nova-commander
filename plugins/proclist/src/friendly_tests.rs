use super::*;
use crate::table::TOTAL;

fn process(pid: u32, name: &str, command_line: &str) -> Process {
    Process {
        pid,
        name: name.to_string(),
        command_line: Some(command_line.to_string()),
        ..Process::default()
    }
}

fn service(display_name: &str) -> Service {
    Service {
        name: display_name.replace(' ', ""),
        display_name: display_name.to_string(),
    }
}

#[test]
fn a_service_host_is_named_by_its_service_or_its_group() {
    let host = process(
        900,
        "svchost.exe",
        r"C:\Windows\system32\svchost.exe -k netsvcs -p",
    );
    assert_eq!(
        name(&host, &[service("Windows Update")], None),
        "Service Host: Windows Update"
    );
    let many = [service("Themes"), service("Task Scheduler")];
    assert_eq!(name(&host, &many, None), "Service Host: netsvcs");
    let no_group = process(901, "SVCHOST.EXE", "svchost.exe");
    assert_eq!(
        name(&no_group, &many, None),
        "Service Host: Themes",
        "its first then"
    );
    assert_eq!(
        name(&no_group, &[], Some("Host Process for Windows Services")),
        "Host Process for Windows Services",
        "no service known: its description"
    );
}

#[test]
fn a_process_is_named_by_its_description_or_its_image() {
    let app = process(1000, "notepad.exe", "notepad.exe");
    assert_eq!(name(&app, &[], Some("Notepad")), "Notepad");
    assert_eq!(name(&app, &[], Some("  ")), "notepad.exe");
    assert_eq!(name(&app, &[], None), "notepad.exe");
    assert_eq!(
        name(&process(0, "Idle", ""), &[], None),
        "System Idle Process"
    );
    let total = Process {
        pid: TOTAL,
        name: "_Total".to_string(),
        ..Process::default()
    };
    assert_eq!(name(&total, &[], Some("x")), "_Total");
}
