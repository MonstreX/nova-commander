//! What every background operation shares (ADR-0010): a worker thread, its progress, the questions it
//! asks and waits on, pause, cancel and the report it ends with. Copy and move (`transfer`) and delete
//! each fill it with their own questions, replies and progress.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::vfs::VfsPath;

/// How often a paused worker looks whether it may go on.
const PAUSE_POLL: Duration = Duration::from_millis(50);
/// How long a job dropped at the program's end is waited for (review R12): one stuck on a dead share
/// or a plugin is left to the process's end.
const DROP_WAIT: Duration = Duration::from_secs(2);
/// A deep tree is walked by recursion on the worker: room for the deepest path Windows allows.
const WORKER_STACK: usize = 64 * 1024 * 1024;

/// An object left out after an error: where it was going, and which selected object it is part of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub source: VfsPath,
    pub destination: VfsPath,
    pub top: VfsPath,
    /// It lies behind a followed link: a move leaves it where it is.
    pub inside_link: bool,
}

/// How the operation ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// One line per item left out because of an error.
    pub failures: Vec<String>,
    /// One line per thing the target could not keep (a copy that is there, without it).
    pub not_kept: Vec<String>,
    /// What can be tried again ("Retry skipped"): errors, not refusals or the user's own Skip.
    pub skipped: Vec<Skipped>,
    pub cancelled: bool,
    /// FAR's `strRenamedName`: the one item copied or moved to a new name in its own folder (a bare
    /// name typed), where the panel's cursor goes after it (R7 review D3).
    pub renamed: Option<VfsPath>,
}

/// What the UI and the worker share.
struct Shared<P> {
    progress: Mutex<P>,
    /// Shared with what the job lists, so that a big or dead folder's listing stops with the job
    /// (review R15).
    cancel: Arc<AtomicBool>,
    pause: AtomicBool,
}

/// The stop flag of a job's host that has none of its own (tests): never set.
pub fn never_stopped() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

/// The progress and the cancel flag, for a helper thread of the job (copy's totals walker).
pub struct Handle<P>(Arc<Shared<P>>);

impl<P> Handle<P> {
    pub fn cancelled(&self) -> bool {
        self.0.cancel.load(Ordering::Relaxed)
    }

    /// The job's cancel flag, for what it lists (`Provider::list` stops once it is set).
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0.cancel)
    }

    pub fn progress(&self) -> MutexGuard<'_, P> {
        self.0
            .progress
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// A running operation, as the UI holds it. `Q` is what it asks, `R` the reply, `P` its progress.
pub struct Job<Q, R, P> {
    shared: Handle<P>,
    questions: Receiver<Q>,
    /// Dropped on cancel: a worker waiting for an answer then gets its cancel reply.
    replies: Option<Sender<R>>,
    pending: Option<Q>,
    worker: Option<JoinHandle<Report>>,
    /// The report of a job whose thread could not be made.
    ended: Option<Report>,
}

impl<Q, R, P> Job<Q, R, P>
where
    Q: Send + 'static,
    R: Send + 'static,
    P: Default + Send + 'static,
{
    /// Runs `work` on its own thread with the worker's side of the channels.
    pub fn spawn(work: impl FnOnce(&mut Worker<Q, R, P>) -> Report + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            progress: Mutex::new(P::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            pause: AtomicBool::new(false),
        });
        let (question_sender, questions) = mpsc::channel();
        let (replies, reply_receiver) = mpsc::channel();
        let mut worker = Worker {
            shared: Handle(Arc::clone(&shared)),
            questions: question_sender,
            replies: reply_receiver,
        };
        let thread = thread::Builder::new()
            .name("file operation".to_string())
            .stack_size(WORKER_STACK)
            // A provider's calls without a stop of their own (a plugin's files) hear the job's (R14).
            .spawn(move || {
                let stop = worker.cancel_flag();
                crate::vfs::stop::stopped_by(stop, || work(&mut worker))
            });
        // No thread: the job ends at once with the reason.
        let (worker, ended) = match thread {
            Ok(thread) => (Some(thread), None),
            Err(error) => (
                None,
                Some(Report {
                    failures: vec![format!("the operation could not start: {error}")],
                    cancelled: true,
                    ..Report::default()
                }),
            ),
        };
        Self {
            shared: Handle(shared),
            questions,
            replies: Some(replies),
            pending: None,
            worker,
            ended,
        }
    }
}

