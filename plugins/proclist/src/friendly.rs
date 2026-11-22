//! A process's friendly name (owner, 2026-11-15: as Task Manager names them): a service host by its
//! service ("Service Host: <service>", several by their group, the command line's `-k`), else its file's
//! description, else its image's name; the idle process "System Idle Process".

use nc_plugin_sdk::system::Service;

use crate::table::Process;

/// The service group a service host's command line names (`svchost.exe -k netsvcs -p`).
fn group(command_line: &str) -> Option<&str> {
    let mut words = command_line.split_whitespace();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("-k") || word.eq_ignore_ascii_case("/k") {
            return words.next();
        }
    }
    None
}

/// The friendly name of `process`, the services running in it given, its file's description given.
pub fn name(process: &Process, services: &[Service], description: Option<&str>) -> String {
    if process.is_total() {
        return process.name.clone();
    }
    if process.pid == 0 {
        return "System Idle Process".to_string();
    }
    if process.name.eq_ignore_ascii_case("svchost.exe") && !services.is_empty() {
        let shown = match services {
            [one] => one.display_name.as_str(),
            [first, ..] => (process.command_line.as_deref())
                .and_then(group)
                .unwrap_or(first.display_name.as_str()),
            [] => "",
        };
        return format!("Service Host: {shown}");
    }
    match description.map(str::trim).filter(|text| !text.is_empty()) {
        Some(description) => description.to_string(),
        None => process.name.clone(),
    }
}

#[cfg(test)]
#[path = "friendly_tests.rs"]
mod tests;
