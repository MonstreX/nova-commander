//! F3 on a folder (FAR's `CountDirSize`): the marked folders, or else the one under the cursor, get
//! their sizes in their rows. As in FAR the count holds the program: a window shows it once it takes a
//! moment, Esc breaks it, other keys do nothing. On a later reading of the folder a size is kept, marked
//! as old.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::dialog::{Dialog, FolderCount};
use super::{App, Overlay, Panel};
use crate::jobs::folder_size::{Counted, Folder, SizeCount};
use crate::vfs::{Entry, FolderSize};

/// FAR's `RedrawTimeout`: a count shorter than this shows no window.
const WINDOW_DELAY: Duration = Duration::from_millis(200);

/// F3's count: the rows it is for and what of it they show.
#[derive(Debug)]
pub(super) struct RunningCount {
    count: SizeCount,
    names: Vec<String>,
    /// The row of each counted folder, looked at again only when the rows moved (review R33).
    rows: HashMap<String, usize>,
    shown: Counted,
    started: Instant,
}

impl App {
    /// While folders are counted: Esc breaks the count, every other key waits for its end (FAR).
    pub(super) fn counting_key(&mut self, key: KeyEvent) -> bool {
        let Some(panel) = self.panels.iter_mut().find(|p| p.size_count.is_some()) else {
            return false;
        };
        if key.code == KeyCode::Esc {
            panel.stop_folder_sizes();
        }
        true
    }

    /// The count's window: up once the count takes a moment, gone when it ends. True on a change.
    pub(super) fn pump_folder_count(&mut self) -> bool {
        let count = self.panels.iter().find_map(Panel::folder_count);
        let window = matches!(self.overlay, Some(Overlay::Dialog(Dialog::Counting(_))));
        let Some(count) = count else {
            if window {
                self.overlay = None;
            }
            return window;
        };
        if !window && self.overlay.is_some() {
            return false;
        }
        let same = matches!(&self.overlay,
            Some(Overlay::Dialog(Dialog::Counting(shown))) if *shown == count);
        self.overlay = Some(Overlay::Dialog(Dialog::Counting(count)));
        !same
    }
}

impl Panel {
    /// Counts on its own thread; the rows are filled by [`poll_folder_sizes`](Self::poll_folder_sizes).
    pub fn count_folder_sizes(&mut self) {
        let folders = self.folders_to_count();
        if folders.is_empty() {
            return;
        }
        let names: Vec<String> = folders.iter().map(|folder| folder.name.clone()).collect();
        let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
        for entry in &mut self.entries {
            if wanted.contains(entry.name.as_str()) {
                entry.size = 0;
                entry.folder_size = FolderSize::Counting;
            }
        }
        self.entries_changed();
        let rows = self.rows_of(&names);
        self.size_count = Some(RunningCount {
            count: SizeCount::start(folders),
            names,
            rows,
            shown: Counted::default(),
            started: Instant::now(),
        });
    }

    /// The marked folders; none marked, the one under the cursor (on ".." the panel's own folder).
    fn folders_to_count(&self) -> Vec<Folder> {
        let folder = |entry: &Entry| Folder {
            name: entry.name.clone(),
            path: if entry.is_parent {
                self.path.clone()
            } else {
                // A plugin's real folder is counted on its disk, as quick view does.
                self.item_path(entry)
            },
        };
        let marked = |entry: &&Entry| {
            entry.is_dir && !entry.is_parent && self.selected.contains(&entry.name)
        };
        let folders: Vec<Folder> = self.entries.iter().filter(marked).map(folder).collect();
        if !folders.is_empty() {
            return folders;
        }
        (self.current().filter(|entry| entry.is_dir))
            .map(folder)
            .into_iter()
            .collect()
    }

    /// Fills the rows from the running count; true when something changed. FAR sorts the panel again
    /// at the end: by size the folders now have one.
    pub fn poll_folder_sizes(&mut self) -> bool {
        let Some(mut running) = self.size_count.take() else {
            return false;
        };
        let now = running.count.now();
        if now == running.shown {
            self.size_count = Some(running);
            return false;
        }
        self.show_counted(&now, &mut running);
        if now.over {
            self.forget_counting(&running.names);
            self.set_sort_order(self.sort);
        } else {
            running.shown = now;
            self.size_count = Some(running);
        }
        true
    }

