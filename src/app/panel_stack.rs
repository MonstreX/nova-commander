//! The plugins' panels of a panel (FAR's `PluginsList`, `PushPlugin` / `PopPlugin`, ADR-0016 §15): each
//! remembers where the panel was and its view and sort. A plugin's panel opened from the drive menu, F11,
//! the history or a path typed closes those the panel shows and comes in their place with what the panel
//! had before the first (FAR's `ChangePanel` with `CreateNew`: `dispose`, `GetPrevViewMode`...); ".." at
//! its root goes back to that folder as it was, as does a disk's folder entered (`SetCurDir`). A file's
//! plugin's panel (FAR's `OpenFilePlugin` with `PushPrev`: TmpPanel's list file) goes over the panel, and
//! ".." at its root comes back with the cursor on the file and the selection as it was (`PopPrevData`);
//! TmpPanel's switch of its common panels shows another in the same place.

use super::{Panel, ViewMode};
use crate::vfs::{PanelLook, SortOrder, VfsPath};

/// Where a panel was as it went into a plugin's panel, and how it looked (FAR's `PluginsListItem`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entered {
    pub from: VfsPath,
    pub view: ViewMode,
    pub sort: SortOrder,
    /// The file a file's plugin was opened from (FAR's `PrevDataList`); none for another plugin's panel.
    pub host: Option<HostFile>,
}

/// The file a plugin's panel was opened from and the selection beside it (FAR's `PrevDataItem`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFile {
    pub name: String,
    pub selected: Vec<String>,
    /// The program's folder holding the copy the panel was opened from, when the file was another
    /// plugin's (an archive inside an archive): it goes as the panel closes (FAR's `delayed_delete`).
    pub temp: Option<std::path::PathBuf>,
}

/// How long a copy's folder is tried for while its plugin may still hold it, and how often.
const COPY_TRIES: (u32, std::time::Duration) = (100, std::time::Duration::from_millis(100));

/// The copies of the file's plugins' panels in `entered` deleted, once their plugins have let them go.
fn forget_copies<'a>(entered: impl IntoIterator<Item = &'a Entered>) {
    for folder in (entered.into_iter()).filter_map(|entered| entered.host.as_ref()?.temp.clone()) {
        forget_copy(folder);
    }
}

/// A folder of the program's holding a copy deleted once whoever has the copy open lets it go (FAR's
/// `delayed_delete`): tried for a few seconds on a thread of its own, as a plugin closes its panel or
/// answers after this.
pub(in crate::app) fn forget_copy(folder: std::path::PathBuf) {
    let thread = std::thread::Builder::new().name("copy removal".to_string());
    // No thread to be had (R7 review B9): tried once here instead.
    let tried = folder.clone();
    if thread
        .spawn(move || remove_copy(&folder, COPY_TRIES.0))
        .is_err()
    {
        remove_copy(&tried, 1);
    }
}

/// `folder` deleted, tried `tries` times a moment apart; whether it is gone.
fn remove_copy(folder: &std::path::Path, tries: u32) -> bool {
    for _ in 0..tries {
        if std::fs::remove_dir_all(folder).is_ok() || !folder.exists() {
            return true;
        }
        std::thread::sleep(COPY_TRIES.1);
    }
    false
}

/// The program ends: the copies its plugins' panels were opened from deleted once the plugins have
/// ended (FAR's `PopPlugin` of each panel as it closes, `DeleteFileWithFolder`), each tried for a moment
/// (a plugin's thread that did not end in time may still hold its copy: it stays).
pub fn remove_copies_now(folders: &[std::path::PathBuf]) {
    for folder in folders {
        remove_copy(folder, 20);
    }
}

/// How the next plugin's panel the panel reads comes in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) enum Entering {
    /// In the place of the plugins' panels shown (FAR's `ChangePanel` with `CreateNew`).
    #[default]
    Anew,
    /// TmpPanel's switch: in the place of the one shown.
    Switch,
    /// Over what the panel shows, opened from that file (FAR's `OpenFilePlugin` with `PushPrev`), from
    /// its copy in the program's folder when it was another plugin's.
    Push(String, Option<std::path::PathBuf>),
}

impl Panel {
    /// Where ".." at a plugin's root leads: the folder or the plugin's panel under this one.
    #[cfg(all(test, windows))]
    pub fn return_to(&self) -> Option<&VfsPath> {
        self.plugin_stack.last().map(|entered| &entered.from)
    }

