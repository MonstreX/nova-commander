//! NetBox's "Keep Remote Directory Up To Date" (`TWinSCPFileSystem::Synchronize`, `TSynchronizeDialog`,
//! WinSCP's `TSynchronizeController`): the dialog ("Watch for changes in the local directory ...", "... and
//! automatically reflect them on the remote directory", " Upload options ": Delete files, Existing files
//! only, Update subdirectories, Selected files only, Synchronize on start, "Use same options next time"),
//! then Start: the disk's folder watched - read again each second - and, when it changes, compared with the
//! server's (to the server, by time) and the differences sent, in the copying's window until Esc stops it.

use std::path::Path;
use std::time::Duration;

use nc_plugin_sdk::{DialogItem, Host, Value};

use crate::copy_work::{Halted, Job};
use crate::placed::{at, heading, line};
use crate::sync_collect::{Mode, Node, Settings, collect};
use crate::sync_work::{local_tree, remote_tree};

/// NetBox's `NB_SYNCHRONIZE_TITLE`.
pub const TITLE: &str = "Keep remote directory up to date";

/// How often the disk's folder is read again.
const PERIOD: Duration = Duration::from_secs(1);
/// How often a stop is looked for meanwhile.
const STEP: Duration = Duration::from_millis(100);
/// Failed reads of the disk's folder in a row passed by before the user is asked.
const FAILED_ROUNDS: u32 = 5;

/// What watching is told to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    pub local: String,
    pub remote: String,
    pub delete: bool,
    pub existing_only: bool,
    pub recursive: bool,
    pub selected_only: bool,
    pub on_start: bool,
    pub save: bool,
}

// The items, in their order.
const LOCAL: usize = 1;
const REMOTE: usize = 3;
const DELETE: usize = 5;
const EXISTING: usize = 6;
const RECURSIVE: usize = 7;
const SELECTED: usize = 8;
const ON_START: usize = 9;
const SAVE: usize = 10;
pub const START: usize = 12;

/// WinSCP's `SynchronizeParams` flags this dialog keeps (`spDelete`, `spExistingOnly`, `spSelectedOnly`)
/// and its `SynchronizeOptions` (`soRecurse` 0x01, `soSynchronize` 0x02).
const DELETE_FLAG: i64 = crate::sync_dialog::DELETE;
const EXISTING_FLAG: i64 = crate::sync_dialog::EXISTING_ONLY;
const SELECTED_FLAG: i64 = crate::sync_dialog::SELECTED_ONLY;
const RECURSE_OPTION: i64 = 0x01;
const SYNCHRONIZE_OPTION: i64 = 0x02;
const OPTIONS_KEY: &str = "SynchronizeOptions";

/// The dialog's items for the folders, with the kept params and options.
pub fn items(
    (local, remote): (&str, &str),
    (params, options): (i64, i64),
    selection: bool,
) -> Vec<DialogItem> {
    let set = |bit: i64| params & bit != 0;
    vec![
        at(
            DialogItem::text("&Watch for changes in the local directory ..."),
            5,
            2,
        ),
        line(
            DialogItem::edit(local).history("WinscpSynchronizeLocal", false),
            5,
            3,
            66,
        ),
        at(
            DialogItem::text("... &and automatically reflect them on the remote directory"),
            5,
            4,
        ),
        line(
            DialogItem::edit(remote).history("WinscpSynchronizeRemote", false),
            5,
            5,
            66,
        ),
        heading("Upload options", 4, 6, 68),
        at(DialogItem::check("&Delete files", set(DELETE_FLAG)), 5, 7),
        at(
            DialogItem::check("&Existing files only", set(EXISTING_FLAG)),
            37,
            7,
        ),
        at(
            DialogItem::check("Update s&ubdirectories", options & RECURSE_OPTION != 0),
            5,
            8,
        ),
        at(
            DialogItem::check("Selected files o&nly", set(SELECTED_FLAG) && selection)
                .disabled(!selection),
            37,
            8,
        ),
        at(
            DialogItem::check("Synchronize on s&tart", options & SYNCHRONIZE_OPTION != 0),
            5,
            9,
        ),
        at(
            DialogItem::check("Use same &options next time", false),
            5,
            10,
        ),
        DialogItem::separator().at(-1, 11, -1, 11),
        at(DialogItem::button("&Start").default_button(), 27, 12),
        at(DialogItem::button("Cancel"), 37, 12),
    ]
}

/// What the dialog's `values` give.
pub fn watch_of(values: &[Value]) -> Watch {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let checked = |item: usize| value(item).checked();
    Watch {
        local: value(LOCAL).text.trim().to_string(),
        remote: value(REMOTE).text.trim().to_string(),
        delete: checked(DELETE),
        existing_only: checked(EXISTING),
        recursive: checked(RECURSIVE),
        selected_only: checked(SELECTED),
        on_start: checked(ON_START),
        save: checked(SAVE),
    }
}

