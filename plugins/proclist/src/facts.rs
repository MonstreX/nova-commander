//! What the text of a process is made of, gathered from the system as FAR's `GetFiles` asks it: the
//! parent's name from the list, the file's version, the owner asked again (FAR asks WMI at that moment),
//! the times now, the memory, the window, the modules and the handles as the options ask; a handle's
//! object named as FAR names it (a process by its name and number, a thread by `TID:` and its
//! description, a registry key shortened).

use nc_plugin_sdk::system::{
    self, HandleInfo, HandleName, file_version, local_time, now_ticks, process_details,
    process_handles, process_memory, process_modules, top_windows,
};

use crate::info::{Facts, HandleLine, WindowFacts, registry_name};
use crate::options::Options;
use crate::table::{Process, Table};

/// FAR's `GetVersionInfo` reads a path without its `\??\`.
fn version_of(path: &str) -> Option<system::FileVersion> {
    file_version(path.strip_prefix(r"\??\").unwrap_or(path).as_ref())
}

/// The object's name of a handle (FAR's `GetNameByType`); `sid` the user's, for the registry's keys.
pub fn handle_name(handle: &HandleInfo, table: &Table, sid: &str) -> String {
    match &handle.name {
        HandleName::None => String::new(),
        HandleName::Process(pid) => {
            let name = (table.process(*pid)).map_or("<unknown>", |process| process.name.as_str());
            format!("{name} ({pid})")
        }
        HandleName::Thread { id, description } => {
            let mut name = if *id == 0 {
                String::new()
            } else {
                format!("TID: {id}")
            };
            if !description.is_empty() {
                name.push_str(", ");
                name.push_str(description);
            }
            name
        }
        HandleName::Text(text) if handle.type_name.eq_ignore_ascii_case("Key") => {
            registry_name(text, sid)
        }
        HandleName::Text(text) => text.clone(),
    }
}

/// The SID of the user the program runs as (FAR's `GetUserAccountID`).
fn own_sid() -> String {
    (process_details(std::process::id()))
        .and_then(|details| details.owner)
        .map(|owner| owner.sid)
        .unwrap_or_default()
}

fn handles(pid: u32, table: &Table, stop: &dyn Fn() -> bool) -> Option<Vec<HandleLine>> {
    let handles = process_handles(pid, stop).ok()?;
    let sid = own_sid();
    Some(
        (handles.iter())
            .map(|handle| HandleLine {
                value: handle.value,
                access: handle.access,
                type_name: handle.type_name.clone(),
                name: handle_name(handle, table, &sid),
            })
            .collect(),
    )
}

fn modules(
    pid: u32,
    options: &Options,
) -> Option<Vec<(system::Module, Option<system::FileVersion>)>> {
    // Not opened: FAR's header still, over no modules.
    let modules = process_modules(pid).unwrap_or_default();
    Some(
        (modules.into_iter())
            .map(|module| {
                let version = (options.export_module_version && !module.path.is_empty())
                    .then(|| version_of(&module.path))
                    .flatten();
                (module, version)
            })
            .collect(),
    )
}

fn window(pid: u32) -> Option<WindowFacts> {
    let window = top_windows().get(&pid).copied()?;
    let (style, extended) = window.styles();
    Some(WindowFacts {
        title: window.title(),
        handle: window.0,
        style,
        extended,
    })
}

/// The facts of `process` (of `table`) with `options`; the handles asked until `stop` says to.
pub fn gather(
    process: &Process,
    table: &Table,
    options: &Options,
    stop: &dyn Fn() -> bool,
) -> Facts {
    let pid = process.pid;
    let own = !process.is_total() && pid != 0;
    let parent_name = (table.process(process.parent)).map(|parent| parent.name.clone());
    let version = (process.path.as_deref()).and_then(version_of);
    let started = (process.created > 0)
        .then(|| {
            let now = now_ticks();
            let gone = u64::try_from(now.saturating_sub(process.created)).unwrap_or(0);
            Some((local_time(process.created)?, local_time(now)?, gone))
        })
        .flatten();
    Facts {
        process: process.clone(),
        parent_name,
        version,
        owner: own.then(|| process_details(pid)?.owner).flatten(),
        started,
        memory: own
            .then(|| process_memory(pid, options.export_environment))
            .flatten(),
        window: own.then(|| window(pid)).flatten(),
        modules: (own && options.export_module_info)
            .then(|| modules(pid, options))
            .flatten(),
        handles: (own && options.export_handles)
            .then(|| handles(pid, table, stop))
            .flatten(),
        decimal: system::decimal_separator(),
    }
}

#[cfg(test)]
#[path = "facts_tests.rs"]
mod tests;
