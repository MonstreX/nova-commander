use super::panel_options::PanelOptions;
use std::collections::BTreeSet;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::ViewMode;
use crate::vfs::{self, Capabilities, Entry, PanelLook, SortOrder, VfsPath, Watch};

/// How long a panel waits for its folder before it lets the keys work and shows "Reading: N files" in
/// its title (FAR's message comes after a moment too). Tests wait longer, so that a loaded machine does
/// not turn them into tests of the message; the reading itself is tested with `read_wait` at zero.
/// The same wait applies to a stopped reading's last entries.
const READ_WAIT: Duration = if cfg!(test) {
    Duration::from_secs(30)
} else {
    Duration::from_millis(300)
};

/// Rows of a panel that are not entries: the frame (2), the column titles (1), the status separator and
/// line (2). FAR can turn titles and the status line off (Options > Panel settings, stage 2b); until then
/// they are always shown, as in FAR's defaults.
pub const NON_LIST_ROWS: u16 = 5;

/// Changes come at a high rate while files are written: FAR shows at most one a second.
const AUTO_REFRESH_GAP: Duration = Duration::from_secs(1);

/// How long an automatic refresh reads before the title says "Reading" (the screen never waits for it).
const QUIET_REFRESH: Duration = Duration::from_millis(300);

/// State of one file panel: directory, listing, cursor, scroll and selection.
#[derive(Debug)]
pub struct Panel {
    pub path: VfsPath,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub offset: usize,
    pub selected: super::panel_totals::Selection,
    pub sort: SortOrder,
    pub view_mode: ViewMode,
    pub options: PanelOptions,
    pub error: Option<String>,
    /// Shown on screen (Ctrl+F1 / Ctrl+F2 / Ctrl+P hide it).
    pub visible: bool,
    /// View metrics written by the renderer: rows per column and number of column stripes.
    height: usize,
    stripes: usize,
    /// The view mode's column types and their widths as the renderer last drew them (FAR's
    /// `FCTL_GETCOLUMNTYPES`, `FCTL_GETCOLUMNWIDTHS`).
    drawn_columns: (String, String),
    /// Hidden and system entries are listed (Ctrl+H, shared by both panels).
    show_hidden: bool,
    /// Ctrl+I: FAR's filters of this panel's area.
    pub(super) filter: crate::vfs::filter::AreaFilter,
    /// The file types the filter hid in the last reading (FAR's `m_FilteredExtensions`).
    pub(super) filtered_types: BTreeSet<String>,
    /// The selection before the last group operation (Ctrl+M).
    pub(super) saved_selection: BTreeSet<String>,
    /// While Shift+movement selects: whether it selects (true) or deselects.
    pub(super) shift_selection: Option<bool>,
    /// A folder still being read after `read_wait`: the keys act on what is shown meanwhile.
    reading: Option<Reading>,
    /// F3's count of folder sizes.
    pub(super) size_count: Option<super::panel_sizes::RunningCount>,
    read_wait: Duration,
    /// Changes of the folder shown (FAR's `FileSystemWatcher`); `None` when its provider reports none.
    watch: Option<Box<dyn Watch>>,
    /// A change seen and not shown yet: the panel was not on screen (FAR's `m_UpdatePending`).
    change_pending: bool,
    last_auto_refresh: Option<Instant>,
    /// `pushd`'s folder, remembered once the folder being entered is (FAR pushes nothing when
    /// `IntChDir` fails); true once it was.
    pushd: Option<(String, bool)>,
    /// FAR's "Network drives autorefresh".
    watch_remote: bool,
    /// A share's root shows ".." (a plugin goes up from it: `plugins::share_up`).
    share_parent: bool,
    /// What the folder shown allows, as its listing said.
    capabilities: Capabilities,
    /// The provider's title and modes (a plugin's panel), as the listing said.
    pub look: PanelLook,
    /// The plugins' panels under this one (FAR's `PluginsList`, `panel_stack`).
    plugin_stack: Vec<Entered>,
    /// ".." at a plugin's root is going back to the panel under it.
    popping: bool,
    /// How the next plugin's panel read comes: anew, in this one's place, over this one.
    entering: stack::Entering,
    /// FAR's sort groups, used while the order says so (Shift+F11).
    pub(super) sort_groups: super::highlighting::SortGroups,
    /// Counts the changes of `entries`, so that their totals are kept until they change (review R32).
    pub(super) entries_version: u64,
    pub(super) kept: super::panel_totals::Kept,
}

