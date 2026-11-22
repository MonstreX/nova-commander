//! The plugin's side of a long library call reached on the plugin's own thread only. The library calls
//! back from threads of its own (a 7z folder of more than one coder runs them on its mixer's threads,
//! the program's host answers the plugin's thread only), so the call runs on a thread the SDK starts
//! while the caller's thread serves each callback's job on the plugin's sink, one at a time, the
//! callback's thread waiting for it. A job that panics ends the serving: every job after it is refused
//! and the panic goes on from `serve` once the call has ended (the SDK's boundary tells it).

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::ptr::NonNull;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// A callback's job, erased: run once on the sink.
type Erased<S> = NonNull<dyn FnMut(&mut S) + Send + 'static>;

/// A job waiting for the caller's thread.
struct Job<S: ?Sized>(Erased<S>);

// SAFETY: the job is a `Send` closure; the pointer is followed on the caller's thread only while the
// callback that made it waits (`Marshal::ask`), so the closure lives.
unsafe impl<S: ?Sized> Send for Job<S> {}

struct State<S: ?Sized> {
    job: Option<Job<S>>,
    /// The job given was run (or found the call over).
    answered: bool,
    /// The caller's thread serves jobs (`Marshal::serve` runs).
    serving: bool,
    /// The call on the SDK's thread has ended.
    ended: bool,
    /// The first job's panic, carried on by `serve`.
    panic: Option<Box<dyn Any + Send>>,
}

/// Jobs of the library's threads for the caller's.
pub(super) struct Marshal<S: ?Sized> {
    state: Mutex<State<S>>,
    changed: Condvar,
    /// One callback's job at a time.
    turn: Mutex<()>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<S: ?Sized> Marshal<S> {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                job: None,
                answered: false,
                serving: false,
                ended: false,
                panic: None,
            }),
            changed: Condvar::new(),
            turn: Mutex::new(()),
        }
    }

    /// On any thread: `job` run with the sink on the caller's thread, its answer; none when no call is
    /// served (it is over) or a job panicked.
    pub fn ask<R: Send>(&self, job: impl FnOnce(&mut S) -> R + Send) -> Option<R> {
        let _turn = locked(&self.turn);
        let mut job = Some(job);
        let mut answer = None;
        let mut run = |sink: &mut S| {
            if let Some(job) = job.take() {
                answer = Some(job(sink));
            }
        };
        self.hand_over(&mut run);
        answer
    }

    /// `run` given to the caller's thread and waited for; the pointer to it taken back before this
    /// returns, whether it ran or serving ended first.
    fn hand_over(&self, run: &mut (dyn FnMut(&mut S) + Send + '_)) {
        let short = NonNull::from(run);
        // SAFETY: only the lifetime is erased; the pointer is taken back below, inside `run`'s borrow.
        let erased: Erased<S> = unsafe { std::mem::transmute(short) };
        let mut state = locked(&self.state);
        if !state.serving || state.panic.is_some() {
            return;
        }
        state.job = Some(Job(erased));
        state.answered = false;
        self.changed.notify_all();
        while !state.answered && state.serving {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.job = None;
        state.answered = false;
    }

    /// On the caller's thread: `call` run on a thread of its own, every job asked meanwhile run with
    /// `sink` here; `call`'s answer (its panic carried on).
    pub fn serve<T: Send>(&self, sink: &mut S, call: impl FnOnce() -> T + Send) -> T {
        {
            let mut state = locked(&self.state);
            state.serving = true;
            state.ended = false;
            state.panic = None;
        }
        let result = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                /// Tells the caller's thread the call ended, whichever way it ends.
                struct Ended<'a, S: ?Sized>(&'a Marshal<S>);
                impl<S: ?Sized> Drop for Ended<'_, S> {
                    fn drop(&mut self) {
                        locked(&self.0.state).ended = true;
                        self.0.changed.notify_all();
                    }
                }
                let _ended = Ended(self);
                call()
            });
            self.run_jobs(sink);
            worker.join()
        });
        let panicked = {
            let mut state = locked(&self.state);
            state.serving = false;
            self.changed.notify_all();
            state.panic.take()
        };
        if let Some(panic) = panicked {
            resume_unwind(panic);
        }
        result.unwrap_or_else(|panic| resume_unwind(panic))
    }

    /// The jobs run as they come until the call ends.
    fn run_jobs(&self, sink: &mut S) {
        let mut state = locked(&self.state);
        loop {
            if let Some(Job(mut job)) = state.job.take() {
                drop(state);
                // SAFETY: the job's callback waits for its answer, so the closure lives; it is run
                // once, here, the only reference to it now.
                let ran = catch_unwind(AssertUnwindSafe(|| unsafe { job.as_mut()(sink) }));
                state = locked(&self.state);
                if let Err(panic) = ran {
                    state.panic.get_or_insert(panic);
                }
                state.answered = true;
                self.changed.notify_all();
            } else if state.ended {
                return;
            } else {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
    }
}

#[cfg(test)]
#[path = "sevenzip_marshal_tests.rs"]
mod tests;
