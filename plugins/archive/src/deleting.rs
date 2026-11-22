//! Deleting from an archive's panel and making a folder in it as arclite does (`delete_files` with
//! `Archive::delete_files`, `create_dir` with `Archive::create_dir`; FAR's `DeleteFilesW` and
//! `MakeDirectoryW`): its question "Do you want to delete selected files?" (none when silent), the items of
//! the entries and of all under them left out of the archive written anew; its "Enter directory name:"
//! for F7, the folder written as a new item with the time now. The new archive is put in place by the
//! caller (`lib`), as an update's.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::SystemTime;

use nc_plugin_sdk::{ButtonSet, Error, ErrorKind, GetMode, Host, Message};

use crate::asking::{HostAsks, fault_lines};
use crate::extract_options::Overwrite;
use crate::extraction::child;
use crate::opened::Opened;
use crate::options::TITLE;
use crate::update::{Added, Faults, Plan};
use crate::update_options::{UpdateOptions, UpdateSettings};
use crate::update_write::{Stop, write};

/// Windows' `FILE_ATTRIBUTE_DIRECTORY`.
const DIRECTORY: u32 = 0x10;

fn stopped() -> Error {
    Error::new(ErrorKind::Cancelled, "Stopped")
}

/// A file and a folder of one name in the folder `folder` (the zip's "x" and "x/"): the program names
/// its entries only by their names (arclite takes the item itself), so deleting either is refused - the
/// other might go in its place.
pub fn refuse_twins(opened: &Opened, folder: usize, names: &[String]) -> Result<(), Error> {
    let children = &opened.tree.nodes[folder].children;
    let twin = (names.iter()).find(|name| {
        (children.iter())
            .filter(|&&node| opened.tree.nodes[node].name == **name)
            .count()
            > 1
    });
    twin.map_or(Ok(()), |name| {
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "The archive has a file and a folder both named \"{name}\": which one to delete \
                 cannot be told"
            ),
        ))
    })
}

/// The entries `names` of the folder `path` by their nodes: by the name as written, else whatever its
/// case; ".." and names not there none, a name of a file and a folder both refused.
fn nodes_of(opened: &Opened, path: &str, names: &[String]) -> Result<Vec<usize>, Error> {
    let Some(folder) = opened.tree.find_dir(path) else {
        return Ok(Vec::new());
    };
    refuse_twins(opened, folder, names)?;
    Ok((names.iter())
        .filter(|name| name.as_str() != "..")
        .filter_map(|name| child(opened, folder, name))
        .collect())
}

/// The archive's items of `nodes` and of all under them (arclite's `enum_deleted_indices`).
pub fn items_under(opened: &Opened, nodes: &[usize]) -> BTreeSet<usize> {
    let mut items = BTreeSet::new();
    let mut pending: Vec<usize> = nodes.to_vec();
    while let Some(node) = pending.pop() {
        let node = &opened.tree.nodes[node];
        items.extend(node.item);
        pending.extend(&node.children);
    }
    items
}

/// The entries of `nodes` extracted whole: none of the items under them `skipped` (arclite's
/// `extracted_indices`, the selected ones less a skipped item and every folder above it).
pub fn extracted(opened: &Opened, nodes: &[usize], skipped: &BTreeSet<usize>) -> Vec<usize> {
    (nodes.iter().copied())
        .filter(|&node| items_under(opened, &[node]).is_disjoint(skipped))
        .collect()
}

/// The new archive written with `plan` beside the old one, through the update's windows.
fn written(host: Host, opened: &Opened, plan: &Plan, deleting: bool) -> Result<PathBuf, Error> {
    let settings = UpdateSettings::load(&host);
    // The items kept are copied as they are; a new folder's item holds no data, so the settings' level
    // and Deflate without encryption stand for arclite's `m_*` properties.
    let mut options = UpdateOptions::for_archive(&settings, ("", false), false, true);
    options.overwrite = Overwrite::Overwrite;
    options.password = opened.password.clone().unwrap_or_default();
    let asks = HostAsks::new(host, &opened.path);
    let mut asks = if deleting { asks.deleting() } else { asks };
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: false,
        log: Vec::new(),
    };
    let written = if opened.library().is_some() {
        crate::update_library::write_plain(opened, plan, &options, &mut faults)
    } else {
        write(opened, plan, &options, &mut faults)
    };
    written.map_err(|stop| match stop {
        Stop::Abort => stopped(),
        Stop::Failed(fault) => Error::new(ErrorKind::Other, fault_lines(&fault).join("\n")),
    })
}

