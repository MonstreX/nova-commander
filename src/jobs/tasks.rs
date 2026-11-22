//! The background tasks (ADR-0010): copies, moves, deletions and changes of attributes that run, wait in the queue, wait for an
//! answer or have ended. A task whose source or target volume is in use by an earlier task waits for it (writing
//! twice to one disk only slows both); tasks on other volumes run at once. A task that ended cleanly
//! leaves the list after a while; one with failures or cancelled stays until it is removed.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use self::running::{Order, Running};
use super::task_clock::TaskClock;
use super::transfer::{Progress, Question, Reply, Report, Request};
use super::{attributes, delete};
use crate::platform::volumes::VolumeId;
use crate::vfs::VfsPath;

#[path = "tasks_running.rs"]
mod running;

pub type TaskId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Copy,
    Move,
    /// Alt+F6.
    Link,
    Delete,
    /// Ctrl+A's "Set".
    Attributes,
}

impl Kind {
    /// The operation's name: the report's title, the Screens list's line.
    pub fn title(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Link => "Link",
            Self::Delete => "Delete",
            Self::Attributes => "Attributes",
        }
    }
}

/// Where a task is, as the status row and the Screens list show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Queued,
    Running,
    /// Paused by the user.
    Paused,
    /// A question waits for the user.
    Waiting,
    Done,
    Failed,
    Cancelled,
}

enum State {
    /// Taken out when the task starts.
    Queued(Option<Order>),
    Running(Running),
    Ended {
        report: Report,
        at: Instant,
    },
}

pub struct Task {
    pub id: TaskId,
    pub kind: Kind,
    pub target: PathBuf,
    volumes: Vec<VolumeId>,
    state: State,
    /// Paused by the user ("Pause" in the progress dialog).
    paused: bool,
    /// Held while the "Operation has been interrupted" question is up.
    held: bool,
    /// Cancelled with its operation: it leaves the list as soon as it has ended.
    dismissed: bool,
    /// The time it worked, for its progress dialog.
    clock: TaskClock,
    /// What was asked for, for "Retry skipped"; a deletion has no such thing.
    request: Option<Request>,
    /// The last progress, kept when the job ends.
    last: Progress,
    last_deletion: delete::Progress,
    last_attributes: attributes::Progress,
}

impl Task {
    pub fn status(&self) -> Status {
        match &self.state {
            State::Queued(_) => Status::Queued,
            State::Running(job) if job.has_question() => Status::Waiting,
            State::Running(_) if self.paused => Status::Paused,
            State::Running(_) => Status::Running,
            State::Ended { report, .. } if report.cancelled => Status::Cancelled,
            State::Ended { report, .. } if !report.failures.is_empty() => Status::Failed,
            State::Ended { .. } => Status::Done,
        }
    }

    /// How far a copy or move is.
    pub fn progress(&self) -> Progress {
        match &self.state {
            State::Running(Running::Transfer(job)) => job.progress(),
            _ => self.last.clone(),
        }
    }

    /// How far a deletion is.
    pub fn deletion(&self) -> delete::Progress {
        match &self.state {
            State::Running(Running::Delete(job)) => job.progress(),
            _ => self.last_deletion.clone(),
        }
    }

    /// How far a change of attributes is.
    pub fn attributes(&self) -> attributes::Progress {
        match &self.state {
            State::Running(Running::Attributes(job)) => job.progress(),
            _ => self.last_attributes.clone(),
        }
    }

    /// The time the task worked (FAR's `m_CalcTime`): since it started, the time it waited for the user
    /// or was paused left out; zero while it is queued.
    pub fn elapsed(&self) -> Duration {
        self.clock.worked(Instant::now())
    }

    /// A task for what this one left out after errors, with its settings; `None` when nothing was.
    pub fn retry_request(&self) -> Option<Request> {
        let skipped = &self.report()?.skipped;
        if skipped.is_empty() {
            return None;
        }
        let request = self.request.as_ref()?;
        Some(Request {
            sources: skipped.iter().map(|item| item.source.clone()).collect(),
            retrying: skipped.clone(),
            ..request.clone()
        })
    }

