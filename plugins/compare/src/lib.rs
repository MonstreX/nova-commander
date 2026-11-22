//! Advanced compare (FAR's compare, `far-source/plugins/compare/Compare.cpp`): the two file panels' items
//! compared - all, or the selected ones only - by time, size and contents, in subfolders too, each panel's
//! filter as it is; the items that differ or have no pair on the other panel are selected, the same ones
//! not, the others as they were. A window tells the files compared; Esc stops it, the selection then as
//! it was ("Operation was interrupted"; ADR-0019). Only two file panels (no tree, info or quick view);
//! for a plugin's panel subfolders and contents are off.

mod compare;
mod contents;
mod options;

pub use compare::{HIDDEN, Item, Run, Side, same_time};
pub use contents::{Contents, Ignore};
pub use options::{OK, Options, TITLE};

use std::path::Path;
use std::time::{Duration, Instant};

use nc_plugin_sdk::{
    FileEntry, Host, Info, Message, PanelItem, PanelKind, Plugin, Which, export_plugin,
};

/// FAR's least wait between two windows telling the files compared.
const SHOW_EVERY: Duration = Duration::from_millis(500);

/// A panel's item to compare, and whether it is selected.
fn item_of(item: &PanelItem) -> Item {
    Item {
        name: item.name.clone(),
        folder: item.folder,
        attributes: item.attributes,
        size: item.size,
        modified: item.modified,
        created: item.created,
        accessed: item.accessed,
        changed: item.changed,
        path: item.path.as_ref().map(Into::into),
    }
}

/// An item as a panel's filter takes it.
fn entry_of(item: &Item) -> FileEntry {
    const READ_ONLY: u32 = 0x1;
    const SYSTEM: u32 = 0x4;
    FileEntry {
        name: item.name.clone(),
        is_dir: item.folder,
        size: item.size,
        modified: item.modified,
        created: item.created,
        accessed: item.accessed,
        changed: item.changed,
        hidden: item.attributes & compare::HIDDEN != 0,
        system: item.attributes & SYSTEM != 0,
        read_only: item.attributes & READ_ONLY != 0,
        attributes: item.attributes,
        ..FileEntry::default()
    }
}

/// Which of `items` the filter of the panel `which` lets through (all without a filter).
fn passing(host: &Host, which: Which, items: &[Item]) -> Vec<bool> {
    let entries: Vec<FileEntry> = items.iter().map(entry_of).collect();
    host.filter(which, &entries)
        .unwrap_or_else(|| vec![true; items.len()])
}

/// The items of a panel compared (FAR's `BuildPanelIndex`): no `..`, folders only with subfolders, the
/// selected ones only when asked and the panel has some, its filter's; each with its place in `items`.
fn compared(
    host: &Host,
    which: Which,
    items: &[PanelItem],
    options: &Options,
) -> Vec<(usize, Item)> {
    let selection = items.iter().any(|item| item.selected);
    let chosen: Vec<(usize, Item)> = (items.iter().enumerate())
        .filter(|(_, item)| !item.parent && (options.subfolders || !item.folder))
        .filter(|(_, item)| !(options.selected_only && selection) || item.selected)
        .map(|(at, item)| (at, item_of(item)))
        .collect();
    let plain: Vec<Item> = chosen.iter().map(|(_, item)| item.clone()).collect();
    (chosen.into_iter().zip(passing(host, which, &plain)))
        .filter_map(|(chosen, passes)| passes.then_some(chosen))
        .collect()
}

/// A path cut to its last `width` characters for the window.
fn shown(path: &Path, width: usize) -> String {
    let text = path.to_string_lossy();
    let count = text.chars().count();
    match count > width {
        true => format!(
            "...{}",
            text.chars().skip(count + 3 - width).collect::<String>()
        ),
        false => text.into_owned(),
    }
}

pub struct Compare {
    host: Host,
}

impl Compare {
    fn warning(&self, lines: &[&str]) {
        let _ = (self.host).show(&Message::new(TITLE, lines).warning().buttons(&["Ok"]));
    }

