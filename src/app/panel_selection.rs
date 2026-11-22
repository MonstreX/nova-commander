//! Selecting files in a panel as FAR does (`far/filelist.cpp`): Shift+movement keys select or deselect
//! along the way (the first entry decides which, `MoveSelection`), group selection by mask
//! (`SelectFiles`), invert, select all / clear, and Ctrl+M to bring back the previous selection.

use super::Panel;
use crate::jobs::delete;
use crate::vfs::{Entry, FileMask, VfsPath};

impl Panel {
    /// Where the cursor goes if its entry is gone after a reading (FAR's `UpdatePlugin` /
    /// `ReadFileNames`): the first unselected one after it when it was selected, else the same row.
    pub(super) fn cursor_fallback(&self) -> (usize, Option<String>) {
        let selected = self
            .current()
            .is_some_and(|entry| self.selected.contains(&entry.name));
        let next = (selected)
            .then(|| {
                (self.entries.iter().skip(self.cursor + 1))
                    .find(|entry| !self.selected.contains(&entry.name))
                    .map(|entry| entry.name.clone())
            })
            .flatten();
        (self.cursor, next)
    }

    /// The cursor's entry gone: FAR's `GoToFile` on the next unselected name, else the same row (the
    /// entry after it, the last one past the end).
    pub(super) fn keep_cursor_place(
        &mut self,
        focus: Option<&str>,
        row: usize,
        next: Option<&str>,
    ) {
        let found = |name: &str| self.entries.iter().position(|entry| entry.name == name);
        if focus.is_none_or(|name| found(name).is_some()) {
            return;
        }
        self.cursor = next
            .and_then(found)
            .unwrap_or(row)
            .min(self.entries.len().saturating_sub(1));
    }

    /// What F5 and F6 apply to: the selection, or else the entry under the cursor.
    pub fn targets(&self) -> Vec<VfsPath> {
        if self.selected.is_empty() {
            return self
                .current()
                .filter(|entry| !entry.is_parent)
                .map(|entry| self.item_path(entry))
                .into_iter()
                .collect();
        }
        // In the panel's order, as FAR goes over its list (review R41); each by its entry, for its real
        // file; a selected name the panel does not show (none, as a rule) after them.
        let mut left: std::collections::BTreeSet<&str> =
            self.selected.iter().map(String::as_str).collect();
        let mut targets: Vec<VfsPath> = (self.entries.iter())
            .filter(|entry| left.remove(entry.name.as_str()))
            .map(|entry| self.item_path(entry))
            .collect();
        targets.extend(left.into_iter().map(|name| self.path.join(name)));
        targets
    }

    /// Whether files are among what F5 / F6 / Alt+F6 take (FAR's `FilesPresent`; a folder link is a
    /// folder).
    pub fn targets_have_files(&self) -> bool {
        if self.selected.is_empty() {
            return self
                .current()
                .is_some_and(|entry| !entry.is_parent && !entry.is_dir);
        }
        (self.entries.iter()).any(|entry| self.selected.contains(&entry.name) && !entry.is_dir)
    }

    /// What F8 deletes, as FAR's `enum_selected`: the selection, or the entry under the cursor (always
    /// that one for Shift+F8, FAR's `ReturnCurrentFile`); never "..". A plugin's real name is its file
    /// on its disk (ADR-0016 §14: FAR deletes the real files).
    pub fn delete_items(&self, current_only: bool) -> Vec<delete::Item> {
        let item = |entry: &Entry| delete::Item {
            // FAR's `UseInternalCommand(PLUGIN_FARDELETEFILES)`: the plugin's own `DeleteFiles` keeps it.
            path: match &entry.real_path {
                Some(real) if !self.look.own_delete => VfsPath::local(real),
                _ => self.path.join(&entry.name),
            },
            entry: entry.clone(),
        };
        if current_only || self.selected.is_empty() {
            return self
                .current()
                .filter(|entry| !entry.is_parent)
                .map(item)
                .into_iter()
                .collect();
        }
        self.entries
            .iter()
            .filter(|entry| !entry.is_parent && self.selected.contains(&entry.name))
            .map(item)
            .collect()
    }