/// arclite's `delete_files`: its question unless silent, then the archive written anew without the
/// entries `names` of the folder `path` (and what is under them); none when nothing is to go.
pub fn delete(
    host: Host,
    opened: &Opened,
    (path, names): (&str, &[String]),
    mode: GetMode,
) -> Result<Option<PathBuf>, Error> {
    let nodes = nodes_of(opened, path, names)?;
    if nodes.is_empty() {
        return Ok(None);
    }
    crate::updating::deletable(opened)?;
    if mode.with_dialog() {
        let question = Message::new(TITLE, &["Do you want to delete selected files?"])
            .button_set(ButtonSet::OkCancel);
        if host.show(&question) != Some(0) {
            return Err(stopped());
        }
    }
    let plan = Plan {
        removed: items_under(opened, &nodes),
        ..Plan::default()
    };
    if plan.removed.is_empty() {
        return Ok(None);
    }
    written(host, opened, &plan, true).map(Some)
}

/// The entries extracted by F6 left out of the archive (arclite's `delete_files(extracted_indices)`):
/// each with all under it, an entry with a file the user skipped kept whole.
pub fn delete_extracted(
    host: Host,
    opened: &Opened,
    nodes: &[usize],
    skipped: &BTreeSet<usize>,
) -> Result<Option<PathBuf>, Error> {
    crate::updating::deletable(opened)?;
    let plan = Plan {
        removed: items_under(opened, &extracted(opened, nodes, skipped)),
        ..Plan::default()
    };
    if plan.removed.is_empty() {
        return Ok(None);
    }
    written(host, opened, &plan, true).map(Some)
}

/// arclite's `create_dir`: the name `name` (empty for F7: its "Enter directory name:" asked unless
/// silent), a folder of that name written in the folder `path`: the new archive and the name.
pub fn make_folder(
    host: Host,
    opened: &Opened,
    (path, name): (&str, &str),
    mode: GetMode,
) -> Result<(PathBuf, String), Error> {
    crate::updating::changeable(opened)?;
    let name = if mode.with_dialog() {
        host.input(TITLE, "Enter directory name:", name)
            .ok_or_else(stopped)?
    } else {
        name.to_string()
    };
    let name = name.trim_matches(['\\', '/']).to_string();
    if name.is_empty() {
        return Err(stopped());
    }
    let parts = new_folder(opened, path, &name)?;
    let now = Some(SystemTime::now());
    let added = Added {
        path: parts,
        source: PathBuf::new(),
        is_dir: true,
        size: 0,
        modified: now,
        created: now,
        accessed: now,
        attributes: DIRECTORY,
    };
    let plan = Plan {
        added: vec![added],
        ..Plan::default()
    };
    let temp = written(host, opened, &plan, false)?;
    Ok((temp, name))
}

/// The new folder's path in the archive: the panel's folder `path` and the name typed, `\` or `/`
/// between its parts; refused with "." or ".." in it (an item no tree shows, a path out of the
/// archive's) and when an entry of that path is there already.
fn new_folder(opened: &Opened, path: &str, name: &str) -> Result<Vec<String>, Error> {
    let own: Vec<&str> = (name.split(['\\', '/']))
        .filter(|part| !part.is_empty())
        .collect();
    if own.iter().any(|part| matches!(*part, "." | "..")) {
        return Err(Error::new(
            ErrorKind::Other,
            format!("\"{name}\" cannot be a folder's name: no \".\" or \"..\" in it"),
        ));
    }
    let parts: Vec<String> = (path.split('/').filter(|part| !part.is_empty()))
        .chain(own)
        .map(str::to_string)
        .collect();
    if already_there(opened, &parts) {
        return Err(Error::new(
            ErrorKind::Exists,
            format!("\"{name}\" is already in the archive"),
        ));
    }
    Ok(parts)
}

/// An entry (a file or a folder) of the path `parts` in the archive already, whatever its case.
fn already_there(opened: &Opened, parts: &[String]) -> bool {
    let Some((last, parent)) = parts.split_last() else {
        return false;
    };
    let parent = format!("/{}", parent.join("/"));
    (opened.tree.find_dir(&parent)).is_some_and(|folder| child(opened, folder, last).is_some())
}

#[cfg(test)]
#[path = "deleting_tests.rs"]
mod tests;