    /// The comparison of the two panels as FAR's `OpenW`.
    fn run(&self) {
        let host = &self.host;
        let (Some(active), Some(passive)) = (host.panel(Which::Active), host.panel(Which::Passive))
        else {
            return;
        };
        if active.kind != PanelKind::Files || passive.kind != PanelKind::Files {
            self.warning(&["Two file panels are required to perform the compare"]);
            return;
        }
        let items = (
            host.panel_items(Which::Active),
            host.panel_items(Which::Passive),
        );
        let plugins = active.plugin.is_some() || passive.plugin.is_some();
        let selection = items.0.iter().chain(&items.1).any(|item| item.selected);
        let dialog = Options::load(host).dialog(plugins, selection);
        let closed = host.dialog_with(&dialog, |event, controls| Options::handle(&event, controls));
        let Some(mut options) = Options::from_dialog(host, &closed, (plugins, selection)) else {
            return;
        };
        if plugins {
            options.subfolders = false;
            options.contents = false;
        }
        self.compare(&options, active.show_hidden, &items);
    }

    fn compare(&self, options: &Options, hidden: bool, items: &(Vec<PanelItem>, Vec<PanelItem>)) {
        let host = &self.host;
        let a = compared(host, Which::Active, &items.0, options);
        let p = compared(host, Which::Passive, &items.1, options);
        let width = host
            .screen()
            .map_or(60, |screen| screen.columns.saturating_sub(20) as usize)
            .max(20);
        let mut last: Option<Instant> = None;
        let mut progress = |first: &Path, second: &Path| {
            if last.is_some_and(|at| at.elapsed() < SHOW_EVERY) {
                return;
            }
            last = Some(Instant::now());
            let line = "\u{2500}".repeat(width);
            host.busy(TITLE, &[&shown(first, width), &line, &shown(second, width)]);
        };
        let filter = |side: Side, items: &[Item]| match side {
            Side::Active => passing(host, Which::Active, items),
            Side::Passive => passing(host, Which::Passive, items),
        };
        let block = usize::try_from(host.int_setting("CompareBufferSize", 32_768)).unwrap_or(0);
        let stop = || host.cancelled();
        let mut run = Run::new(options, hidden, block, &stop, &mut progress, &filter);
        let plain = |list: &[(usize, Item)]| {
            list.iter()
                .map(|(_, item)| item.clone())
                .collect::<Vec<_>>()
        };
        let (marks_a, marks_p, same) = run.compare(&plain(&a), &plain(&p), true, 0);
        if run.stopped {
            self.warning(&["Operation was interrupted", "The selection is as it was"]);
            return;
        }
        let open_failed = run.open_failed;
        self.select(Which::Active, &items.0, &a, &marks_a);
        self.select(Which::Passive, &items.1, &p, &marks_p);
        if open_failed {
            let message = Message::new("Error", &["Some files couldn't been opened"]);
            let _ = host.show(&message.warning().buttons(&["Ok"]));
        }
        if same && options.message_when_no_difference {
            let message = Message::new(
                "No difference",
                &["The compared files seem to be identical"],
            );
            let _ = host.show(&message.buttons(&["Ok"]));
        }
    }

    /// The compared items of a panel selected as marked (FAR's `FCTL_SETSELECTION`), the others kept.
    fn select(
        &self,
        which: Which,
        items: &[PanelItem],
        compared: &[(usize, Item)],
        marks: &[bool],
    ) {
        // Only those whose selection changes.
        let names = |wanted: bool| -> Vec<String> {
            (compared.iter().zip(marks))
                .filter(|(_, mark)| **mark == wanted)
                .filter_map(|((at, _), _)| items.get(*at))
                .filter(|item| item.selected != wanted)
                .map(|item| item.name.clone())
                .collect()
        };
        let _ = self.host.select(which, &names(true), true);
        let _ = self.host.select(which, &names(false), false);
    }
}

impl Plugin for Compare {
    fn new(host: Host) -> Self {
        Self { host }
    }

    fn info(&self) -> Info {
        Info {
            title: TITLE.to_string(),
            description: "The two panels' files compared, the different ones selected".to_string(),
            author: "nova-commander".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        self.run();
        Ok(None)
    }
}

export_plugin!(Compare);