    /// Where an entry of this panel is for what reads or copies it (what is selected and under the
    /// cursor): its real file (a plugin's real names), else its path here.
    pub fn item_path(&self, entry: &Entry) -> VfsPath {
        match &entry.real_path {
            // FAR's `UseInternalCommand(PLUGIN_FARGETFILES)`: the plugin's own `GetFiles` keeps it.
            Some(real) if !self.look.own_get => VfsPath::local(real),
            _ => self.path.join(&entry.name),
        }
    }

    /// Selects or deselects `index` (never "..").
    fn select_index(&mut self, index: usize, on: bool) {
        let Some(entry) = self.entries.get(index).filter(|entry| !entry.is_parent) else {
            return;
        };
        let name = entry.name.clone();
        if on {
            self.selected.insert(name);
        } else {
            self.selected.remove(&name);
        }
    }

    /// FAR's `get_first_selected`: the first selected entry or else the one under the cursor, unless
    /// that is "..".
    pub fn first_target(&self) -> Option<String> {
        self.entries
            .iter()
            .find(|entry| self.selected.contains(&entry.name))
            .or_else(|| self.current())
            .filter(|entry| !entry.is_parent)
            .map(|entry| entry.name.clone())
    }

    /// The cursor on the entry called `name`, when there is one (FAR's `GoToFile`).
    pub fn go_to_name(&mut self, name: &str) {
        if let Some(index) = self.entries.iter().position(|entry| entry.name == name) {
            self.cursor = index;
        }
    }

    /// Whether the entry under the cursor is selected.
    pub(super) fn current_selected(&self) -> bool {
        self.is_selected(self.cursor)
    }

    /// Selects or deselects the entry under the cursor (never "..").
    pub(super) fn set_current_selected(&mut self, on: bool) {
        self.select_index(self.cursor, on);
    }

    fn is_selected(&self, index: usize) -> bool {
        self.entries
            .get(index)
            .is_some_and(|entry| self.selected.contains(&entry.name))
    }

    /// Takes the mark off a finished object, if this panel still shows its folder. True if it had
    /// one; the caller sorts once after a batch (`selection_changed`).
    pub fn unmark(&mut self, path: &VfsPath) -> bool {
        path.parent().as_ref() == Some(&self.path)
            && path
                .file_name()
                .is_some_and(|name| self.selected.remove(&name))
    }

    /// FAR's `MoveSelection`: the first step decides between selecting and deselecting (from the entry
    /// under the cursor, or the next one when the cursor is on ".."); then the entry is marked and the
    /// cursor moves.
    fn move_selection(&mut self, down: bool) {
        if self.entries.is_empty() {
            return;
        }
        let state = match self.shift_selection {
            Some(state) => state,
            None => {
                let on_parent = self.entries[self.cursor].is_parent;
                let decider = if on_parent && self.cursor + 1 < self.entries.len() {
                    self.cursor + 1
                } else {
                    self.cursor
                };
                let state = !self.is_selected(decider);
                self.shift_selection = Some(state);
                state
            }
        };
        self.select_index(self.cursor, state);
        self.move_cursor(if down { 1 } else { -1 });
    }

    /// Shift+Up / Shift+Down.
    pub fn shift_step(&mut self, down: bool) {
        self.move_selection(down);
        self.selection_changed();
    }

    /// Shift+Home / Shift+End: to the first or last entry, marking all of them.
    pub fn shift_to_edge(&mut self, down: bool) {
        let last = self.entries.len().saturating_sub(1);
        while (down && self.cursor < last) || (!down && self.cursor > 0) {
            self.move_selection(down);
        }
        self.move_selection(down);
        self.selection_changed();
    }

    /// Shift+PgUp / Shift+PgDn: one screen minus one entry.
    pub fn shift_page(&mut self, down: bool) {
        for _ in 0..self.visible_count().saturating_sub(1) {
            self.move_selection(down);
        }
        self.selection_changed();
    }

    /// Shift+Left / Shift+Right in multi-column views: one column, marking the entry it lands on too.
    pub fn shift_column(&mut self, down: bool) {
        if self.stripes() < 2 || self.entries.is_empty() {
            return;
        }
        for _ in 0..self.column_height() {
            self.move_selection(down);
        }
        let state = self.shift_selection.unwrap_or(true);
        self.select_index(self.cursor, state);
        self.selection_changed();
    }