/// A whole path's last name (FAR's `PointToName`).
fn last_name(name: &str) -> &str {
    name.rsplit(std::path::is_separator).next().unwrap_or(name)
}

/// What `Panel::poll` found.
#[derive(Debug)]
pub enum Polled {
    Quiet,
    /// The panel or its "Reading" count changed: draw again.
    Changed,
    /// The folder being entered could not be read; the panel stayed where it was. The error's title:
    /// what asked for the folder ("Change drive" from the drive menu).
    Failed(String, io::Error),
}

impl Panel {
    #[cfg(test)]
    pub fn new(path: VfsPath) -> Self {
        Self::with_settings(path, true, true)
    }

    /// A panel as the settings make it, its folder read once with them: hidden files listed or not,
    /// FAR's "Network drives autorefresh".
    pub fn with_settings(path: VfsPath, show_hidden: bool, watch_remote: bool) -> Self {
        let mut panel = Self {
            path,
            entries: Vec::new(),
            cursor: 0,
            offset: 0,
            selected: super::panel_totals::Selection::default(),
            options: PanelOptions::default(),
            sort: SortOrder::default(),
            view_mode: ViewMode::default(),
            error: None,
            visible: true,
            height: 1,
            stripes: 1,
            drawn_columns: (String::new(), String::new()),
            show_hidden,
            filter: crate::vfs::filter::AreaFilter::default(),
            filtered_types: BTreeSet::new(),
            saved_selection: BTreeSet::new(),
            shift_selection: None,
            reading: None,
            size_count: None,
            read_wait: READ_WAIT,
            watch: None,
            change_pending: false,
            last_auto_refresh: None,
            pushd: None,
            watch_remote,
            share_parent: false,
            capabilities: Capabilities::default(),
            look: PanelLook::default(),
            plugin_stack: Vec::new(),
            popping: false,
            entering: stack::Entering::default(),
            sort_groups: super::highlighting::SortGroups::default(),
            entries_version: 0,
            kept: super::panel_totals::Kept::default(),
        };
        panel.reload();
        panel
    }

    /// Re-reads the directory, keeping the cursor on the same name and dropping vanished selections.
    /// A failure shows in the panel itself.
    pub fn reload(&mut self) {
        // While another folder is being entered, reading this one again would only throw that away.
        if self
            .reading
            .as_ref()
            .is_some_and(|reading| reading.entering)
        {
            return;
        }
        if let Some(result) = self.begin(self.path.clone(), None, false) {
            self.reloaded(result);
        }
    }

    /// Re-reads the directory as [`Self::reload`] does, the selection kept, then the cursor on `name`
    /// when it is there (FAR's `Update(UPDATE_KEEP_SELECTION)` and `GoToFile` after a folder made).
    pub fn reload_onto(&mut self, name: &str) {
        if (self.reading.as_ref()).is_some_and(|reading| reading.entering) {
            return;
        }
        if let Some(result) = self.begin(self.path.clone(), Some(name), false) {
            self.reloaded_onto(result, Some(name));
        }
    }

