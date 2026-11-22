//! A deletion on its own worker thread, as the UI holds it.

use std::thread;
use std::time::Duration;

use super::{Answer, Counter, Host, Mode, Progress, Question, Reply, Request, Tally};
use crate::jobs::tree_cache::TreeCache;
use crate::jobs::work::{Handle, Job as Work, Worker};

/// How often the follower looks whether the deletion was cancelled.
const FOLLOW_TICK: Duration = Duration::from_millis(100);

/// A running deletion: progress to read, questions to answer, pause and cancel.
pub type Job = Work<Question, Reply, Progress>;

impl Work<Question, Reply, Progress> {
    /// Starts the deletion on its own thread. With `count_totals` its total comes from `counter` (the
    /// count the question started) or from a count started now, beside the deletion. To the Recycle
    /// Bin the count ends first (it would hold folders open that the bin renames) and the total is the
    /// selected objects: the bin takes each in one step. The folders gone leave the trees' files as it
    /// ends.
    pub fn start(
        request: Request,
        counter: Option<Counter>,
        tree_cache: Option<TreeCache>,
    ) -> Self {
        if request.mode == Mode::Trash {
            return Self::spawn(move |worker| {
                if let Some(counter) = counter {
                    counter.finish();
                }
                if request.count_totals {
                    let selected = u64::try_from(request.items.len()).unwrap_or(u64::MAX);
                    worker.progress(&mut |progress: &mut Progress| {
                        progress.total = selected;
                        progress.totals_known = true;
                    });
                }
                run_keeping(&request, worker, tree_cache.as_ref())
            });
        }
        let counter = request
            .count_totals
            .then(|| counter.unwrap_or_else(|| Counter::start(request.items.clone())));
        let job = Self::spawn(move |worker| run_keeping(&request, worker, tree_cache.as_ref()));
        if let Some(counter) = counter {
            let handle = job.handle();
            // No thread to follow the count: the deletion goes on without its total.
            let _ = thread::Builder::new()
                .name("delete total".to_string())
                .spawn(move || follow(&counter, &handle));
        }
        job
    }
}

/// The deletion, then the trees' files.
fn run_keeping(
    request: &Request,
    worker: &mut Worker<Question, Reply, Progress>,
    tree_cache: Option<&TreeCache>,
) -> super::Report {
    let report = super::run(request, worker);
    if let Some(cache) = tree_cache {
        cache.after_delete(request);
    }
    report
}

/// Copies the count into the progress as it grows, until it is over or the deletion is cancelled
/// (dropping the counter stops it).
fn follow(counter: &Counter, handle: &Handle<Progress>) {
    let mut last = (Tally::default(), false);
    while !handle.cancelled() {
        let (tally, known) = counter.wait(last, FOLLOW_TICK);
        let mut progress = handle.progress();
        progress.total = tally.objects();
        progress.totals_known = known;
        if known {
            return;
        }
        last = (tally, known);
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
