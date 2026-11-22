//! Disk work the screen must not wait for (ADR-0012 3c): the drive menu's reading and actions, the looks
//! at a drive before its questions, entering a drive, making a folder, network names. The work runs on a
//! thread of its own; FAR's message without buttons is up meanwhile (none at all where FAR shows none).
//! Keys pressed meanwhile wait and come after the work's end, in order, as FAR leaves them in the console
//! queue; dialogs of other work (a task's question, a late error) wait too. Work that ends within
//! [`QUICK`] is done at once, as before: the screen does not blink (a menu gone for a frame).
//! ADR-0019: past [`QUICK`] a window is up whatever the work (its own lines, or "Working..." over the
//! screen as it is), and Esc stops the waiting: what the work ends with is never done to the app, so the
//! app stays as it was before it (a disk that does not answer holds nothing); the keys held go, and a
//! notice says so. The work's thread is left to end by itself. The keys, the window and Esc are
//! [`Held`]'s, the plugins' calls' too (R54).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyEvent;

use super::App;
use super::dialog::{Dialog, WaitBar, WaitWindow};
use super::held::{Cancel, Held, Press};

/// What is done to the app when the work has ended.
pub(super) type Then = Box<dyn FnOnce(&mut App) + Send>;

/// How long the UI waits for work before it goes on beside it (as a panel waits for its folder).
use super::held::QUICK;

/// [`QUICK`]: in tests, long enough that work which ends is always done at once whatever the machine's
/// load (the disks' answers then deterministic); tests of work that outlasts it shorten it.
#[cfg(not(test))]
fn quick() -> Duration {
    QUICK
}

#[cfg(test)]
thread_local! {
    static TEST_QUICK: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::from_secs(10)) };
}

#[cfg(test)]
fn quick() -> Duration {
    TEST_QUICK.with(std::cell::Cell::get)
}

/// Work outlasting the quick wait, on this test's thread: the quick wait as the program has it.
#[cfg(test)]
pub(super) fn quick_as_the_program() {
    TEST_QUICK.with(|quick| quick.set(QUICK));
}

#[cfg(test)]
thread_local! {
    static TEST_GATE: std::cell::RefCell<Option<mpsc::Receiver<()>>> =
        const { std::cell::RefCell::new(None) };
}

/// The next work this test's thread starts waits until the sender is used or dropped (a disk that does
/// not answer); the quick wait as the program has it.
#[cfg(test)]
pub(super) fn hold_next_work() -> mpsc::Sender<()> {
    quick_as_the_program();
    let (release, gate) = mpsc::channel();
    TEST_GATE.with(|held| *held.borrow_mut() = Some(gate));
    release
}

/// FAR's `MChangeWaitingLoadDisk`, up while media is loaded or a network drive connected.
pub(super) const WAITING_FOR_DISK: &str = "Waiting for the disk to mount\u{2026}";

/// The window over the screen while work that shows none of its own runs past [`QUICK`].
pub(super) const WORKING: &str = "Working\u{2026}";

/// The notice after Esc stopped the waiting (what the work did before is not undone).
pub(super) const STOPPED: [&str; 2] = [
    "The operation was not waited for.",
    "It may still finish by itself.",
];

/// The notice after Esc stopped looks at the disks an operation makes before it starts: it never did.
pub(super) const NOT_STARTED: [&str; 2] = [
    "The operation was not started:",
    "a disk did not answer. Nothing has changed.",
];

