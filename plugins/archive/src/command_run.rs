//! `arc:`'s commands run as arclite's `cmdline_*` run them, no dialog of theirs: `c` / `u` (a new
//! archive made, or the files added to one with the archive's own method and encryption; arclite's
//! `cmdline_update` shows no log), `x` (its `extract(arc_list)`), `e` (the items matched by their paths
//! or masks extracted, each into the folder by its name; `extract_items`), `d` (deleted;
//! `delete_items`), `t` (`bulk_test`). Names given without a folder are the program's folder's (FAR's
//! current one).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use nc_plugin_sdk::{CodePage, Error, ErrorKind, Host, Which};

use crate::asking::HostAsks;
use crate::command_line::{BadFormat, Command, ItemsCommand, UpdateCommand, list_file};
use crate::extract::{Abort, extract_noting};
use crate::opened::Opened;
use crate::opening::full_path;
use crate::{ArchivePlugin, NOT_ARCHIVE, bulk, deleting, extraction};

fn stopped() -> Error {
    Error::new(ErrorKind::Cancelled, "Stopped")
}

fn not_archive(path: &Path) -> Error {
    Error::new(
        ErrorKind::Other,
        format!("{NOT_ARCHIVE}\n{}", path.display()),
    )
}

/// Why a command asks for what the zip library does not write.
pub(crate) fn not_written(what: &str) -> Error {
    Error::new(ErrorKind::Unsupported, what.to_string())
}

/// A list file's text (arclite's `load_file`): UTF-16 or UTF-8 by their marks, else as `ansi` reads it
/// (the ANSI code page).
fn list_text(bytes: &[u8], ansi: &dyn Fn(&[u8]) -> String) -> String {
    if let Some(wide) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = (wide.as_chunks::<2>().0.iter())
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(utf8) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(utf8).into_owned();
    }
    ansi(bytes)
}

/// Each node of the tree under `folder` with its path from the root (`\` between the parts) and its
/// folder's node, in the tree's order.
fn every_node(opened: &Opened, folder: usize, way: &str, out: &mut Vec<(usize, String, usize)>) {
    for &node in &opened.tree.nodes[folder].children {
        let name = &opened.tree.nodes[node].name;
        let path = if way.is_empty() {
            name.clone()
        } else {
            format!("{way}\\{name}")
        };
        out.push((node, path.clone(), folder));
        if opened.tree.nodes[node].is_dir {
            every_node(opened, node, &path, out);
        }
    }
}

/// Whether `item` has the signs of FAR's masks (`*`, `?`, `[`, `,`, `;`, `|`): only such a name is
/// matched as masks, every other compared as a path (the same answer, without a call to the program
/// for each of an archive's entries).
fn has_masks(item: &str) -> bool {
    item.contains(['*', '?', '[', ',', ';', '|'])
}

/// arclite's matching of `e` / `d`: an item whose path is one of `items` whatever its case, or that one
/// of them takes as masks; with its folder's node.
fn matched(host: &Host, opened: &Opened, items: &[String]) -> Vec<(usize, usize)> {
    let mut all = Vec::new();
    every_node(opened, 0, "", &mut all);
    let (masks, names): (Vec<&String>, Vec<&String>) =
        items.iter().partition(|item| has_masks(item));
    let names: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    (all.into_iter())
        .filter(|(_, path, _)| {
            names.contains(&path.to_lowercase())
                || masks
                    .iter()
                    .any(|mask| host.match_masks(mask, path) == Some(true))
        })
        .map(|(node, _, parent)| (node, parent))
        .collect()
}

impl ArchivePlugin {
    /// `arc:` with a command other than opening, run; its text not understood: arclite's syntax shown
    /// (in place of its help topic "Prefix").
    pub(crate) fn run_line(
        &mut self,
        command: Command,
        folder: Option<&Path>,
    ) -> Result<(), Error> {
        let whole = |name: &str| full_path(name, folder);
        let load = |path: &Path| self.load(path);
        let host = self.host;
        let done = match command {
            // Opening is `open_command`'s own: never handed here.
            Command::Open(_) => Ok(()),
            Command::Test(names) => {
                let archives: Vec<PathBuf> = names.iter().map(|name| whole(name)).collect();
                bulk::test_all(host, &archives, &load)
            }
            Command::Extract(command) => {
                let archives: Vec<PathBuf> =
                    (command.archives.iter()).map(|name| whole(name)).collect();
                let mut options = command.options;
                options.dst_dir = whole(&options.dst_dir.to_string_lossy());
                bulk::extract_each(host, &archives, options, &load)
            }
            Command::Items(command) if command.delete => self.delete_items(&command, folder),
            Command::Items(command) => self.extract_items(&command, folder),
            Command::Update(command) => self.update_line(&command, folder),
        };
        match done {
            Err(error) if error.kind == ErrorKind::Cancelled => Ok(()),
            other => other,
        }
    }

    /// The archive `name` read for a command, its password given.
    pub(crate) fn command_archive(&self, path: &Path, password: &str) -> Result<Opened, Error> {
        let mut opened = self.load(path)?.ok_or_else(|| not_archive(path))?;
        if !password.is_empty() {
            opened.password = Some(password.to_string());
        }
        Ok(opened)
    }

