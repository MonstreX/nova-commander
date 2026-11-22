//! "Synchronize" at work (NetBox's `FullSynchronize` over WinSCP's `SynchronizeCollect`, the checklist of
//! `TSynchronizeChecklistDialog` and `SynchronizeApply`): both folders' trees read (the disk's, the
//! server's with links taken as they are), the checklist shown as a menu of checked lines - Space or Ins
//! toggles a line, "+" checks all, "-" unchecks all, Enter goes on - and the checked items done through
//! the copying's work: uploads, downloads, deletions on either side, times set for "Synchronize
//! timestamps" (no questions: WinSCP's `spNoConfirmation`).

use std::path::Path;

use nc_plugin_sdk::{Menu, MenuItem};

use crate::copy_questions::{format_size, modification};
use crate::copy_work::{Halted, Job, fault, system_time, unix_seconds};
use crate::fs::Fs;
use crate::sftp::child;
use crate::sync_collect::{Action, Item, Node};

/// The disk's folder's tree.
pub fn local_tree(folder: &Path) -> std::io::Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let children = match meta.is_dir() {
            true => local_tree(&entry.path())?,
            false => Vec::new(),
        };
        nodes.push(Node {
            name,
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta.modified().ok().and_then(unix_seconds),
            precision: crate::sftp::Precision::Second,
            children,
        });
    }
    Ok(nodes)
}

/// The server's folder's tree (a link as it is: a link to a folder is not entered), stopped by `stop`.
pub async fn remote_tree(
    fs: &Fs,
    folder: &str,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Node>, String> {
    let mut pending = vec![folder.to_string()];
    let mut done: Vec<(String, Vec<Node>)> = Vec::new();
    while let Some(path) = pending.pop() {
        if stop() {
            return Err("Cancelled".to_string());
        }
        let mut nodes = Vec::new();
        for entry in fs.list_raw(&path).await? {
            let is_dir = entry.is_dir && !entry.is_link;
            if is_dir {
                pending.push(child(&path, &entry.name));
            }
            nodes.push(Node {
                name: entry.name,
                is_dir,
                size: if is_dir { 0 } else { entry.size },
                modified: entry.modified.map(u64::from),
                precision: entry.precision,
                children: Vec::new(),
            });
        }
        done.push((path, nodes));
    }
    Ok(assembled(folder, &mut done))
}

/// The folders' listings put together into the tree of `folder`.
fn assembled(folder: &str, listed: &mut Vec<(String, Vec<Node>)>) -> Vec<Node> {
    let Some(at) = listed.iter().position(|(path, _)| path == folder) else {
        return Vec::new();
    };
    let (_, mut nodes) = listed.swap_remove(at);
    for node in nodes.iter_mut().filter(|node| node.is_dir) {
        node.children = assembled(&child(folder, &node.name), listed);
    }
    nodes
}

/// The arrow of an action (NetBox's `NB_CHECKLIST_ACTIONS` with the direction drawn).
fn arrow(action: Action) -> &'static str {
    match action {
        Action::UploadNew => "*->",
        Action::DownloadNew => "<-*",
        Action::UploadUpdate => " ->",
        Action::DownloadUpdate => "<- ",
        Action::DeleteRemote => "  X",
        Action::DeleteLocal => "X  ",
    }
}

fn side(found: Option<(u64, Option<u64>)>, is_dir: bool) -> String {
    match found {
        Some((size, time)) if !is_dir => {
            format!("{:>9} {}", format_size(size), modification(time))
        }
        Some((_, time)) => format!("{:>9} {}", "", modification(time)),
        None => String::new(),
    }
}

/// A line of the checklist: the path, the local side, the arrow, the remote side.
pub fn line(item: &Item) -> String {
    let slash = if item.is_dir { "/" } else { "" };
    format!(
        "{:<20} {:<29} {} {}",
        format!("{}{slash}", item.path),
        side(item.local, item.is_dir),
        arrow(item.action),
        side(item.remote, item.is_dir)
    )
}

/// NetBox's `NB_CHECKLIST_TITLE` and `NB_CHECKLIST_CHECKED`; the menu's keys.
pub const CHECKLIST_TITLE: &str = "Synchronization checklist";
pub const CHECKLIST_KEYS: [&str; 4] = ["Space", "Ins", "+", "-"];