/// What Esc stopped: its notice, and what to let go then (a count that went with a question).
pub(super) struct Stop {
    notice: [&'static str; 2],
    on_stop: Option<Box<dyn FnOnce() + Send>>,
    /// What closing the notice does (a menu the work was asked from, read again); none: nothing.
    back: Option<super::dialog::ConfirmAction>,
}

/// Work on its own thread.
pub struct Waiting {
    done: Receiver<Then>,
    /// Its thread: a call it hangs in broken when it is let go (ADR-0019 §4).
    thread: std::thread::JoinHandle<()>,
    /// Keys pressed meanwhile, to be handled after its end; its window and Esc.
    held: Held,
    /// Its window's own lines (none: "Working...").
    lines: Vec<String>,
    stop: Stop,
    /// How far it is (`show_progress`): the part done, the whole, the percent last drawn.
    progress: Option<(std::sync::Arc<std::sync::atomic::AtomicU64>, u64, u64)>,
    /// When it began, and the busy bar's step last drawn (`busy_line`).
    began: Instant,
    step: u128,
    /// What the work is, said in place of "Working..." over a screen it leaves as it is
    /// (`label_waiting`).
    label: Option<&'static str>,
}

/// How often the busy bar steps on (the window drawn again; `ui::wait_window` draws it).
pub const BUSY_STEP: Duration = Duration::from_millis(50);

/// `done` of `total` as a percent.
fn percent_of(done: u64, total: u64) -> u64 {
    match total {
        0 => 100,
        total => {
            u64::try_from(u128::from(done.min(total)) * 100 / u128::from(total)).unwrap_or(100)
        }
    }
}

impl App {
    /// Runs `work` on its own thread; what it returns is done to the app when it ends. Work that takes
    /// longer than [`QUICK`] goes on beside the UI with `lines` shown (none: the screen stays as it is,
    /// a dialog included), the keys waiting.
    pub(super) fn wait_for(
        &mut self,
        lines: Vec<String>,
        work: impl FnOnce() -> Then + Send + 'static,
    ) {
        let stop = Stop {
            notice: STOPPED,
            on_stop: None,
            back: None,
        };
        self.wait_with(lines, stop, None, work);
    }

    /// [`wait_for`](Self::wait_for) of work that can be stopped: it runs under a stop of its own
    /// (`vfs::stop`, a plugin's file call hears it), Esc sets it and the work's end says how it went;
    /// one that does not end within 3 s asks whether to wait more (ADR-0019 §4, review R14).
    pub(super) fn wait_for_stoppable(
        &mut self,
        lines: Vec<String>,
        work: impl FnOnce() -> Then + Send + 'static,
    ) {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let asked = std::sync::Arc::clone(&stop);
        let cancel: Cancel =
            Box::new(move || asked.store(true, std::sync::atomic::Ordering::Relaxed));
        let notice = Stop {
            notice: STOPPED,
            on_stop: None,
            back: None,
        };
        self.wait_with(lines, notice, Some(cancel), move || {
            crate::vfs::stop::stopped_by(stop, work)
        });
    }

    /// The notice of work that ended because Esc asked it to.
    pub(super) fn show_stopped(&mut self) {
        let lines = STOPPED.iter().map(ToString::to_string).collect();
        self.show_dialog(Dialog::message("Stopped", lines));
    }

    /// [`wait_for`](Self::wait_for) of work asked from a menu that is read again after it: Esc's
    /// notice brings it back as closing it does (`back`).
    pub(super) fn wait_for_back(
        &mut self,
        lines: Vec<String>,
        back: super::dialog::ConfirmAction,
        work: impl FnOnce() -> Then + Send + 'static,
    ) {
        let stop = Stop {
            notice: STOPPED,
            on_stop: None,
            back: Some(back),
        };
        self.wait_with(lines, stop, None, work);
    }

    /// [`wait_for`](Self::wait_for) of the looks at the disks an operation makes before it starts (its
    /// volumes, its target, the Recycle Bins): Esc says it never started, and `on_stop` lets go what was
    /// kept for it (ADR-0019 §3: back to before).
    pub(super) fn wait_to_start(
        &mut self,
        on_stop: Option<Box<dyn FnOnce() + Send>>,
        work: impl FnOnce() -> Then + Send + 'static,
    ) {
        let stop = Stop {
            notice: NOT_STARTED,
            on_stop,
            back: None,
        };
        self.wait_with(Vec::new(), stop, None, work);
    }

