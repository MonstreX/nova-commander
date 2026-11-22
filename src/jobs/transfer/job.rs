//! The worker thread of one operation and the handle the UI keeps: progress to read, questions to
//! answer, pause and cancel. A second thread counts the totals ahead of the copy (ADR-0009 §3).

use std::collections::HashSet;
use std::thread;
use std::time::SystemTime;

use super::{Answer, Host, Progress, Question, Reply, Request, sides};
use crate::jobs::tree_cache::TreeCache;
use crate::jobs::work::{Handle, Job as Work, Worker};
use crate::vfs::filter::AreaFilter;
use crate::vfs::{Entry, VfsPath, provider_for};

/// The totals walker publishes its count after this many entries.
const TOTALS_BATCH: u64 = 256;

/// A running copy or move, as the UI holds it.
pub type Job = Work<Question, Reply, Progress>;

impl Work<Question, Reply, Progress> {
    /// Starts the operation on its own thread, with the totals walker beside it; a junction or a
    /// symbolic link is one object, whatever it leads to. Its folders go into the trees' files as it
    /// ends.
    pub fn start(request: Request, tree_cache: Option<TreeCache>) -> Self {
        let sources = request.sources.clone();
        let follow = request.keep.link_contents;
        let walks = request.walks();
        let filter = request.filter.clone();
        let job = Self::spawn(move |worker| {
            let plan = tree_cache.map(|cache| cache.before_transfer(&request));
            let report = super::run(&request, worker);
            if let Some(plan) = plan {
                plan.apply();
            }
            report
        });
        let totals = job.handle();
        if walks {
            // Without a thread the totals stay unknown (review R24: no panic).
            // Under the task's stop: a plugin it waits for hears a cancel (R7 review B4: the thread
            // stayed blocked on a hung plugin after the task was cancelled).
            let stop = totals.cancel_flag();
            let count = move || count_totals(&totals, sources, follow, filter.as_ref());
            let _ = thread::Builder::new()
                .name("copy totals".to_string())
                .spawn(move || crate::vfs::stop::stopped_by(stop, count));
        } else {
            let mut progress = totals.progress();
            progress.files_total = u64::try_from(sources.len()).unwrap_or(u64::MAX);
            progress.totals_known = true;
        }
        job
    }
}

impl Host for Worker<Question, Reply, Progress> {
    fn ask(&mut self, question: Question) -> Reply {
        Worker::ask(self, question, Reply::once(Answer::Cancel))
    }

    fn stop_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.cancel_flag()
    }

    fn cancelled(&mut self) -> bool {
        Worker::cancelled(self)
    }

    fn progress(&mut self, change: &mut dyn FnMut(&mut Progress)) {
        Worker::progress(self, change);
    }
}

/// What the count takes `entry` for: with links followed what it leads to, once for each place a link
/// leads to (a link back up would not end); `None` for one already seen or leading nowhere.
fn as_walked(
    path: &VfsPath,
    entry: Entry,
    follow: bool,
    places: &mut HashSet<(u32, u64)>,
) -> Option<Entry> {
    if !follow || !entry.points_elsewhere() || sides::disk(path).is_none() {
        return Some(entry);
    }
    let leads_to = sides::followed(path).ok()?;
    let first = !leads_to.is_dir || sides::identity(path).is_some_and(|place| places.insert(place));
    first.then_some(leads_to)
}

/// Counts files and bytes from the listings (nothing is opened); links count as files of their own,
/// unless they are followed; what the filter leaves out is not counted (FAR's `GETDIRINFO_USEFILTER`).
fn count_totals(
    shared: &Handle<Progress>,
    sources: Vec<VfsPath>,
    follow: bool,
    filter: Option<&(AreaFilter, SystemTime)>,
) {
    let mut pending: Vec<(VfsPath, Entry)> = sources
        .into_iter()
        .filter_map(|path| sides::stat(&path).ok().map(|entry| (path, entry)))
        .collect();
    let (mut files, mut bytes, mut seen) = (0u64, 0u64, 0u64);
    let mut places = HashSet::new();
    while let Some((path, entry)) = pending.pop() {
        if shared.cancelled() {
            return;
        }
        if super::filtered_out(filter, &entry) {
            continue;
        }
        let Some(entry) = as_walked(&path, entry, follow, &mut places) else {
            continue;
        };
        if sides::is_real_folder(&entry) {
            // A folder that cannot be read is the copy's to report.
            let _ = provider_for(&path).and_then(|provider| {
                provider.list(&path, &shared.cancel_flag(), &mut |child| {
                    pending.push((path.join(&child.name), child));
                })
            });
        } else {
            files += 1;
            bytes += entry.size;
        }
        seen += 1;
        if seen % TOTALS_BATCH == 0 {
            let mut progress = shared.progress();
            (progress.files_total, progress.bytes_total) = (files, bytes);
        }
    }
    let mut progress = shared.progress();
    (progress.files_total, progress.bytes_total) = (files, bytes);
    progress.totals_known = true;
}