    /// Shift was released: the next Shift+movement decides anew.
    pub fn end_shift_selection(&mut self) {
        self.shift_selection = None;
    }

    /// Gray +/- with a mask (FAR's `SELECT_ADD` / `SELECT_REMOVE`).
    pub fn select_by_mask(&mut self, mask: &FileMask, on: bool) {
        self.save_selection();
        for index in 0..self.entries.len() {
            let entry = &self.entries[index];
            if !mask.matches(&entry.name) {
                continue;
            }
            if !entry.is_dir || self.options.select_folders || !on {
                self.select_index(index, on);
            }
        }
        self.selection_changed();
    }

    /// Select group's Filter: the entries `takes` takes, folders too (FAR's `bUseFilter`).
    pub fn select_where(&mut self, on: bool, takes: impl Fn(&crate::vfs::Entry) -> bool) {
        self.save_selection();
        for index in 0..self.entries.len() {
            if takes(&self.entries[index]) {
                self.select_index(index, on);
            }
        }
        self.selection_changed();
    }

    /// Shift+Gray+: every file (and folder with "Select folders").
    pub fn select_all(&mut self) {
        self.save_selection();
        for index in 0..self.entries.len() {
            if !self.entries[index].is_dir || self.options.select_folders {
                self.select_index(index, true);
            }
        }
        self.selection_changed();
    }

    /// Shift+Gray-.
    pub fn clear_selection(&mut self) {
        self.save_selection();
        self.selected.clear();
        self.selection_changed();
    }

    /// Gray* (files, folders too with "Select folders"), Ctrl+Gray* (everything), Alt+Gray* (files).
    pub fn invert_selection(&mut self, folders: bool) {
        self.save_selection();
        for index in 0..self.entries.len() {
            let on = !self.is_selected(index);
            if !self.entries[index].is_dir || folders || !on {
                self.select_index(index, on);
            }
        }
        self.selection_changed();
    }

    /// The mask of the current entry's extension, for Ctrl+Gray+/- (`"*.ext"`, or `*.` without one).
    pub fn current_extension_mask(&self) -> Option<String> {
        let entry = self.current().filter(|entry| !entry.is_parent)?;
        Some(match entry.name.rfind('.') {
            Some(dot) => format!("\"*{}\"", literal_brackets(&entry.name[dot..])),
            None => "*.".to_string(),
        })
    }

    /// The mask of the current entry's name without extension, for Alt+Gray+/-.
    pub fn current_name_mask(&self) -> Option<String> {
        let entry = self.current().filter(|entry| !entry.is_parent)?;
        let name = entry
            .name
            .rfind('.')
            .map_or(entry.name.as_str(), |dot| &entry.name[..dot]);
        Some(format!("\"{}.*\"", literal_brackets(name)))
    }

    pub(super) fn save_selection(&mut self) {
        self.saved_selection = self.selected.to_set();
    }

    /// Ctrl+M: back to the selection before the last group operation (pressing again swaps back).
    pub fn restore_selection(&mut self) {
        let now = self.selected.to_set();
        self.selected
            .replace(std::mem::replace(&mut self.saved_selection, now));
        let names: std::collections::BTreeSet<String> = self
            .entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        self.selected.retain(|name| names.contains(name));
        self.selection_changed();
    }
}

