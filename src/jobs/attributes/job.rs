//! "Set" on its own worker thread, as the UI holds it.

use super::{Host, Progress, Question, Reply, Request};
use crate::jobs::work::{Job as Work, Worker};

/// A running "Set": progress to read, questions to answer, pause and cancel.
pub type Job = Work<Question, Reply, Progress>;

impl Work<Question, Reply, Progress> {
    pub fn start(request: Request) -> Self {
        Self::spawn(move |worker| super::run(&request, worker))
    }
}

impl Host for Worker<Question, Reply, Progress> {
    fn ask(&mut self, question: Question) -> Reply {
        Worker::ask(self, question, Reply::Cancel)
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