    /// arclite's `extract_items`: the items matched extracted, a folder's group at a time, each into the
    /// folder (`-out`, else the program's) by its name; the log, or the panel read again.
    fn extract_items(
        &mut self,
        command: &ItemsCommand,
        folder: Option<&Path>,
    ) -> Result<(), Error> {
        let archive = full_path(&command.archive, folder);
        let mut opened = self.command_archive(&archive, &command.options.password)?;
        let found = matched(&self.host, &opened, &command.items);
        if found.is_empty() {
            return Ok(());
        }
        let mut options = command.options.clone();
        let given = options.dst_dir.to_string_lossy().into_owned();
        options.dst_dir = match (given.is_empty(), folder) {
            (true, Some(folder)) => folder.to_path_buf(),
            _ => extraction::whole(PathBuf::from(given), folder),
        };
        let mut parents: Vec<usize> = found.iter().map(|(_, parent)| *parent).collect();
        parents.sort_unstable();
        parents.dedup();
        let mut asks = HostAsks::new(self.host, &archive);
        let mut log = Vec::new();
        for parent in parents {
            let nodes: Vec<usize> = (found.iter())
                .filter(|(_, of)| *of == parent)
                .map(|(node, _)| *node)
                .collect();
            let (faults, _) = extract_noting(&mut opened, &nodes, &options, &mut asks)
                .map_err(|Abort| stopped())?;
            log.extend(faults);
        }
        if log.is_empty() {
            self.host.reread(Which::Active, false);
        } else {
            asks.show_log(&log);
        }
        Ok(())
    }

    /// arclite's `delete_items`: the items matched (and all under them) deleted, no question; the
    /// archive written anew and put in its place as an update's.
    fn delete_items(&mut self, command: &ItemsCommand, folder: Option<&Path>) -> Result<(), Error> {
        let archive = full_path(&command.archive, folder);
        let opened = self.command_archive(&archive, &command.options.password)?;
        let nodes: Vec<usize> = (matched(&self.host, &opened, &command.items).into_iter())
            .map(|(node, _)| node)
            .collect();
        if nodes.is_empty() {
            return Ok(());
        }
        let written = deleting::delete_extracted(self.host, &opened, &nodes, &BTreeSet::new())?;
        drop(opened);
        match written {
            Some(temp) => self.swap_in(&archive, &temp),
            None => Ok(()),
        }
    }

    /// arclite's `cmdline_update`: the files (and those the list files name) made whole, their shared
    /// folder taken off; a new archive made, or the files added to the one there.
    fn update_line(&mut self, command: &UpdateCommand, folder: Option<&Path>) -> Result<(), Error> {
        let mut files: Vec<PathBuf> = (command.files.iter())
            .map(|file| full_path(file, folder))
            .collect();
        for list in &command.listfiles {
            let path = full_path(list, folder);
            let bytes = std::fs::read(&path).map_err(|error| crate::io_error(&error))?;
            let host = self.host;
            let ansi = move |bytes: &[u8]| {
                (host.decode(CodePage::Ansi, bytes))
                    .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
            };
            let text = list_text(&bytes, &ansi);
            files.extend(list_file(&text).iter().map(|file| full_path(file, folder)));
        }
        if files.is_empty() {
            return Err(stopped());
        }
        let archive = full_path(&command.archive, folder);
        if command.create {
            self.create_line(command, &archive, &files)
        } else {
            self.add_line(command, &archive, &files)
        }
    }
}

/// arclite's `E_BAD_FORMAT` where it is found late (a profile not there): its syntax shown, nothing done.
pub(crate) fn bad_format(host: &Host) -> Error {
    show_usage(host, BadFormat);
    stopped()
}

/// arclite's help topic "Prefix" in short: the commands' syntax.
pub fn show_usage(host: &Host, _: BadFormat) {
    let message = nc_plugin_sdk::Message::new(crate::options::TITLE, &crate::command_line::USAGE)
        .buttons(&["OK"])
        .left_align();
    let _ = host.show(&message);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A list file's text: UTF-16 by its mark, UTF-8 by its mark, else the ANSI code page (arclite's
    /// `load_file`; a UTF-8 text without its mark read as ANSI too).
    #[test]
    fn a_list_file_is_read_by_its_mark() {
        let ansi = |bytes: &[u8]| format!("ansi:{}", bytes.len());
        let mut wide = vec![0xFF, 0xFE];
        wide.extend("я.txt".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(list_text(&wide, &ansi), "я.txt");
        let mut utf8 = vec![0xEF, 0xBB, 0xBF];
        utf8.extend("я.txt".as_bytes());
        assert_eq!(list_text(&utf8, &ansi), "я.txt");
        assert_eq!(
            list_text("я.txt".as_bytes(), &ansi),
            "ansi:6",
            "no mark: ANSI"
        );
    }

    /// Only a name with masks' signs is matched as masks.
    #[test]
    fn masks_are_told_by_their_signs() {
        assert!(has_masks("*.txt") && has_masks("a?b") && has_masks("a,b") && has_masks("[ab]"));
        assert!(!has_masks(r"docs\a.txt"));
    }
}