    /// The view the panel had before its first plugin's panel (what comes back as they all close).
    pub fn return_view(&self) -> Option<ViewMode> {
        self.plugin_stack.first().map(|entered| entered.view)
    }

    /// The folder on a disk and the sort the panel had before its first plugin's panel.
    pub fn before_plugins(&self) -> Option<&Entered> {
        self.plugin_stack.first()
    }

    /// FAR's `PFLAGS_REALNAMES` (`OPIF_REALNAMES`): always on a disk's panel; a provider's real names are
    /// on all its entries or on none, so the first one tells.
    pub fn has_real_names(&self) -> bool {
        self.path.provider().is_none()
            || (self.entries.iter().find(|entry| !entry.is_parent))
                .is_some_and(|entry| entry.real_path.is_some())
    }

    /// The folder on a disk the panel stands for (FAR's `GetCurDir`): its own, or on a provider's panel the
    /// one the first plugin's panel was opened from (where commands run, the drive menu opens, the info
    /// panel looks); `None` on a provider's panel entered otherwise.
    pub fn local_folder(&self) -> Option<&std::path::Path> {
        (self.path.as_local()).or_else(|| self.before_plugins()?.from.as_local())
    }

    /// The panel's folder as the command line and the clipboard take it, with a separator at its end
    /// (FAR's `MakePath` with `AddEndSlash`): a disk's path, or a provider's own path (FAR's plugin
    /// `CurDir`, without the plugin's prefix: `SubstPluginPrefix` is off by default).
    pub fn folder_text(&self) -> String {
        match &self.path {
            VfsPath::Local(path) => crate::app::cmdline_insert::with_end_slash(path),
            VfsPath::Mounted { path, .. } if path.ends_with('/') => path.clone(),
            VfsPath::Mounted { path, .. } => format!("{path}/"),
        }
    }

    /// The `at`-th of what is under this panel (the first entered from a disk).
    pub fn under(&self, at: usize) -> Option<&VfsPath> {
        self.plugin_stack.get(at).map(|entered| &entered.from)
    }

    /// The file the plugin's panel shown was opened from (FAR's `HostFile`), by its name.
    pub fn host_file(&self) -> Option<&str> {
        let top = self.plugin_stack.last()?;
        top.host.as_ref().map(|host| host.name.as_str())
    }

    /// The plugins' panels under this one, the first entered from a disk.
    pub(in crate::app) fn plugin_stack_mut(&mut self) -> &mut Vec<Entered> {
        &mut self.plugin_stack
    }

    /// ".." at a plugin's root: where the panel under it is, and the file it was opened from; nothing
    /// when there is none.
    pub(in crate::app) fn pop_plugin(&mut self) -> Option<(VfsPath, Option<String>)> {
        let top = self.plugin_stack.last()?;
        self.popping = true;
        let host = top.host.as_ref().map(|host| host.name.clone());
        Some((top.from.clone(), host))
    }

    /// TmpPanel's switch: the plugin's panel read next shows in the place of this one.
    pub(in crate::app) fn switch_plugin_panel(&mut self) {
        self.entering = Entering::Switch;
    }

    /// FAR's `OpenFilePlugin` with `PushPrev`: the plugin's panel read next goes over this one, opened
    /// from its file `name`.
    pub(in crate::app) fn push_file_plugin(
        &mut self,
        name: &str,
        temp: Option<std::path::PathBuf>,
    ) {
        self.entering = Entering::Push(name.to_string(), temp);
    }

    /// The selection the panel had under a file's plugin, back on the names still among `entries`.
    pub(super) fn reselect(&mut self, names: Option<Vec<String>>, entries: &[crate::vfs::Entry]) {
        let Some(names) = names else {
            return;
        };
        // A set of the names there, not a walk of the entries per name (review R33).
        let there: std::collections::HashSet<&str> =
            entries.iter().map(|entry| entry.name.as_str()).collect();
        let back = names
            .into_iter()
            .filter(|name| there.contains(name.as_str()));
        self.selected.extend(back);
    }

    /// A reading that would enter a plugin's panel or leave one failed or was stopped: the plugins'
    /// panels stay as they are, nothing waits to be pushed or popped.
    pub(super) fn plugin_move_failed(&mut self) {
        self.popping = false;
        if let Entering::Push(_, Some(folder)) = std::mem::take(&mut self.entering) {
            // The plugin's panel over a copy did not open: the copy goes.
            forget_copy(folder);
        }
    }