    /// What the count's window says (FAR's `dirinfo_progress`), once the count takes a moment.
    fn folder_count(&self) -> Option<FolderCount> {
        let running = self.size_count.as_ref()?;
        if running.started.elapsed() < WINDOW_DELAY {
            return None;
        }
        let shown = &running.shown;
        let name = (shown.sizes.last().map(|(name, _)| name))
            .or(running.names.first())
            .map_or(String::new(), |name| self.shown_folder_name(name));
        Some(FolderCount {
            folder: name,
            items: shown.items,
            bytes: shown.sizes.iter().map(|(_, bytes)| *bytes).sum(),
        })
    }

    /// The row's name; for ".." the panel's own folder (FAR).
    fn shown_folder_name(&self, name: &str) -> String {
        let own = (name == "..").then(|| self.path.file_name());
        own.flatten().unwrap_or_else(|| name.to_string())
    }

    /// The rows of the folders `names` now.
    fn rows_of(&self, names: &[String]) -> HashMap<String, usize> {
        let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
        (self.entries.iter().enumerate())
            .filter(|(_, entry)| wanted.contains(entry.name.as_str()))
            .map(|(row, entry)| (entry.name.clone(), row))
            .collect()
    }

    fn show_counted(&mut self, counted: &Counted, running: &mut RunningCount) {
        let moved = |entries: &[Entry], rows: &HashMap<String, usize>| {
            (rows.iter())
                .any(|(name, &row)| entries.get(row).is_none_or(|entry| entry.name != *name))
        };
        // Read again or sorted meanwhile: the rows are looked for once more.
        if moved(&self.entries, &running.rows) {
            running.rows = self.rows_of(&running.names);
        }
        for (index, (name, bytes)) in counted.sizes.iter().enumerate() {
            let Some(&row) = running.rows.get(name) else {
                continue;
            };
            self.entries[row].size = *bytes;
            self.entries[row].folder_size = if index < counted.finished {
                FolderSize::Precise
            } else {
                FolderSize::Counting
            };
        }
        self.entries_changed();
    }

    /// Rows of an ended count that it never finished are as if never counted.
    fn forget_counting(&mut self, names: &[String]) {
        let names: HashSet<&str> = names.iter().map(String::as_str).collect();
        for entry in &mut self.entries {
            if entry.folder_size == FolderSize::Counting && names.contains(entry.name.as_str()) {
                entry.size = 0;
                entry.folder_size = FolderSize::Unknown;
            }
        }
        self.entries_changed();
    }

    /// Esc: the count stops; what was finished keeps its size and the panel is sorted (FAR).
    pub fn stop_folder_sizes(&mut self) {
        if let Some(running) = self.size_count.take() {
            self.forget_counting(&running.names);
            self.set_sort_order(self.sort);
        }
    }