    /// The work, `cancel` asking it to stop on Esc (none: Esc lets it go).
    fn wait_with(
        &mut self,
        lines: Vec<String>,
        stop: Stop,
        cancel: Option<Cancel>,
        work: impl FnOnce() -> Then + Send + 'static,
    ) {
        debug_assert!(self.waiting.is_none(), "one work at a time");
        let (send, done) = mpsc::channel();
        #[cfg(test)]
        let gate = TEST_GATE.with(|held| held.borrow_mut().take());
        let started = std::thread::Builder::new()
            .name("waiting work".to_string())
            .spawn(move || {
                #[cfg(test)]
                if let Some(gate) = gate {
                    let _ = gate.recv();
                }
                // The app may be gone already (it quit, or Esc stopped the waiting): nobody to tell.
                let _ = send.send(work());
            });
        let Ok(thread) = started else {
            return self.work_broke();
        };
        let since = Instant::now();
        match done.recv_timeout(quick()) {
            Ok(then) => return then(self),
            Err(RecvTimeoutError::Disconnected) => return self.work_broke(),
            Err(RecvTimeoutError::Timeout) => {}
        }
        // A window of its own is FAR's message in place of what was up.
        if !lines.is_empty() {
            self.overlay = None;
        }
        self.waiting = Some(Waiting {
            done,
            thread,
            held: Held::new(since, cancel),
            lines,
            stop,
            progress: None,
            began: since,
            step: 0,
            label: None,
        });
    }

    /// The work under way with no window of its own says what it is in place of "Working..." (owner
    /// 2026-11-21: a waiting window tells what goes on); the dialog or menu it was asked from stays.
    pub(super) fn label_waiting(&mut self, label: &'static str) {
        if let Some(waiting) = &mut self.waiting {
            waiting.label = Some(label);
        }
    }

    /// The work under way shows how far it is: `done` (the work counts it) of `total`, FAR's bar under
    /// its lines (the editor's save, FAR's `MEditSaving` with its progress).
    pub(super) fn show_progress(
        &mut self,
        done: std::sync::Arc<std::sync::atomic::AtomicU64>,
        total: u64,
    ) {
        if let Some(waiting) = &mut self.waiting {
            waiting.progress = Some((done, total, 0));
        }
    }

    /// `path` opened by the system as Explorer would (its association), on a thread of its own: a share
    /// or a handler that does not answer holds nothing, a window and Esc past 300 ms (ADR-0019, R10);
    /// a failure is shown under `title`.
    pub(super) fn open_by_system(&mut self, path: std::path::PathBuf, title: &'static str) {
        self.wait_for(Vec::new(), move || {
            let opened = crate::platform::process::open_with_association(&path);
            Box::new(move |app: &mut App| {
                if let Err(error) = opened {
                    app.show_error(title, &error);
                }
            })
        });
        self.label_waiting("Opening the file with its program\u{2026}");
    }

    /// Whether keys wait for work.
    pub(super) fn is_waiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// A dialog is up or work runs: other work's dialogs wait.
    pub(super) fn screen_busy(&self) -> bool {
        self.overlay.is_some() || self.is_waiting()
    }

    /// Keeps `key` for after the work; false when no work runs. Esc stops the waiting.
    pub(super) fn hold_key(&mut self, key: KeyEvent) -> bool {
        let Some(waiting) = &mut self.waiting else {
            return false;
        };
        if waiting.held.press(key) == Press::Abandon {
            self.stop_waiting();
        }
        true
    }

    /// Esc while work runs (ADR-0019): its end will not be done to the app, the keys held go, a notice;
    /// a window that was up (a menu the work was asked from) comes back after it.
    fn stop_waiting(&mut self) {
        let Some(waiting) = self.waiting.take() else {
            return;
        };
        // A disk that does not answer: its call fails at once, the thread freed (what it ends with is
        // thrown away all the same).
        crate::platform::process::break_thread_io(&waiting.thread);
        if let Some(on_stop) = waiting.stop.on_stop {
            on_stop();
        }
        self.stopped_under = self.overlay.take();
        let lines = waiting
            .stop
            .notice
            .iter()
            .map(ToString::to_string)
            .collect();
        match waiting.stop.back {
            Some(action) => self.show_dialog(Dialog::Confirm {
                title: "Stopped".to_string(),
                lines,
                buttons: vec!["OK"],
                focused: 0,
                danger: false,
                action,
            }),
            None => self.show_dialog(Dialog::message("Stopped", lines)),
        }
    }