impl<Q, R, P> Job<Q, R, P> {
    /// The progress and cancel flag for a helper thread.
    pub fn handle(&self) -> Handle<P> {
        Handle(Arc::clone(&self.shared.0))
    }

    pub fn progress(&self) -> P
    where
        P: Clone,
    {
        self.shared.progress().clone()
    }

    /// Whether a question picked up by [`question`](Self::question) waits for its answer.
    pub fn has_question(&self) -> bool {
        self.pending.is_some()
    }

    /// The question the worker waits on, if it asked one; none once the job is cancelled (one asked just
    /// before the stop has nobody to answer it: its worker gets the stop's answer, review R26).
    pub fn question(&mut self) -> Option<&Q> {
        if self.shared.cancelled() {
            return None;
        }
        if self.pending.is_none() {
            self.pending = self.questions.try_recv().ok();
        }
        self.pending.as_ref()
    }

    pub fn answer(&mut self, reply: R) {
        if self.pending.take().is_some()
            && let Some(replies) = &self.replies
        {
            // A worker that is gone needs no answer.
            let _ = replies.send(reply);
        }
    }

    /// Holds the worker at its next step (FAR: while "Operation has been interrupted" is asked).
    pub fn pause(&self, paused: bool) {
        self.shared.0.pause.store(paused, Ordering::Relaxed);
    }

    /// Cancelled and left to end by itself: nothing waits for its thread any more (the program ends
    /// with it still stuck, review R12).
    pub fn abandon(&mut self) {
        self.cancel();
        self.worker = None;
    }

    /// Stops the operation at its next step.
    pub fn cancel(&mut self) {
        self.shared.0.cancel.store(true, Ordering::Relaxed);
        self.pause(false);
        self.pending = None;
        self.replies = None;
    }

    /// How it ended, once the worker is done.
    pub fn finished(&mut self) -> Option<Report> {
        if let Some(report) = self.ended.take() {
            return Some(report);
        }
        if !self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            return None;
        }
        let worker = self.worker.take()?;
        Some(worker.join().unwrap_or_else(|_| Report {
            failures: vec!["the operation stopped on an internal error".to_string()],
            cancelled: true,
            ..Report::default()
        }))
    }
}

impl<Q, R, P> Drop for Job<Q, R, P> {
    /// The job is done with (the program is closing): it is cancelled and waited for, so what it was
    /// doing ends cleanly before the program exits - at most [`DROP_WAIT`] (ADR-0019: a worker that does
    /// not come back holds nothing; it ends with the process).
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let deadline = std::time::Instant::now() + DROP_WAIT;
            while !worker.is_finished() && std::time::Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

/// The worker's side of the channels.
pub struct Worker<Q, R, P> {
    shared: Handle<P>,
    questions: Sender<Q>,
    replies: Receiver<R>,
}

impl<Q, R, P> Worker<Q, R, P> {
    /// Asks the UI and waits; `cancel` is the reply when the job is cancelled meanwhile.
    pub fn ask(&mut self, question: Q, cancel: R) -> R {
        if self.shared.cancelled() || self.questions.send(question).is_err() {
            return cancel;
        }
        self.replies.recv().unwrap_or(cancel)
    }

    /// Whether the user stopped the operation; waits while it is paused.
    pub fn cancelled(&mut self) -> bool {
        while self.shared.0.pause.load(Ordering::Relaxed) && !self.shared.cancelled() {
            thread::sleep(PAUSE_POLL);
        }
        self.shared.cancelled()
    }

    pub fn progress(&mut self, change: &mut dyn FnMut(&mut P)) {
        change(&mut self.shared.progress());
    }

