//! The stop of the work running on this thread, for a provider's calls that take none of their own (a
//! plugin's file calls: `stat`, `make_dir`, a file read or written, review R14). A job or a wait that can
//! be stopped runs its work under [`stopped_by`]; a provider that waits on another thread (a plugin's)
//! looks at [`current`] and gives up - telling the plugin - once it is set.

use std::cell::RefCell;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

thread_local! {
    static STOP: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// `work` run with `stop` as this thread's stop (the one before it back afterwards).
pub fn stopped_by<T>(stop: Arc<AtomicBool>, work: impl FnOnce() -> T) -> T {
    let before = STOP.with(|current| current.borrow_mut().replace(stop));
    /// Puts the stop before back however the work ends.
    struct Restore(Option<Arc<AtomicBool>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let before = self.0.take();
            STOP.with(|current| *current.borrow_mut() = before);
        }
    }
    let _restore = Restore(before);
    work()
}

/// What a call its work's stop broke fails with (review of P2).
#[derive(Debug)]
struct Stopped;

impl std::fmt::Display for Stopped {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("The operation was stopped")
    }
}

impl std::error::Error for Stopped {}

/// The failure of a call its work's stop broke: never `Interrupted`, which `std`'s readers and writers
/// (`write_all`, `read_to_end`) and the copy's loop take for "try again".
pub fn stopped() -> io::Error {
    io::Error::other(Stopped)
}

/// Whether `error` is a call its work's stop broke.
pub fn is_stopped(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<Stopped>())
}

/// `work` that cleans up after a stop (an unfinished file removed): run under a stop of its own, never
/// set, so that the stop that came does not break it too.
pub fn unstopped<T>(work: impl FnOnce() -> T) -> T {
    stopped_by(Arc::default(), work)
}

/// This thread's stop: a fresh one never set when its work has none.
pub fn current() -> Arc<AtomicBool> {
    STOP.with(|current| current.borrow().clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn work_sees_its_stop_and_the_one_before_comes_back() {
        assert!(!current().load(Ordering::Relaxed), "none: never set");
        let outer = Arc::new(AtomicBool::new(false));
        let inner = Arc::new(AtomicBool::new(true));
        stopped_by(Arc::clone(&outer), || {
            assert!(Arc::ptr_eq(&current(), &outer));
            stopped_by(Arc::clone(&inner), || {
                assert!(current().load(Ordering::Relaxed))
            });
            assert!(Arc::ptr_eq(&current(), &outer), "back after the inner work");
        });
        assert!(!Arc::ptr_eq(&current(), &outer), "gone after the work");
    }

    #[test]
    fn a_stopped_call_is_told_apart_and_never_taken_for_try_again() {
        let error = stopped();
        assert!(is_stopped(&error));
        assert_ne!(error.kind(), io::ErrorKind::Interrupted);
        assert!(!is_stopped(&io::ErrorKind::Interrupted.into()));
        let set = Arc::new(AtomicBool::new(true));
        stopped_by(set, || {
            assert!(current().load(Ordering::Relaxed));
            unstopped(|| {
                assert!(
                    !current().load(Ordering::Relaxed),
                    "the cleanup is not stopped"
                )
            });
        });
    }
}
