use std::time::Instant;

use super::*;
use crate::info::text;
use nc_plugin_sdk::system::processes;

fn handle(type_name: &str, name: HandleName) -> HandleInfo {
    HandleInfo {
        value: 4,
        access: 0,
        type_name: type_name.to_string(),
        name,
    }
}

fn table() -> Table {
    let mut none = |_| None;
    let records = processes().unwrap();
    Table::default().next(&records, Instant::now(), 1, &mut none)
}

#[test]
fn a_handles_object_is_named_as_far_names_it() {
    let table = table();
    let named = |handle: &HandleInfo| handle_name(handle, &table, "S-1-5-21-9");
    assert_eq!(
        named(&handle("Process", HandleName::Process(4))),
        "System (4)"
    );
    assert_eq!(
        named(&handle("Process", HandleName::Process(3))),
        "<unknown> (3)",
        "a process not listed"
    );
    let thread = HandleName::Thread {
        id: 12,
        description: "worker".to_string(),
    };
    assert_eq!(named(&handle("Thread", thread)), "TID: 12, worker");
    let plain = HandleName::Thread {
        id: 12,
        description: String::new(),
    };
    assert_eq!(named(&handle("Thread", plain)), "TID: 12");
    let key = HandleName::Text(r"\REGISTRY\USER\S-1-5-21-9\Software".to_string());
    assert_eq!(named(&handle("Key", key)), r"HKCU\Software");
    let file = HandleName::Text(r"\Device\Null".to_string());
    assert_eq!(named(&handle("File", file)), r"\Device\Null");
    assert_eq!(named(&handle("Event", HandleName::None)), "");
}

/// The tests' own process gathered with every option: its text has each of FAR's parts.
#[test]
fn the_tests_own_process_gives_every_part() {
    let table = table();
    let own = table.process(std::process::id()).expect("listed").clone();
    let mut own = own;
    own.path = Some(
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    own.command_line = Some("test".to_string());
    let options = Options {
        export_module_version: true,
        export_handles: true,
        export_handles_unnamed: true,
        ..Options::default()
    };
    let facts = gather(&own, &table, &options, &|| false);
    assert!(facts.owner.is_some(), "its owner");
    assert!(facts.started.is_some(), "its start");
    let memory = facts.memory.as_ref().expect("its memory");
    assert!(memory.current_folder.is_some());
    assert!(!memory.environment.is_empty());
    assert!(
        facts
            .modules
            .as_ref()
            .is_some_and(|modules| modules.len() > 3)
    );
    assert!(
        facts
            .handles
            .as_ref()
            .is_some_and(|handles| !handles.is_empty())
    );
    let text = text(&facts, &options);
    for part in [
        "Module:",
        "PID:",
        "Owner:",
        "Started at:",
        "Current Directory:",
        "Environment:",
        "Processor Time:",
        "Modules:",
        "Handles:",
    ] {
        assert!(text.contains(part), "{part}");
    }
}

#[test]
fn total_gathers_nothing_of_a_process() {
    let table = table();
    let total = table.processes[0].clone();
    let facts = gather(&total, &table, &Options::default(), &|| false);
    assert!(facts.owner.is_none() && facts.memory.is_none() && facts.modules.is_none());
    assert!(facts.started.is_none());
}