    pub fn report(&self) -> Option<&Report> {
        match &self.state {
            State::Ended { report, .. } => Some(report),
            _ => None,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Whether its job ever started (a task cancelled in the queue touched nothing).
    pub fn has_started(&self) -> bool {
        self.clock.has_started()
    }

    /// Queued or running (also waiting for an answer).
    pub fn is_active(&self) -> bool {
        !matches!(self.state, State::Ended { .. })
    }

    fn take_finished_objects(&mut self) -> Vec<VfsPath> {
        match &self.state {
            State::Running(job) => job.take_finished_objects(),
            _ => {
                let mut objects = std::mem::take(&mut self.last.finished);
                objects.append(&mut self.last_deletion.finished);
                objects.append(&mut self.last_attributes.finished);
                objects
            }
        }
    }

    fn apply_pause(&mut self) {
        if let State::Running(job) = &self.state {
            job.pause(self.paused || self.held);
        }
        self.update_clock(Instant::now());
    }

    /// Its clock stands while it waits for an answer, is paused or held, and once it has ended.
    fn update_clock(&mut self, now: Instant) {
        let standing = match &self.state {
            State::Running(job) => self.paused || self.held || job.has_question(),
            _ => true,
        };
        self.clock.stand(standing, now);
    }
}

pub struct TaskList {
    tasks: Vec<Task>,
    next_id: TaskId,
    /// How long a task that ended cleanly stays in the list.
    pub keep_done: Duration,
    /// Queued tasks cancelled since the last pump: they ended without a job.
    cancelled_queued: Vec<TaskId>,
    /// What tasks that left the list had finished and nobody read yet.
    unread_objects: Vec<(TaskId, VfsPath)>,
    /// The trees' files the operations keep up (FAR's `TreeList::AddTreeName` and the others).
    pub tree_cache: Option<super::tree_cache::TreeCache>,
    /// When "Cancel all" was given: tasks still running long after it do not answer (review R12).
    cancelled_at: Option<std::time::Instant>,
}

/// How long tasks may take to stop after "Cancel all" before they count as not answering.
const STOP_WAIT: Duration = Duration::from_secs(5);

impl TaskList {
    pub fn new(keep_done: Duration) -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
            keep_done,
            cancelled_queued: Vec::new(),
            unread_objects: Vec::new(),
            tree_cache: None,
            cancelled_at: None,
        }
    }

    /// Tasks still running [`STOP_WAIT`] after "Cancel all": a disk or a plugin does not answer. Once
    /// none runs, "Cancel all" is over.
    pub fn not_stopping(&mut self) -> bool {
        if self.active() == 0 {
            self.cancelled_at = None;
        }
        self.cancelled_at
            .is_some_and(|at| at.elapsed() >= STOP_WAIT)
    }

    /// "Wait" once more: they get another [`STOP_WAIT`].
    pub fn wait_for_stop(&mut self) {
        self.cancelled_at = Some(std::time::Instant::now());
    }

    /// No quitting waits on the cancelled tasks any more (Back): they are let be.
    pub fn forget_stop(&mut self) {
        self.cancelled_at = None;
    }

    /// "Quit anyway": every task cancelled at once, none waited for at the end (one stuck on a dead share
    /// or a plugin ends with the process).
    pub fn abandon_all(&mut self) {
        for task in &mut self.tasks {
            if let State::Running(job) = &mut task.state {
                job.abandon();
            }
        }
    }

    /// "Cancel all" given long ago (tests).
    #[cfg(test)]
    pub fn cancelled_long_ago(&mut self) {
        self.cancelled_at = std::time::Instant::now().checked_sub(STOP_WAIT);
    }

    /// Adds a copy or move; it starts at the next [`pump`](Self::pump) unless its volumes are busy.
    #[cfg(test)]
    pub fn add(&mut self, request: Request) -> TaskId {
        let volumes = super::task_volumes::of_transfer(&request);
        self.add_on(request, volumes)
    }

    /// Adds a deletion; it waits for the tasks before it on `volumes`, those of the folders it deletes in
    /// (`task_volumes::of_delete`, worked out off the screen's thread). `counter`: the count its question
    /// started, to go on with.
    pub fn add_delete_on(
        &mut self,
        request: delete::Request,
        counter: Option<delete::Counter>,
        volumes: Vec<VolumeId>,
    ) -> TaskId {
        let target = request
            .items
            .first()
            .and_then(|item| item.path.parent())
            .map(|folder| shown_path(&folder))
            .unwrap_or_default();
        self.push(
            Kind::Delete,
            target,
            Order::Delete(request, counter),
            volumes,
        )
    }