    /// The job's cancel flag, for what it lists.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.shared.cancel_flag()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    type TestJob = Job<&'static str, bool, u32>;

    #[test]
    fn a_question_asked_as_the_job_is_cancelled_never_comes_up() {
        let (asking, asked) = mpsc::channel();
        let (go, wait) = mpsc::channel::<()>();
        let mut job: TestJob = Job::spawn(move |worker| {
            let _ = wait.recv();
            // The stop came after its last look: the question is sent all the same.
            let _ = worker.questions.send("overwrite?");
            let _ = asking.send(worker.replies.recv().unwrap_or(false));
            Report::default()
        });
        job.cancel();
        go.send(()).unwrap();
        let reply = asked.recv().unwrap();
        assert!(!reply, "the worker gets the stop's answer");
        assert_eq!(job.question(), None, "nobody is asked");
        assert!(!job.has_question());
    }

    #[test]
    fn a_job_that_does_not_stop_is_not_waited_for_long_at_the_end() {
        // Review R12: a worker stuck (a dead share, a plugin) does not hold the program's end.
        let (release, stuck) = mpsc::channel::<()>();
        let job: TestJob = Job::spawn(move |_| {
            let _ = stuck.recv();
            Report::default()
        });
        let started = Instant::now();
        drop(job);
        assert!(started.elapsed() < DROP_WAIT + Duration::from_secs(1));
        assert!(started.elapsed() >= DROP_WAIT, "it was waited for a while");
        let _ = release.send(());
    }

    #[test]
    fn what_the_job_lists_stops_with_it() {
        // Review R15: the flag handed to a listing is the job's own cancel.
        let mut job: TestJob = Job::spawn(|worker| {
            let stop = worker.cancel_flag();
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(1));
            }
            Report::default()
        });
        assert!(!job.handle().cancel_flag().load(Ordering::Relaxed));
        job.cancel();
        let report = wait_for(&mut job);
        assert_eq!(report, Report::default(), "it ended on the cancel");
    }

    fn wait_for(job: &mut TestJob) -> Report {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(report) = job.finished() {
                return report;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("the job did not end");
    }

    #[test]
    fn a_question_waits_for_its_answer_and_the_progress_is_seen() {
        let mut job = TestJob::spawn(|worker| {
            worker.progress(&mut |done| *done = 1);
            let yes = worker.ask("go on?", false);
            Report {
                failures: if yes { vec![] } else { vec!["no".into()] },
                cancelled: false,
                ..Report::default()
            }
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while job.question().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(job.question(), Some(&"go on?"));
        assert_eq!(job.progress(), 1);
        job.answer(true);
        assert_eq!(wait_for(&mut job), Report::default());
    }

    #[test]
    fn a_cancel_answers_a_waiting_question_with_the_cancel_reply() {
        let mut job = TestJob::spawn(|worker| {
            let yes = worker.ask("go on?", false);
            Report {
                failures: vec![],
                cancelled: !yes && worker.cancelled(),
                ..Report::default()
            }
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while job.question().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        job.cancel();
        assert!(wait_for(&mut job).cancelled);
    }

    #[test]
    fn a_paused_worker_waits_until_it_is_resumed() {
        let mut job = TestJob::spawn(|worker| {
            while !worker.cancelled() {
                worker.progress(&mut |done| *done += 1);
                thread::sleep(Duration::from_millis(1));
            }
            Report::default()
        });
        job.pause(true);
        // Paused, the count stops moving: two looks in a row see the same.
        let deadline = Instant::now() + Duration::from_secs(10);
        let held = loop {
            let before = job.progress();
            thread::sleep(Duration::from_millis(60));
            if job.progress() == before {
                break before;
            }
            assert!(Instant::now() < deadline, "the worker did not hold");
        };
        job.pause(false);
        while job.progress() <= held + 1 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(job.progress() > held + 1, "steps again once resumed");
        job.cancel();
        wait_for(&mut job);
    }
}