/// FAR's `WrapBrackets`: a name's brackets become one-character sets so the mask matches them literally.
fn literal_brackets(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '[' => result.push_str("[[]"),
            ']' => result.push_str("[]]"),
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(names: &[&str]) -> (tempfile::TempDir, Panel) {
        let temp = tempfile::tempdir().unwrap();
        for name in names {
            if let Some(dir) = name.strip_suffix('/') {
                std::fs::create_dir(temp.path().join(dir)).unwrap();
            } else {
                std::fs::write(temp.path().join(name), "x").unwrap();
            }
        }
        let mut panel = Panel::new(crate::vfs::VfsPath::local(temp.path()));
        panel.set_view_metrics(20, 1);
        (temp, panel)
    }

    fn selected(panel: &Panel) -> Vec<&str> {
        panel.selected.iter().map(String::as_str).collect()
    }

    #[test]
    fn shift_down_selects_along_and_the_first_entry_decides() {
        let (_temp, mut panel) = panel(&["a.txt", "b.txt", "c.txt"]);
        // On "..": the next entry decides (not selected -> select).
        panel.shift_step(true);
        panel.shift_step(true);
        panel.shift_step(true);
        assert_eq!(selected(&panel), ["a.txt", "b.txt"]);
        panel.end_shift_selection();
        // Starting on a selected entry deselects.
        panel.home();
        panel.move_cursor(1);
        panel.shift_step(true);
        assert_eq!(selected(&panel), ["b.txt"]);
    }

    #[test]
    fn shift_end_marks_through_the_last_entry() {
        let (_temp, mut panel) = panel(&["a", "b", "c"]);
        panel.shift_to_edge(true);
        assert_eq!(selected(&panel), ["a", "b", "c"]);
        assert_eq!(panel.cursor, 3);
    }

    #[test]
    fn group_selection_by_mask_skips_folders_but_deselects_them() {
        let (_temp, mut panel) = panel(&["src/", "main.rs", "lib.rs", "notes.md"]);
        panel.select_by_mask(&FileMask::parse("*.rs").unwrap(), true);
        assert_eq!(selected(&panel), ["lib.rs", "main.rs"]);
        panel.select_by_mask(&FileMask::parse("*.*").unwrap(), true);
        assert_eq!(selected(&panel), ["lib.rs", "main.rs", "notes.md"]);
        panel.select_by_mask(&FileMask::parse("m*").unwrap(), false);
        assert_eq!(selected(&panel), ["lib.rs", "notes.md"]);
    }

    #[test]
    fn invert_all_clear_and_restore() {
        let (_temp, mut panel) = panel(&["dir/", "a", "b"]);
        panel.select_all();
        assert_eq!(selected(&panel), ["a", "b"]);
        panel.clear_selection();
        assert!(panel.selected.is_empty());
        panel.restore_selection();
        assert_eq!(selected(&panel), ["a", "b"], "Ctrl+M brings it back");
        panel.invert_selection(false);
        assert!(
            panel.selected.is_empty(),
            "files inverted, folders untouched"
        );
        panel.invert_selection(true);
        assert_eq!(selected(&panel), ["a", "b", "dir"]);
    }

    #[test]
    fn masks_from_names_with_brackets_select_that_name_only() {
        let (_temp, mut panel) =
            panel(&["file[1].txt", "file1.txt", "song [live].mp3", "song l.mp3"]);
        let index = |panel: &Panel, name: &str| {
            panel
                .entries
                .iter()
                .position(|entry| entry.name == name)
                .unwrap()
        };
        panel.cursor = index(&panel, "song [live].mp3");
        let mask = panel.current_name_mask().unwrap();
        assert_eq!(mask, "\"song [[]live[]].*\"");
        panel.select_by_mask(&FileMask::parse(&mask).unwrap(), true);
        assert_eq!(selected(&panel), ["song [live].mp3"]);
        panel.cursor = index(&panel, "file[1].txt");
        let mask = panel.current_name_mask().unwrap();
        panel.select_by_mask(&FileMask::parse(&mask).unwrap(), true);
        assert!(panel.selected.contains("file[1].txt"));
        assert!(!panel.selected.contains("file1.txt"));
    }

    #[test]
    fn masks_from_the_current_entry() {
        let (_temp, mut panel) = panel(&["report.final.pdf", "Makefile"]);
        panel.end();
        assert_eq!(
            panel.current().map(|e| e.name.as_str()),
            Some("report.final.pdf")
        );
        assert_eq!(panel.current_extension_mask().as_deref(), Some("\"*.pdf\""));
        assert_eq!(
            panel.current_name_mask().as_deref(),
            Some("\"report.final.*\"")
        );
        panel.move_cursor(-1);
        assert_eq!(panel.current_extension_mask().as_deref(), Some("*."));
        panel.home();
        assert_eq!(panel.current_extension_mask(), None);
    }
}