    /// The window under Esc's notice, back once the notice has gone.
    pub(super) fn resume_stopped_under(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() || self.stopped_under.is_none() {
            return false;
        }
        self.overlay = self.stopped_under.take();
        true
    }

    /// The window over the screen while work the keys wait for runs past [`QUICK`] (ADR-0019): its own
    /// lines, else "Working..."; a plugin's call's (R54: one window for both).
    pub fn held_window(&self) -> Option<WaitWindow> {
        let Some(waiting) = &self.waiting else {
            return self.plugin_wait_window();
        };
        let tail = waiting.held.tail()?;
        let lines = match waiting.lines.is_empty() {
            true => vec![waiting.label.unwrap_or(WORKING).to_string()],
            false => waiting.lines.clone(),
        };
        let bar = match &waiting.progress {
            Some((done, total, _)) => {
                let done = done.load(std::sync::atomic::Ordering::Relaxed);
                WaitBar::Percent(percent_of(done, *total))
            }
            None => WaitBar::Busy(waiting.began.elapsed()),
        };
        Some(WaitWindow {
            lines,
            bar: Some(bar),
            tail,
        })
    }

    /// The work's end: its message goes, what it ended with is done, then the keys pressed meanwhile.
    /// True on a change.
    pub(super) fn pump_waiting(&mut self) -> bool {
        let Some(waiting) = &mut self.waiting else {
            return false;
        };
        let then = match waiting.done.try_recv() {
            Ok(then) => Some(then),
            // Its window came, or changed (its percent too): drawn again.
            Err(TryRecvError::Empty) => {
                let moved = waiting
                    .progress
                    .as_mut()
                    .is_some_and(|(done, total, drawn)| {
                        let now =
                            percent_of(done.load(std::sync::atomic::Ordering::Relaxed), *total);
                        std::mem::replace(drawn, now) != now
                    });
                // The busy bar's next step, while the window is up.
                let step = waiting.began.elapsed().as_millis() / BUSY_STEP.as_millis();
                let shown = waiting.held.window(&[]).is_some();
                let stepped = shown && std::mem::replace(&mut waiting.step, step) != step;
                return waiting.held.due() | moved | stepped;
            }
            Err(TryRecvError::Disconnected) => None,
        };
        let keys = self
            .waiting
            .take()
            .map(|waiting| waiting.held.keys)
            .unwrap_or_default();
        match then {
            Some(then) => then(self),
            None => self.work_broke(),
        }
        let mut keys = keys.into_iter();
        while !self.is_waiting()
            && let Some(key) = keys.next()
        {
            self.dispatch_key(key);
        }
        // Work the keys started holds the rest.
        if let Some(waiting) = &mut self.waiting {
            waiting.held.keys.splice(0..0, keys);
        }
        true
    }

    /// The work panicked.
    fn work_broke(&mut self) {
        let error = std::io::Error::other("The operation ended unexpectedly");
        self.show_error("Error", &error);
    }
}

/// The work's window's lines without its bar (the busy one moves with the clock; tests).
#[cfg(test)]
pub(in crate::app) fn held_text(app: &App) -> Option<Vec<String>> {
    (app.held_window()).map(|window| window.lines.into_iter().chain(window.tail).collect())
}

/// Whether the screen shows the notice Esc leaves on stopped work.
#[cfg(test)]
pub(super) fn shows_stopped(app: &App) -> bool {
    matches!(
        &app.overlay,
        Some(super::Overlay::Dialog(Dialog::Message { lines, .. })) if lines.first().map(String::as_str) == Some(STOPPED[0])
    )
}

/// Ticks until the work has ended (tests).
#[cfg(test)]
pub(super) fn finish_waiting(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.is_waiting() {
        assert!(std::time::Instant::now() < deadline, "the work did not end");
        if !app.pump_waiting() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
#[path = "waiting_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "wait_to_start_tests.rs"]
mod start_tests;