    /// The same folder read again: FAR keeps a counted size and shows that it may be old.
    pub(super) fn keep_folder_sizes(&self, entries: &mut [Entry]) {
        let counted: HashMap<&str, &Entry> = (self.entries.iter())
            .filter(|entry| entry.folder_size != FolderSize::Unknown)
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        for entry in entries.iter_mut().filter(|entry| entry.is_dir) {
            if let Some(old) = counted.get(entry.name.as_str()) {
                entry.size = old.size;
                entry.folder_size = match old.folder_size {
                    FolderSize::Counting => FolderSize::Counting,
                    _ => FolderSize::Outdated,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::super::fileops::tests::app_in;
    use super::super::{MainView, Screen};
    use super::*;
    use crate::vfs::test_tree as tree;
    use crate::vfs::{SortMode, VfsPath};

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
    }

    /// A panel on `tree://<session>/` with folders `big` (300 bytes) and `small` (5), a file `f`.
    fn app_on_tree(session: &str, cursor: &str) -> (App, VfsPath) {
        let root = tree::root(session);
        let big = tree::dir(&root.join("big"));
        tree::write(&big.join("x"), &[1; 300]);
        let small = tree::dir(&root.join("small"));
        tree::write(&small.join("y"), &[1; 5]);
        tree::write(&root.join("f"), &[1; 7]);
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        app.active_panel_mut()
            .change_dir(&root, Some(cursor))
            .unwrap();
        (app, root)
    }

    fn entry<'a>(app: &'a App, name: &str) -> &'a Entry {
        let panel = app.active_panel();
        panel.entries.iter().find(|e| e.name == name).unwrap()
    }

    fn until(app: &mut App, mut done: impl FnMut(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(app) {
            assert!(Instant::now() < deadline, "waited too long");
            app.tick();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn counted(app: &mut App) {
        until(app, |app| app.active_panel().size_count.is_none());
    }

    #[test]
    fn f3_on_a_folder_counts_it_and_opens_nothing() {
        let (mut app, _root) = app_on_tree("f3-one", "big");

        press(&mut app, KeyCode::F(3));
        counted(&mut app);

        assert!(matches!(app.screen, Screen::Panels));
        assert_eq!(entry(&app, "big").size, 300);
        assert_eq!(entry(&app, "big").folder_size, FolderSize::Precise);
        assert_eq!(entry(&app, "small").folder_size, FolderSize::Unknown);
    }

    #[test]
    fn with_marked_folders_those_are_counted_not_the_one_under_the_cursor() {
        let (mut app, _root) = app_on_tree("f3-marked", "small");
        app.active_panel_mut().selected.insert("big".to_string());
        app.active_panel_mut().selected.insert("f".to_string());

        press(&mut app, KeyCode::F(3));
        counted(&mut app);

        assert_eq!(entry(&app, "big").size, 300);
        assert_eq!(entry(&app, "small").folder_size, FolderSize::Unknown);
        assert_eq!(entry(&app, "f").size, 7, "a file keeps its size");
    }

    #[test]
    fn f3_on_the_parent_row_counts_the_panels_own_folder() {
        let (mut app, _root) = app_on_tree("f3-up", "big");
        let big = app.active_panel().path.join("big");
        app.active_panel_mut().change_dir(&big, Some("..")).unwrap();

        press(&mut app, KeyCode::F(3));
        counted(&mut app);

        assert_eq!(entry(&app, "..").size, 300);
    }

    #[test]
    fn f3_on_a_file_still_views_it_whatever_is_marked() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("d")).unwrap();
        std::fs::write(temp.path().join("f.txt"), "text").unwrap();
        let mut app = app_in(temp.path());
        let panel = app.active_panel_mut();
        panel.selected.insert("d".to_string());
        let at = panel.entries.iter().position(|e| e.name == "f.txt");
        panel.cursor = at.unwrap();

        press(&mut app, KeyCode::F(3));

        assert!(matches!(app.screen, Screen::Viewer(_)));
    }

    #[test]
    fn the_panel_is_sorted_again_when_the_count_ends() {
        let (mut app, _root) = app_on_tree("f3-sort", "small");
        app.active_panel_mut()
            .set_sort_direction(SortMode::Size, false);
        let panel = app.active_panel_mut();
        panel
            .selected
            .extend(["big".to_string(), "small".to_string()]);

        press(&mut app, KeyCode::F(3));
        counted(&mut app);

        let names: Vec<&str> = (app.active_panel().entries.iter())
            .map(|e| e.name.as_str())
            .collect();
        let big = names.iter().position(|name| *name == "big");
        let small = names.iter().position(|name| *name == "small");
        assert!(small < big, "{names:?}");
        assert_eq!(app.active_panel().current().unwrap().name, "small");
    }

    fn stalled(app: &mut App) {
        let panel = app.active_panel_mut();
        panel.count_folder_sizes();
        let running = panel.size_count.as_mut().unwrap();
        running.count = SizeCount::stalled();
        running.shown = Counted::default();
    }

    #[test]
    fn a_count_that_ends_without_a_row_leaves_it_as_never_counted() {
        let (mut app, _root) = app_on_tree("f3-lost", "big");
        stalled(&mut app);
        let running = app.active_panel_mut().size_count.as_mut().unwrap();
        running.count = SizeCount::at(Counted {
            over: true,
            ..Counted::default()
        });

        app.tick();

        assert_eq!(entry(&app, "big").folder_size, FolderSize::Unknown);
        assert!(app.active_panel().size_count.is_none());
    }

    #[test]
    fn esc_keeps_what_was_finished_and_sorts() {
        let (mut app, _root) = app_on_tree("f3-esc-sort", "big");
        app.active_panel_mut()
            .set_sort_direction(SortMode::Size, false);
        stalled(&mut app);
        let panel = app.active_panel_mut();
        let row = panel.entries.iter().position(|e| e.name == "big").unwrap();
        (panel.entries[row].size, panel.entries[row].folder_size) = (300, FolderSize::Precise);

        panel.stop_folder_sizes();

        let names: Vec<&str> = (panel.entries.iter()).map(|e| e.name.as_str()).collect();
        let big = names.iter().position(|name| *name == "big");
        let small = names.iter().position(|name| *name == "small");
        assert!(small < big, "{names:?}");
    }

    fn age(app: &mut App) {
        let running = app.active_panel_mut().size_count.as_mut().unwrap();
        running.started = Instant::now() - Duration::from_secs(1);
    }

    fn window(app: &App) -> Option<&FolderCount> {
        match &app.overlay {
            Some(Overlay::Dialog(Dialog::Counting(count))) => Some(count),
            _ => None,
        }
    }

    #[test]
    fn a_short_count_shows_no_window() {
        let (mut app, _root) = app_on_tree("f3-short", "big");
        stalled(&mut app);
        // A loaded machine must not make this "a moment" long.
        let running = app.active_panel_mut().size_count.as_mut().unwrap();
        running.started = Instant::now() + Duration::from_secs(60);

        app.tick();

        assert!(window(&app).is_none());
    }

    #[test]
    fn a_long_count_shows_what_it_counts_and_how_far_it_is() {
        let (mut app, _root) = app_on_tree("f3-window", "big");
        stalled(&mut app);
        age(&mut app);
        let running = app.active_panel_mut().size_count.as_mut().unwrap();
        running.count = SizeCount::at(Counted {
            sizes: vec![("big".to_string(), 1234)],
            items: 5678,
            ..Counted::default()
        });

        app.tick();

        let expected = FolderCount {
            folder: "big".to_string(),
            items: 5678,
            bytes: 1234,
        };
        assert_eq!(window(&app), Some(&expected));
    }

    #[test]
    fn on_the_parent_row_the_window_names_the_panels_folder() {
        let (mut app, root) = app_on_tree("f3-window-up", "big");
        app.active_panel_mut()
            .change_dir(&root.join("big"), Some(".."))
            .unwrap();
        stalled(&mut app);
        age(&mut app);

        app.tick();

        assert_eq!(window(&app).unwrap().folder, "big");
    }

    #[test]
    fn the_window_goes_when_the_count_ends() {
        let (mut app, _root) = app_on_tree("f3-window-end", "big");
        stalled(&mut app);
        age(&mut app);
        app.tick();
        assert!(window(&app).is_some());

        let running = app.active_panel_mut().size_count.as_mut().unwrap();
        running.count = SizeCount::at(Counted {
            over: true,
            ..Counted::default()
        });
        app.tick();

        assert!(window(&app).is_none());
    }

    #[test]
    fn esc_breaks_the_count_and_closes_the_window() {
        let (mut app, _root) = app_on_tree("f3-esc", "big");
        stalled(&mut app);
        age(&mut app);
        app.tick();
        app.command_line.set("typed");

        press(&mut app, KeyCode::Esc);
        app.tick();

        assert!(window(&app).is_none());
        assert!(app.active_panel().size_count.is_none());
        assert_eq!(entry(&app, "big").folder_size, FolderSize::Unknown);
        assert_eq!(app.command_line.text(), "typed", "Esc went to the count");
        assert_eq!(app.main_view, MainView::Panels);
    }

    #[test]
    fn other_keys_do_nothing_while_it_counts() {
        let (mut app, _root) = app_on_tree("f3-modal", "big");
        stalled(&mut app);
        let cursor = app.active_panel().cursor;

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Insert);
        press(&mut app, KeyCode::Char('x'));
        press(&mut app, KeyCode::F(3));

        assert_eq!(app.active_panel().cursor, cursor);
        assert!(app.active_panel().selected.is_empty());
        assert!(app.command_line.is_empty());
        assert!(app.active_panel().size_count.is_some());
    }

    #[test]
    fn with_no_count_keys_work_as_before() {
        let (mut app, _root) = app_on_tree("f3-free", "big");
        app.command_line.set("typed");

        press(&mut app, KeyCode::Esc);

        assert!(app.command_line.is_empty());
    }

    #[test]
    fn a_reading_during_a_count_keeps_it_going() {
        let (mut app, root) = app_on_tree("f3-reread-counting", "big");
        stalled(&mut app);

        tree::write(&root.join("new"), b"1");
        app.active_panel_mut().reload();
        until(&mut app, |app| {
            app.active_panel().entries.iter().any(|e| e.name == "new")
        });

        assert_eq!(entry(&app, "big").folder_size, FolderSize::Counting);
    }

    #[test]
    fn a_reading_keeps_a_size_as_old() {
        let (mut app, root) = app_on_tree("f3-reread", "big");
        press(&mut app, KeyCode::F(3));
        counted(&mut app);

        tree::write(&root.join("new"), b"1");
        app.active_panel_mut().reload();
        until(&mut app, |app| {
            app.active_panel().entries.iter().any(|e| e.name == "new")
        });

        assert_eq!(entry(&app, "big").size, 300);
        assert_eq!(entry(&app, "big").folder_size, FolderSize::Outdated);
    }

    #[test]
    fn leaving_the_folder_stops_the_count() {
        let (mut app, root) = app_on_tree("f3-leave", "big");
        stalled(&mut app);

        let small = root.join("small");
        app.active_panel_mut().change_dir(&small, None).unwrap();

        assert!(app.active_panel().size_count.is_none());
    }
}