    /// Switches to `path`. A folder that fails within `read_wait` is an error and the panel stays where
    /// it was; a slower one finishes in `poll`.
    pub fn change_dir(&mut self, path: &VfsPath, focus: Option<&str>) -> io::Result<()> {
        if let Some(result) = self.begin(path.clone(), focus, true) {
            match result {
                Ok(listed) => self.entered(listed, focus),
                Err(error) => {
                    self.plugin_move_failed();
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub(super) fn set_entries(&mut self, mut entries: Vec<Entry>, focus: Option<&str>) {
        if !self.show_hidden {
            entries.retain(|entry| {
                entry.is_parent || !(entry.attributes.is_hidden() || entry.attributes.is_system())
            });
        }
        self.apply_filter(&mut entries);
        self.place_entries(entries, focus);
    }

    /// The entries sorted, the cursor on `focus` (FAR's sort, which reads nothing again).
    pub(super) fn place_entries(&mut self, mut entries: Vec<Entry>, focus: Option<&str>) {
        let (selected, groups) = (&self.selected, &self.sort_groups);
        let now = std::time::SystemTime::now();
        let is_selected = |entry: &Entry| selected.contains(&entry.name);
        vfs::sort_entries_grouped(&mut entries, &self.sort, is_selected, |entry| {
            groups.group(entry, now)
        });
        self.entries = entries;
        self.entries_changed();
        self.cursor = focus
            .and_then(|name| self.entries.iter().position(|e| e.name == name))
            .unwrap_or(0)
            .min(self.entries.len().saturating_sub(1));
    }

    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    /// The OS path of the entry under the cursor, unless it is ".." or it has none (a provider's entry
    /// without a real file).
    pub fn current_path(&self) -> Option<PathBuf> {
        let entry = self.current().filter(|entry| !entry.is_parent)?;
        (entry.real_path.clone()).or_else(|| Some(self.path.as_local()?.join(&entry.name)))
    }

    /// Enters the directory under the cursor. Returns false when the cursor is on a file.
    pub fn enter(&mut self) -> io::Result<bool> {
        let Some(entry) = self.current().cloned() else {
            return Ok(false);
        };
        if entry.is_parent {
            self.go_parent()?;
            return Ok(true);
        }
        if !entry.is_dir {
            return Ok(false);
        }
        // A plugin's real folder is entered on its disk (FAR's TmpPanel leaves for it).
        let folder = match &entry.real_path {
            Some(real) => VfsPath::local(real),
            None => self.path.join(&entry.name),
        };
        self.change_dir(&folder, None)?;
        Ok(true)
    }

    /// The folder above; at a provider's root the folder the panel came from.
    pub fn go_parent(&mut self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            let came_from = self.path.file_name();
            return self.change_dir(&parent, came_from.as_deref());
        }
        // FAR's `ChangeDir("..")` at a plugin's root: the panel under it (`PopPlugin`), with real names the
        // cursor on the name of the entry it was on (`GoToPanelFile`, `PopPrevData`'s `FindFile`).
        let real_name = (self.current())
            .filter(|entry| entry.real_path.is_some())
            .map(|entry| last_name(&entry.name).to_string());
        let Some((back, host_file)) = self.pop_plugin() else {
            return Ok(());
        };
        // FAR's `PopPrevData`: a file's plugin back to its file (TmpPanel's `HostFile` on "..").
        let focus = real_name.or(host_file);
        self.change_dir(&back, focus.as_deref())
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let last = self.entries.len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(delta).min(last);
    }

    /// PgUp/PgDn as in FAR: the view and the cursor move by one screen minus one entry.
    pub fn page(&mut self, pages: isize) {
        let step = isize::try_from(self.visible_count().saturating_sub(1).max(1)).unwrap_or(1);
        let delta = step.saturating_mul(pages);
        self.offset = self.offset.saturating_add_signed(delta);
        self.move_cursor(delta);
        self.correct_position();
    }

    /// FAR's `Scroll`: the view and the cursor by `delta` entries (the mouse wheel).
    pub fn scroll(&mut self, delta: isize) {
        self.offset = self.offset.saturating_add_signed(delta);
        self.move_cursor(delta);
        self.correct_position();
    }

    /// Left/Right as in FAR: move by one column of entries, scrolling by a column at the edges.
    pub fn move_column(&mut self, direction: isize) {
        let height = self.height;
        if direction < 0 {
            if self.offset >= height && self.cursor.saturating_sub(self.offset) < height {
                self.offset -= height;
            }
            self.move_cursor(-isize::try_from(height).unwrap_or(1));
        } else {
            if self.cursor + height < self.entries.len()
                && self.cursor.saturating_sub(self.offset) >= (self.stripes - 1) * height
            {
                self.offset += height;
            }
            self.move_cursor(isize::try_from(height).unwrap_or(1));
        }
        self.correct_position();
    }

    /// Number of column stripes in the current view (Brief: 3, Medium: 2, others: 1).
    pub fn stripes(&self) -> usize {
        self.stripes
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.entries.len().saturating_sub(1);
    }

    /// Toggles the selection of the entry under the cursor (never "..") and moves down.
    pub fn toggle_selection(&mut self) {
        if let Some(entry) = self.current().filter(|entry| !entry.is_parent) {
            let name = entry.name.clone();
            if !self.selected.remove(&name) {
                self.selected.insert(name);
            }
        }
        self.move_cursor(1);
        self.selection_changed();
    }

    /// FAR re-sorts after every selection change while "selected first" is on.
    pub(super) fn selection_changed(&mut self) {
        if self.sort.selected_first {
            self.set_sort_order(self.sort);
        }
    }

    /// Whether deleting here goes to a Recycle Bin.
    pub fn can_trash(&self) -> bool {
        self.capabilities.trash
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
    }

    /// Sorted by names without their folders or by whole paths (FAR's `IgnorePaths`, read from the view's
    /// first name column); the entries sorted again when that changes.
    pub(super) fn sort_by_names_only(&mut self, names_only: bool) {
        if self.sort.names_only == names_only {
            return;
        }
        self.sort.names_only = names_only;
        let focus = self.current().map(|entry| entry.name.clone());
        let entries = std::mem::take(&mut self.entries);
        self.place_entries(entries, focus.as_deref());
    }

    /// Called by the renderer with the rows per column and the number of stripes; keeps the cursor on
    /// screen (FAR's `CorrectPosition`).
    pub fn set_view_metrics(&mut self, height: usize, stripes: usize) {
        self.height = height.max(1);
        self.stripes = stripes.max(1);
        self.correct_position();
    }

    /// The view mode's column types and the widths the renderer drew them with.
    pub fn set_drawn_columns(&mut self, types: &str, widths: &[usize]) {
        let widths = (widths.iter().map(ToString::to_string))
            .collect::<Vec<_>>()
            .join(",");
        if self.drawn_columns.0 != types || self.drawn_columns.1 != widths {
            self.drawn_columns = (types.to_string(), widths);
        }
    }

    /// The column types and widths as last drawn ("N,S,D", "20,10,8"); empty before the first drawing.
    pub fn drawn_columns(&self) -> (&str, &str) {
        (&self.drawn_columns.0, &self.drawn_columns.1)
    }

    /// FAR's "Network drives autorefresh", from the next reading on.
    pub fn set_watch_remote(&mut self, watch: bool) {
        self.watch_remote = watch;
    }

    /// Whether a share's root shows "..", from the next reading on; whether it changed.
    pub fn set_share_parent(&mut self, shown: bool) -> bool {
        std::mem::replace(&mut self.share_parent, shown) != shown
    }

    /// Whether a share's root shows ".." (`set_share_parent`).
    #[cfg(test)]
    pub fn share_parent(&self) -> bool {
        self.share_parent
    }

    /// Ctrl+H: lists or hides hidden and system entries, keeping the cursor and selection.
    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
        self.reload();
    }

    pub(super) fn column_height(&self) -> usize {
        self.height
    }

    pub(super) fn visible_count(&self) -> usize {
        self.height * self.stripes
    }

    /// The cursor's row in the middle of the panel, as FAR puts a name its quick search found
    /// (`FindPartName`: the top at the cursor less half the panel's height, its frame, titles and status
    /// lines in), then kept within the list.
    pub(super) fn centre_cursor(&mut self) {
        let window = self.height + usize::from(NON_LIST_ROWS);
        self.offset = self.cursor.saturating_sub((window - 1) / 2);
        self.correct_position();
    }

    pub(in crate::app) fn correct_position(&mut self) {
        let count = self.entries.len();
        if count == 0 {
            self.cursor = 0;
            self.offset = 0;
            return;
        }
        let visible = self.visible_count();
        if self.offset + visible > count {
            self.offset = count.saturating_sub(visible);
        }
        self.cursor = self.cursor.min(count - 1);
        self.offset = self.offset.min(count - 1);
        if self.cursor < self.offset {
            self.offset = self.cursor;
        }
        if self.cursor + 1 > self.offset + visible {
            self.offset = self.cursor + 1 - visible;
        }
    }
}

#[path = "panel_reading.rs"]
mod reading;
#[path = "panel_stack.rs"]
mod stack;

use reading::Reading;

pub use stack::Entered;
pub(in crate::app) use stack::forget_copy;
pub use stack::remove_copies_now;

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