    /// The program's folders holding the copies the plugins' panels were opened from, and the one a
    /// panel about to open would be.
    pub(in crate::app) fn plugin_copies(&self) -> impl Iterator<Item = std::path::PathBuf> + '_ {
        let entering = match &self.entering {
            Entering::Push(_, Some(folder)) => Some(folder.clone()),
            _ => None,
        };
        (self.plugin_stack.iter())
            .filter_map(|entered| entered.host.as_ref()?.temp.clone())
            .chain(entering)
    }

    /// The panel read at `dir` with `look`: a plugin's panel entered over what the panel shows, the one
    /// under it come back to (the selection it had then, for a file's plugin), or all of them left for a
    /// disk's folder.
    pub(in crate::app) fn track_plugins(
        &mut self,
        dir: &VfsPath,
        look: &PanelLook,
    ) -> Option<Vec<String>> {
        let entering = std::mem::take(&mut self.entering);
        if std::mem::take(&mut self.popping)
            && self.plugin_stack.last().is_some_and(|top| top.from == *dir)
            && let Some(top) = self.plugin_stack.pop()
        {
            self.restore(&top);
            forget_copies([&top]);
            return top.host.map(|host| host.selected);
        }
        // FAR's `OpenFilePlugin` with `PushPrev` pushes even over the same plugin's panel.
        if let Entering::Push(name, temp) = &entering
            && dir.provider().is_some()
        {
            self.push_over(name.clone(), temp.clone());
            self.start_mode(look);
            return None;
        }
        if let Entering::Push(_, Some(folder)) = &entering {
            // Not opened after all: its copy goes.
            forget_copy(folder.clone());
        }
        if dir.same_mount(&self.path) {
            return None;
        }
        let Some(provider) = dir.provider() else {
            // A disk's folder: every plugin's panel closes (FAR's `SetCurDir`).
            if let Some(first) = self.plugin_stack.first().cloned() {
                self.restore(&first);
            }
            forget_copies(&self.plugin_stack);
            self.plugin_stack.clear();
            return None;
        };
        if entering == Entering::Switch && self.path.provider() == Some(provider) {
            // TmpPanel's switch: its other common panel shown in the place of this one.
            return None;
        }
        // FAR's `ChangePanel` with `CreateNew`: the plugins' panels shown close (`dispose`), the new one
        // comes with what the panel had before the first (`GetPrevViewMode` / `GetPrevSortMode`...).
        let first = match self.plugin_stack.first().cloned() {
            Some(first) => {
                self.restore(&first);
                first
            }
            None => Entered {
                from: self.path.clone(),
                view: self.view_mode,
                sort: self.sort,
                host: None,
            },
        };
        forget_copies(&self.plugin_stack);
        self.plugin_stack = vec![first];
        self.start_mode(look);
        None
    }

    /// FAR's `PushPlugin` with `PrevDataList`: what the panel shows now kept under the file's plugin.
    fn push_over(&mut self, name: String, temp: Option<std::path::PathBuf>) {
        let host = HostFile {
            name,
            selected: self.selected.iter().cloned().collect(),
            temp,
        };
        self.plugin_stack.push(Entered {
            from: self.path.clone(),
            view: self.view_mode,
            sort: self.sort,
            host: Some(host),
        });
    }

    /// FAR's `StartPanelMode` (the program's mode at 6..9 only where the plugin has its own) and
    /// `StartSortMode` / `StartSortOrder`.
    fn start_mode(&mut self, look: &PanelLook) {
        if let Some((mode, descending)) = look.start_sort {
            self.sort.mode = mode;
            self.sort.reverse = descending;
        }
        if let Some(mode) = (look.start_mode)
            .filter(|number| *number < 6 || look.mode(*number).is_some())
            .and_then(ViewMode::for_number)
        {
            self.view_mode = mode;
        }
    }

    /// FAR's `PopPlugin`: the view mode, the sort mode, its order and "directories first" as they were.
    fn restore(&mut self, entered: &Entered) {
        self.view_mode = entered.view;
        self.sort.mode = entered.sort.mode;
        self.sort.reverse = entered.sort.reverse;
        self.sort.directories_first = entered.sort.directories_first;
    }
}
