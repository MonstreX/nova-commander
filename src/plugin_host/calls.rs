//! The calls sent to one plugin's thread (ADR-0019 §5): each gets a number as it is sent, so that the
//! program can ask that very call to stop (the plugin hears it through `cancelled()`) and read the
//! progress it tells (`progress()`) - never another call's, sent before or after it.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::worker::Job;

/// How far a plugin's call is, as it tells it (FAR's progress in a plugin's own message).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallProgress {
    pub done: u64,
    /// 0: not known.
    pub total: u64,
    pub text: String,
}

/// What the program and the plugin's thread share of the call running: its number, the number asked
/// to stop, the progress told. Number 0 is no call.
#[derive(Debug, Default)]
pub struct CallState {
    running: AtomicU64,
    /// The calls asked to stop that have not ended (one asked before it began hears it as it begins).
    stopped: Mutex<Vec<u64>>,
    progress: Mutex<Option<CallProgress>>,
    /// The program let this plugin go (unloaded, or left hanging for a new one, P6.0e): its host functions
    /// answer nothing from now on and the call running is asked to stop.
    let_go: AtomicBool,
}

impl CallState {
    fn progress_now(&self) -> MutexGuard<'_, Option<CallProgress>> {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn stopped_now(&self) -> MutexGuard<'_, Vec<u64>> {
        self.stopped.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The plugin's thread starts the call `id`: nothing told of it yet; the stops of calls before it go.
    pub(super) fn begin(&self, id: u64) {
        *self.progress_now() = None;
        self.stopped_now().retain(|stopped| *stopped >= id);
        self.running.store(id, Ordering::Release);
    }

    /// The call running has ended: nothing runs until the next.
    pub(super) fn end(&self) {
        self.running.store(0, Ordering::Release);
    }

    /// The call `id` is asked to stop: the running one hears it at once, one queued as it begins, one
    /// ended never.
    pub fn stop(&self, id: u64) {
        let mut stopped = self.stopped_now();
        if id != 0 && !stopped.contains(&id) {
            stopped.push(id);
        }
    }

    /// The number of the call running now (0: none).
    pub fn running(&self) -> u64 {
        self.running.load(Ordering::Acquire)
    }

    /// Whether the call running was asked to stop (the plugin's `cancelled()`): always once let go.
    pub fn cancelled(&self) -> bool {
        let running = self.running.load(Ordering::Acquire);
        self.is_let_go() || (running != 0 && self.stopped_now().contains(&running))
    }

    /// The stop asked of the call running forgotten: it is not cancelled until it is asked again.
    pub fn resume(&self) {
        let running = self.running.load(Ordering::Acquire);
        self.stopped_now().retain(|stopped| *stopped != running);
    }

    /// The program lets the plugin go: nothing it asks is heard any more, its call is asked to stop.
    pub fn let_go(&self) {
        self.let_go.store(true, Ordering::Release);
    }

    pub fn is_let_go(&self) -> bool {
        self.let_go.load(Ordering::Acquire)
    }

    /// The running call tells how far it is.
    pub fn tell(&self, progress: CallProgress) {
        *self.progress_now() = Some(progress);
    }

    /// What the call `id` told of its progress, while it runs.
    pub fn progress(&self, id: u64) -> Option<CallProgress> {
        if self.running.load(Ordering::Acquire) != id {
            return None;
        }
        self.progress_now().clone()
    }
}

/// A job with its call's number.
pub(super) struct Call {
    pub id: u64,
    pub job: Job,
}

/// The line to a plugin's thread, shared by its worker, its provider and its open files: `None` once the
/// plugin ends.
#[derive(Debug, Default)]
pub(super) struct Calls {
    sender: Mutex<Option<Sender<Call>>>,
    next: AtomicU64,
    pub state: Arc<CallState>,
}

impl Calls {
    pub(super) fn new(sender: Sender<Call>, state: Arc<CallState>) -> Self {
        Self {
            sender: Mutex::new(Some(sender)),
            next: AtomicU64::new(0),
            state,
        }
    }

    /// `job` sent to the plugin's thread: its call's number; none when the plugin is gone.
    pub(super) fn send(&self, job: Job) -> Option<u64> {
        let sender = self.sender.lock().unwrap_or_else(PoisonError::into_inner);
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        sender.as_ref()?.send(Call { id, job }).ok()?;
        Some(id)
    }

    /// The plugin ends: nothing more is sent (its thread ends once it has nothing left to do).
    pub(super) fn close(&self) {
        self.sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_call_asked_to_stop_hears_it() {
        let state = CallState::default();
        assert!(!state.cancelled(), "no call runs");
        state.begin(1);
        state.stop(2);
        assert!(!state.cancelled(), "another call was asked");
        state.stop(1);
        assert!(state.cancelled(), "two asked: each hears its own");
        state.begin(2);
        assert!(
            state.cancelled(),
            "the queued call asked before it began hears it"
        );
        state.begin(3);
        assert!(!state.cancelled(), "the next call starts afresh");
        state.stop(0);
        assert!(!state.cancelled(), "no call is no stop");
    }

    #[test]
    fn a_stop_taken_back_is_heard_again_only_when_asked_again() {
        let state = CallState::default();
        state.begin(4);
        state.stop(5);
        state.stop(4);
        state.resume();
        assert!(!state.cancelled(), "No to the plugin's question");
        state.stop(4);
        assert!(state.cancelled(), "the next Esc");
        state.begin(5);
        assert!(state.cancelled(), "another call's stop kept");
    }

    #[test]
    fn a_resume_with_no_call_running_or_once_let_go_changes_nothing() {
        let state = CallState::default();
        state.stop(7);
        state.resume();
        state.begin(7);
        assert!(state.cancelled(), "a queued call's stop kept");
        state.resume();
        assert!(!state.cancelled());
        state.let_go();
        state.resume();
        assert!(state.cancelled(), "let go: always stopped");
    }

    #[test]
    fn a_calls_progress_is_its_own_and_goes_with_it() {
        let state = CallState::default();
        state.begin(5);
        let told = CallProgress {
            done: 3,
            total: 10,
            text: "a.txt".to_string(),
        };
        state.tell(told.clone());
        assert_eq!(state.progress(5), Some(told));
        assert_eq!(state.progress(4), None, "not the call running");
        state.begin(6);
        assert_eq!(state.progress(6), None, "nothing told yet");
    }

    #[test]
    fn calls_are_numbered_as_sent_and_none_once_closed() {
        let (sender, received) = std::sync::mpsc::channel();
        let calls = Calls::new(sender, Arc::default());
        assert_eq!(calls.send(Job::Close("a".to_string())), Some(1));
        assert_eq!(calls.send(Job::Close("b".to_string())), Some(2));
        assert_eq!(
            received.try_iter().map(|call| call.id).collect::<Vec<_>>(),
            [1, 2]
        );
        calls.close();
        assert_eq!(calls.send(Job::Close("c".to_string())), None);
    }
}
