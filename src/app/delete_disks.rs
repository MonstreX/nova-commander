//! What F8's question asks of the disks before it is shown (ADR-0019, review R11: off the screen's
//! thread): whether the items can go to a Recycle Bin, the shell's own answer for the first one (FAR's
//! `can_recycle`), the tree's folder looked at; and the task's volumes before it starts.

use crate::app::App;
use crate::app::transfer::{BACKGROUND, ShownTask};
use crate::jobs::delete::{Counter, Request};

/// Whether every item can go to a Recycle Bin, each folder asked of its provider once: a plugin's real
/// files come from several disks, and the shell deletes for good what a disk's bin cannot take.
pub(super) fn all_can_trash(items: &[crate::jobs::delete::Item]) -> bool {
    let mut asked: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    !items.is_empty()
        && items.iter().all(|item| {
            let folder = item.path.parent().unwrap_or_else(|| item.path.clone());
            *asked.entry(folder.to_string()).or_insert_with(|| {
                crate::vfs::provider_for(&item.path)
                    .is_ok_and(|provider| provider.capabilities(&item.path).trash)
            })
        })
}

/// FAR's probe of the first item to delete (`ShellDelete::ShellDelete`, `os::com::can_recycle`): the
/// shell's own answer for a file on a disk; `None` for one elsewhere or when it could not be asked.
pub(super) fn first_recyclable(items: &[crate::jobs::delete::Item]) -> Option<bool> {
    let first = items.first()?.path.as_local()?;
    crate::vfs::local::can_recycle(first)
}

/// Whether the shell would recycle the items: FAR's probe of the first (`None`, not asked: yes), and each
/// file no bigger than its disk's Recycle Bin takes, which the shell deletes for good with no question
/// though the probe says yes (review R1, checked on a test disk; owner's choice over FAR). A folder's
/// size is not known before the question.
pub(super) fn recyclable(items: &[crate::jobs::delete::Item]) -> bool {
    let mut biggest: std::collections::HashMap<&std::path::Path, u64> =
        std::collections::HashMap::new();
    for item in items.iter().filter(|item| !item.entry.is_dir) {
        if let Some(path) = item.path.as_local() {
            let size = biggest.entry(path.parent().unwrap_or(path)).or_default();
            *size = (*size).max(item.entry.size);
        }
    }
    first_recyclable(items) != Some(false)
        && (biggest.iter()).all(|(folder, &size)| crate::vfs::local::bin_takes(folder, size))
}

/// What F8 deletes, before its disks are asked: the tree's folder (none on the tree's root), the panel's
/// items and whether its folder has a Recycle Bin, or a plugin's real files.
pub(super) enum Asked {
    Tree(Option<std::path::PathBuf>),
    Panel(Vec<crate::jobs::delete::Item>, bool),
    Real(Vec<crate::jobs::delete::Item>),
}

impl Asked {
    /// The items and whether they can go to a Recycle Bin, as their disks say now.
    pub(super) fn looked_at(self) -> (Vec<crate::jobs::delete::Item>, bool) {
        match self {
            Self::Tree(folder) => {
                let items = crate::app::tree_panel::delete_items_of(folder);
                let can_trash = crate::app::tree_panel::can_trash(&items);
                (items, can_trash)
            }
            Self::Panel(items, can_trash) => (items, can_trash),
            Self::Real(items) => {
                let can_trash = all_can_trash(&items);
                (items, can_trash)
            }
        }
    }
}

impl App {
    /// The deletion as a task, its progress in front; `counter`: the question's count to go on with.
    /// Its volumes are asked of the disks off the screen's thread (ADR-0019, review R19).
    pub(in crate::app) fn start_delete(&mut self, request: Request, counter: Option<Counter>) {
        // The question's count waits with the start: Esc stops it with the start (review of 19-3).
        let kept = std::sync::Arc::new(std::sync::Mutex::new(counter));
        let dropped = std::sync::Arc::clone(&kept);
        let on_stop: Box<dyn FnOnce() + Send> = Box::new(move || {
            drop(dropped.lock().map(|mut counter| counter.take()));
        });
        self.wait_to_start(Some(on_stop), move || {
            let volumes = crate::jobs::task_volumes::of_delete(&request);
            Box::new(move |app: &mut App| {
                let counter = kept.lock().ok().and_then(|mut counter| counter.take());
                app.start_delete_on(request, counter, volumes);
            })
        });
        self.label_waiting("Looking at the disks…");
    }

    fn start_delete_on(
        &mut self,
        request: Request,
        counter: Option<Counter>,
        volumes: Vec<crate::platform::volumes::VolumeId>,
    ) {
        let id = self.tasks.add_delete_on(request, counter, volumes);
        // What the tree takes is marked nowhere.
        if !self.tree_has_keys() {
            self.work.task_marks.insert(id, self.active);
        }
        self.shown_task = Some(ShownTask {
            id,
            focus: BACKGROUND,
            rest: Vec::new(),
        });
    }
}
