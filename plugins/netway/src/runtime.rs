//! The plugin's own runtime (ADR-0023 §2): the plugin's calls come on its thread (ADR-0016) and wait there
//! for the network's work, which runs on the runtime's two threads; the user's stop (`Host::cancelled`, Esc)
//! is asked while a call waits and drops the work. Between calls the runtime keeps the connections alive.

use std::future::Future;
use std::time::Duration;

/// How often a wait asks whether the user stopped it.
const STOP_POLL: Duration = Duration::from_millis(50);

/// The work was dropped because the user stopped it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped;

/// The runtime of the plugin's network work.
pub struct Runtime {
    inner: tokio::runtime::Runtime,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Runtime")
    }
}

impl Runtime {
    /// A runtime of two threads: one for a transfer, one for the rest (keep-alive, a listing).
    pub fn new() -> std::io::Result<Self> {
        let inner = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("netway")
            .enable_all()
            .build()?;
        Ok(Self { inner })
    }

    /// `work` waited for on the calling thread; `stopped` asked every 50 ms, the work dropped as soon as it
    /// says so.
    pub fn wait<T>(
        &self,
        work: impl Future<Output = T>,
        stopped: &dyn Fn() -> bool,
    ) -> Result<T, Stopped> {
        self.inner.block_on(async {
            let mut work = std::pin::pin!(work);
            let mut tick = tokio::time::interval(STOP_POLL);
            loop {
                tokio::select! {
                    done = &mut work => return Ok(done),
                    _ = tick.tick() => {
                        if stopped() {
                            return Err(Stopped);
                        }
                    }
                }
            }
        })
    }

    /// `work` waited for as [`Runtime::wait`] does, its `questions` answered meanwhile on the calling
    /// thread by `answer` (the program's windows are shown from there).
    pub fn wait_answering<T>(
        &self,
        work: impl Future<Output = T>,
        stopped: &dyn Fn() -> bool,
        questions: &mut tokio::sync::mpsc::UnboundedReceiver<crate::ask::Asked>,
        answer: &mut dyn FnMut(crate::ask::Question) -> crate::ask::Reply,
    ) -> Result<T, Stopped> {
        self.inner.block_on(async {
            let mut work = std::pin::pin!(work);
            let mut tick = tokio::time::interval(STOP_POLL);
            loop {
                tokio::select! {
                    done = &mut work => return Ok(done),
                    Some((question, reply)) = questions.recv() => {
                        let _ = reply.send(answer(question));
                    }
                    _ = tick.tick() => {
                        if stopped() {
                            return Err(Stopped);
                        }
                    }
                }
            }
        })
    }

    /// `work` run on the runtime's threads, the caller not waiting (a connection's background work).
    pub fn spawn<F>(&self, work: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(work)
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