    /// Adds a change of attributes. It starts at once: it writes no file data, and one Ctrl+A on a disk
    /// a long copy is writing to should not wait for the copy (owner, 2026-11-08).
    pub fn add_attributes(&mut self, request: attributes::Request) -> TaskId {
        let target = (request.items.first())
            .map(|item| item.parent().unwrap_or(item).to_path_buf())
            .unwrap_or_default();
        self.push(
            Kind::Attributes,
            target,
            Order::Attributes(Box::new(request)),
            Vec::new(),
        )
    }

    /// [`add`](Self::add) on `volumes` worked out before (`task_volumes::of_transfer`).
    pub fn add_on(&mut self, request: Request, volumes: Vec<VolumeId>) -> TaskId {
        let kind = match (request.link, request.move_) {
            (Some(_), _) => Kind::Link,
            (None, true) => Kind::Move,
            (None, false) => Kind::Copy,
        };
        let target = shown_path(&request.target);
        let id = self.push(kind, target, Order::Transfer(request.clone()), volumes);
        if let Some(task) = self.tasks.last_mut() {
            task.request = Some(request);
        }
        id
    }

    fn push(
        &mut self,
        kind: Kind,
        target: PathBuf,
        order: Order,
        volumes: Vec<VolumeId>,
    ) -> TaskId {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task {
            id,
            kind,
            target,
            volumes,
            state: State::Queued(Some(order)),
            paused: false,
            held: false,
            dismissed: false,
            clock: TaskClock::default(),
            request: None,
            last: Progress::default(),
            last_deletion: delete::Progress::default(),
            last_attributes: attributes::Progress::default(),
        });
        id
    }

    /// Periodic work: ended jobs become reports, queued tasks whose volumes are free start, questions
    /// are picked up, tasks that ended cleanly long enough ago leave. Returns the tasks that ended now.
    pub fn pump(&mut self, now: Instant) -> Vec<TaskId> {
        let mut ended = std::mem::take(&mut self.cancelled_queued);
        for task in &mut self.tasks {
            if let State::Running(job) = &mut task.state {
                job.poll_question();
                if let Some(report) = job.finished() {
                    match job {
                        Running::Transfer(job) => task.last = job.progress(),
                        Running::Delete(job) => task.last_deletion = job.progress(),
                        Running::Attributes(job) => task.last_attributes = job.progress(),
                    }
                    task.state = State::Ended { report, at: now };
                    ended.push(task.id);
                }
            }
        }
        self.start_queued(now);
        for task in &mut self.tasks {
            task.update_clock(now);
            if task.dismissed && !task.is_active() {
                let objects = task.take_finished_objects();
                self.unread_objects
                    .extend(objects.into_iter().map(|path| (task.id, path)));
            }
        }
        let keep = self.keep_done;
        self.tasks.retain(|task| match &task.state {
            State::Ended { .. } if task.dismissed => false,
            State::Ended { report, at } => {
                let clean = report.failures.is_empty() && !report.cancelled;
                !(clean && now.saturating_duration_since(*at) >= keep)
            }
            _ => true,
        });
        ended
    }

    /// The selected objects finished since the last look, with their tasks.
    pub fn take_finished_objects(&mut self) -> Vec<(TaskId, VfsPath)> {
        let mut finished = std::mem::take(&mut self.unread_objects);
        for task in &mut self.tasks {
            let objects = task.take_finished_objects();
            finished.extend(objects.into_iter().map(|path| (task.id, path)));
        }
        finished
    }

    /// In list order: a queued task starts when none of its volumes is used by a task before it that
    /// runs or waits in the queue, so later tasks never overtake earlier ones on the same disk.
    fn start_queued(&mut self, now: Instant) {
        let mut busy: HashSet<VolumeId> = HashSet::new();
        let tree_cache = self.tree_cache.as_ref();
        for task in &mut self.tasks {
            let free = !task.volumes.iter().any(|volume| busy.contains(volume));
            match &mut task.state {
                State::Running(_) => {}
                State::Queued(order) if free => {
                    if let Some(order) = order.take() {
                        task.state = State::Running(Running::start(order, tree_cache.cloned()));
                        task.clock.start(now);
                        task.apply_pause();
                    }
                }
                State::Queued(_) => {}
                State::Ended { .. } => continue,
            }
            busy.extend(task.volumes.iter().cloned());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter()
    }

    pub fn get(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    fn get_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Tasks queued or running.
    pub fn active(&self) -> usize {
        self.tasks.iter().filter(|task| task.is_active()).count()
    }

    /// The question a copy or move waits on.
    pub fn question(&mut self, id: TaskId) -> Option<&Question> {
        match &mut self.get_mut(id)?.state {
            State::Running(Running::Transfer(job)) => job.question(),
            _ => None,
        }
    }

    /// The question a deletion waits on.
    pub fn delete_question(&mut self, id: TaskId) -> Option<&delete::Question> {
        match &mut self.get_mut(id)?.state {
            State::Running(Running::Delete(job)) => job.question(),
            _ => None,
        }
    }

    /// The question a change of attributes waits on.
    pub fn attributes_question(&mut self, id: TaskId) -> Option<&attributes::Question> {
        match &mut self.get_mut(id)?.state {
            State::Running(Running::Attributes(job)) => job.question(),
            _ => None,
        }
    }

    pub fn answer_attributes(&mut self, id: TaskId, reply: attributes::Reply) {
        if let Some(Task {
            state: State::Running(Running::Attributes(job)),
            ..
        }) = self.get_mut(id)
        {
            job.answer(reply);
        }
    }

    pub fn answer(&mut self, id: TaskId, reply: Reply) {
        if let Some(Task {
            state: State::Running(Running::Transfer(job)),
            ..
        }) = self.get_mut(id)
        {
            job.answer(reply);
        }
    }

    pub fn answer_delete(&mut self, id: TaskId, reply: delete::Reply) {
        if let Some(Task {
            state: State::Running(Running::Delete(job)),
            ..
        }) = self.get_mut(id)
        {
            job.answer(reply);
        }
    }

    /// "Pause" / "Continue" in the progress dialog.
    pub fn set_paused(&mut self, id: TaskId, paused: bool) {
        if let Some(task) = self.get_mut(id) {
            task.paused = paused;
            task.apply_pause();
        }
    }

    /// Holds the task the "Operation has been interrupted" question is up for, and releases every
    /// other (the question was answered, whatever happened to the task in front meanwhile).
    pub fn hold_only(&mut self, held: Option<TaskId>) {
        for task in &mut self.tasks {
            let hold = held == Some(task.id);
            if task.held != hold {
                task.held = hold;
                task.apply_pause();
            }
        }
    }

    /// Stops a task: a running one cancels its job (the copy in progress is removed), a queued one
    /// ends without starting.
    pub fn cancel(&mut self, id: TaskId) {
        let Some(task) = self.get_mut(id) else {
            return;
        };
        match &mut task.state {
            State::Running(job) => job.cancel(),
            State::Queued(_) => {
                task.state = State::Ended {
                    report: Report {
                        failures: Vec::new(),
                        cancelled: true,
                        ..Report::default()
                    },
                    at: Instant::now(),
                };
                self.cancelled_queued.push(id);
            }
            State::Ended { .. } => {}
        }
    }

    /// Cancels a task that goes with another one being stopped (the other destinations of an
    /// operation); it leaves the list once it has ended, now if it never started.
    pub fn dismiss(&mut self, id: TaskId) {
        self.cancel(id);
        if let Some(task) = self.get_mut(id) {
            task.dismissed = true;
        }
        self.tasks
            .retain(|task| task.id != id || (task.is_active() && task.has_started()));
    }

    /// F10 > Wait: a task the user paused would keep the program open for good.
    pub fn resume_all(&mut self) {
        let ids: Vec<TaskId> = self.tasks.iter().map(|task| task.id).collect();
        for id in ids {
            self.set_paused(id, false);
        }
    }

    pub fn cancel_all(&mut self) {
        let ids: Vec<TaskId> = self.tasks.iter().map(|task| task.id).collect();
        for id in ids {
            self.cancel(id);
        }
        self.cancelled_at = Some(std::time::Instant::now());
    }

    /// Removes a task that has ended (its report was seen); a running one stays.
    pub fn remove(&mut self, id: TaskId) {
        self.tasks.retain(|task| task.id != id || task.is_active());
    }
}

/// A folder as the Screens list names it.
fn shown_path(folder: &VfsPath) -> PathBuf {
    folder
        .as_local()
        .map_or_else(|| PathBuf::from(folder.to_string()), PathBuf::from)
}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
