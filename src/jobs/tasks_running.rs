//! What a task is to do and its job once it runs: a copy or move, a deletion, a change of attributes.

use super::super::transfer::{Job, Report, Request};
use super::super::tree_cache::TreeCache;
use super::super::{attributes, delete};
use crate::vfs::VfsPath;

/// What a task is to do.
pub(super) enum Order {
    Transfer(Request),
    /// With the count the question started, if any.
    Delete(delete::Request, Option<delete::Counter>),
    Attributes(Box<attributes::Request>),
}

/// A task's running job.
pub(super) enum Running {
    Transfer(Box<Job>),
    Delete(Box<delete::Job>),
    Attributes(Box<attributes::Job>),
}

impl Running {
    /// The job of `order`, keeping the trees' files up when there are any.
    pub(super) fn start(order: Order, tree_cache: Option<TreeCache>) -> Self {
        match order {
            Order::Transfer(request) => Self::Transfer(Box::new(Job::start(request, tree_cache))),
            Order::Delete(request, counter) => {
                Self::Delete(Box::new(delete::Job::start(request, counter, tree_cache)))
            }
            Order::Attributes(request) => {
                Self::Attributes(Box::new(attributes::Job::start(*request)))
            }
        }
    }

    pub(super) fn has_question(&self) -> bool {
        match self {
            Self::Transfer(job) => job.has_question(),
            Self::Delete(job) => job.has_question(),
            Self::Attributes(job) => job.has_question(),
        }
    }

    /// Picks up a question the worker asked.
    pub(super) fn poll_question(&mut self) {
        match self {
            Self::Transfer(job) => {
                job.question();
            }
            Self::Delete(job) => {
                job.question();
            }
            Self::Attributes(job) => {
                job.question();
            }
        }
    }

    pub(super) fn pause(&self, paused: bool) {
        match self {
            Self::Transfer(job) => job.pause(paused),
            Self::Delete(job) => job.pause(paused),
            Self::Attributes(job) => job.pause(paused),
        }
    }

    pub(super) fn cancel(&mut self) {
        match self {
            Self::Transfer(job) => job.cancel(),
            Self::Delete(job) => job.cancel(),
            Self::Attributes(job) => job.cancel(),
        }
    }

    /// Cancelled, its thread not waited for any more.
    pub(super) fn abandon(&mut self) {
        match self {
            Self::Transfer(job) => job.abandon(),
            Self::Delete(job) => job.abandon(),
            Self::Attributes(job) => job.abandon(),
        }
    }

    pub(super) fn finished(&mut self) -> Option<Report> {
        match self {
            Self::Transfer(job) => job.finished(),
            Self::Delete(job) => job.finished(),
            Self::Attributes(job) => job.finished(),
        }
    }

    /// The selected objects it finished since the last look.
    pub(super) fn take_finished_objects(&self) -> Vec<VfsPath> {
        match self {
            Self::Transfer(job) => std::mem::take(&mut job.handle().progress().finished),
            Self::Delete(job) => std::mem::take(&mut job.handle().progress().finished),
            Self::Attributes(job) => std::mem::take(&mut job.handle().progress().finished),
        }
    }
}

impl super::TaskList {
    /// Stops the copies and moves to or from the provider `provider` (a plugin let go): its panels' names
    /// would reach a new instance of it - another connection's - and a move remove there (R7 review B7).
    pub fn cancel_on_provider(&mut self, provider: &str) {
        let on = |path: &VfsPath| {
            path.provider()
                .is_some_and(|id| id.eq_ignore_ascii_case(provider))
        };
        let ids: Vec<super::TaskId> = (self.tasks.iter())
            .filter(|task| {
                (task.request.as_ref())
                    .is_some_and(|request| on(&request.target) || request.sources.iter().any(on))
            })
            .map(|task| task.id)
            .collect();
        for id in ids {
            self.cancel(id);
        }
    }
}