/// The kept options (WinSCP's `SynchronizeOptions`; default: subdirectories updated).
pub fn kept_options(host: &Host) -> i64 {
    host.int_setting(OPTIONS_KEY, RECURSE_OPTION)
}

/// `watch` kept as WinSCP keeps it: its flags into the params (the others left), its options.
pub fn keep(host: &Host, watch: &Watch, params: i64) {
    let flag = |on: bool, bit: i64| if on { bit } else { 0 };
    let params = (params & !(DELETE_FLAG | EXISTING_FLAG | SELECTED_FLAG))
        | flag(watch.delete, DELETE_FLAG)
        | flag(watch.existing_only, EXISTING_FLAG)
        | flag(watch.selected_only, SELECTED_FLAG);
    let options = flag(watch.recursive, RECURSE_OPTION) | flag(watch.on_start, SYNCHRONIZE_OPTION);
    host.set_setting("SynchronizeParams", &params.to_string());
    host.set_setting(OPTIONS_KEY, &options.to_string());
}

/// The comparison's settings of watching: to the server, by time.
pub fn settings_of(watch: &Watch) -> Settings {
    Settings {
        mode: Mode::Remote,
        delete: watch.delete,
        existing_only: watch.existing_only,
        ..Settings::default()
    }
}

/// A tree kept to its first level unless subdirectories are updated, and to the names selected.
pub fn trimmed(mut nodes: Vec<Node>, watch: &Watch, names: &[String]) -> Vec<Node> {
    if !names.is_empty() {
        nodes.retain(|node| {
            names
                .iter()
                .any(|name| crate::sync_collect::same_name(name, &node.name))
        });
    }
    if !watch.recursive {
        for node in &mut nodes {
            node.children.clear();
        }
        nodes.retain(|node| !node.is_dir);
    }
    nodes
}

impl Job<'_> {
    /// The server's folder brought to the disk's once: compared, the checked differences sent.
    async fn reflect(
        &self,
        watch: &Watch,
        local: Vec<Node>,
        names: &[String],
    ) -> Result<(), Halted> {
        let stop = || self.progress.stopped();
        let mut answers = crate::copy_work::Answers::default();
        // WinSCP's `LIST_DIR_ERROR`.
        let headline = format!("Error listing directory '{}'.", watch.remote);
        let listing = async || {
            (remote_tree(self.fs, &watch.remote, &stop).await).map_err(crate::copy_work::fault)
        };
        let Some(server) = self
            .retrying_about(&mut answers, &headline, listing)
            .await?
        else {
            return Ok(());
        };
        self.halt_if_stopped()?;
        let server = trimmed(server, watch, names);
        let items = collect(&local, &server, &settings_of(watch), "");
        self.synchronize(&items, (Path::new(&watch.local), &watch.remote), false)
            .await
    }

    /// Watching until stopped (Esc): each change of the disk's folder reflected on the server.
    /// A failed read of the disk's folder is never compared (it would look empty and, with "Delete files",
    /// empty the server's): the round is passed by; after [`FAILED_ROUNDS`] in a row WinSCP's
    /// `LIST_DIR_ERROR` is asked (Retry reads again, Skip goes on watching, Abort stops).
    pub async fn keep_up_to_date(&self, watch: &Watch, names: &[String]) -> Result<(), Halted> {
        let mut answers = crate::copy_work::Answers::default();
        let headline = format!("Error listing directory '{}'.", watch.local);
        let mut failed = 0;
        let mut seen = None;
        let mut on_start = watch.on_start;
        loop {
            match self.local_now(watch, names) {
                Ok(now) => {
                    failed = 0;
                    let changed = seen.as_ref().is_some_and(|seen| *seen != now);
                    if changed || std::mem::take(&mut on_start) {
                        self.reflect(watch, now.clone(), names).await?;
                    }
                    seen = Some(now);
                }
                Err(_) if failed + 1 < FAILED_ROUNDS => failed += 1,
                Err(_) => {
                    failed = 0;
                    let read = async || self.local_now(watch, names);
                    if let Some(now) = self.retrying_about(&mut answers, &headline, read).await? {
                        seen = Some(now);
                    }
                }
            }
            if self.waited_out().await {
                return Ok(());
            }
        }
    }

    /// The disk's folder as watched.
    fn local_now(
        &self,
        watch: &Watch,
        names: &[String],
    ) -> Result<Vec<Node>, crate::copy_work::Fault> {
        local_tree(Path::new(&watch.local))
            .map(|tree| trimmed(tree, watch, names))
            .map_err(crate::copy_work::fault)
    }

    /// One period waited; true when stopped meanwhile.
    async fn waited_out(&self) -> bool {
        let mut waited = Duration::ZERO;
        while waited < PERIOD {
            if self.progress.stopped() {
                return true;
            }
            tokio::time::sleep(STEP).await;
            waited += STEP;
        }
        false
    }
}

#[cfg(test)]
#[path = "keep_up_to_date_tests.rs"]
mod tests;