/// The checklist's menu, the cursor on `focus`.
pub fn checklist_menu(items: &[Item], focus: usize) -> Menu {
    let lines: Vec<MenuItem> = (items.iter())
        .map(|item| MenuItem::new(&line(item)).checked(item.checked))
        .collect();
    let checked = items.iter().filter(|item| item.checked).count();
    Menu::new(CHECKLIST_TITLE, lines)
        .bottom(&format!(" {checked} of {} ", items.len()))
        .selected(focus)
        .break_keys(&CHECKLIST_KEYS)
        .show_ampersand()
        .wrap()
}

/// What a break key (its number) on line `at` does to the checklist.
pub fn toggled(items: &mut [Item], at: Option<usize>, key: usize) {
    match key {
        0 | 1 => {
            if let Some(item) = at.and_then(|at| items.get_mut(at)) {
                item.checked = !item.checked;
            }
        }
        2 | 3 => items.iter_mut().for_each(|item| item.checked = key == 2),
        _ => {}
    }
}

/// The parent folder and the name of a checklist path.
fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

impl Job<'_> {
    /// WinSCP's `SynchronizeApply`: the checked items done, `local` and `remote` the folders compared;
    /// `timestamps`: an update sets the time only.
    pub async fn synchronize(
        &self,
        items: &[Item],
        (local, remote): (&Path, &str),
        timestamps: bool,
    ) -> Result<(), Halted> {
        for item in items.iter().filter(|item| item.checked) {
            self.halt_if_stopped()?;
            let (folder, name) = split(&item.path);
            let local_folder = (folder.split('/'))
                .filter(|part| !part.is_empty())
                .fold(local.to_path_buf(), |path, part| path.join(part));
            let remote_folder = match folder.is_empty() {
                true => remote.to_string(),
                false => child(remote, folder),
            };
            let names = [name.to_string()];
            match item.action {
                Action::UploadUpdate | Action::DownloadUpdate if timestamps => {
                    self.set_time(item, &local_folder.join(name), &child(&remote_folder, name))
                        .await?;
                }
                Action::UploadNew | Action::UploadUpdate => {
                    self.upload(&[local_folder.join(name)], &remote_folder)
                        .await?;
                }
                Action::DownloadNew | Action::DownloadUpdate => {
                    self.download(&remote_folder, &names, &local_folder).await?;
                }
                Action::DeleteRemote => self.delete(&remote_folder, &names).await?,
                Action::DeleteLocal => self.delete_local(&local_folder.join(name)).await?,
            }
        }
        Ok(())
    }

    /// "Synchronize timestamps": the target's time set to the source's.
    async fn set_time(&self, item: &Item, local: &Path, remote: &str) -> Result<(), Halted> {
        let mut answers = crate::copy_work::Answers::default();
        let headline = format!("Error changing timestamp of '{}'.", item.path);
        let set = async || match item.action {
            Action::UploadUpdate => {
                let seconds = item.local.and_then(|(_, time)| time).unwrap_or(0);
                let attributes = russh_sftp::protocol::FileAttributes {
                    mtime: u32::try_from(seconds).ok(),
                    atime: u32::try_from(seconds).ok(),
                    ..russh_sftp::protocol::FileAttributes::empty()
                };
                self.fs
                    .set_metadata(remote, attributes)
                    .await
                    .map_err(fault)
            }
            _ => {
                let seconds = item.remote.and_then(|(_, time)| time).unwrap_or(0);
                let file = std::fs::File::options()
                    .write(true)
                    .open(local)
                    .map_err(fault)?;
                file.set_modified(system_time(seconds)).map_err(fault)
            }
        };
        self.retrying_about(&mut answers, &headline, set)
            .await
            .map(drop)
    }

    /// A disk's file or folder removed (WinSCP's local deletion of `saDeleteLocal`).
    async fn delete_local(&self, path: &Path) -> Result<(), Halted> {
        let mut answers = crate::copy_work::Answers::default();
        let headline = format!("Error deleting file '{}'.", path.display());
        let gone = async || match path.is_dir() {
            true => std::fs::remove_dir_all(path).map_err(fault),
            false => std::fs::remove_file(path).map_err(fault),
        };
        self.retrying_about(&mut answers, &headline, gone)
            .await
            .map(drop)
    }
}

#[cfg(test)]
#[path = "sync_work_tests.rs"]
mod tests;
