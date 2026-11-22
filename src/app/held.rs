//! One mechanism for work the keys wait for (ADR-0019, review R54): disk work on a thread of its own
//! (`waiting`) and a plugin's call (`plugins::keys`). The keys pressed meanwhile are held for after it, in
//! order; past [`QUICK`] a window is up, and Esc stops the work. Work that can be asked to stop (a call to
//! break, a plugin asked whether Esc was pressed) is asked, and its own end says how far it got; one that
//! cannot, or does not stop within [`STOP_WAIT`], is let go: "It does not answer" - Enter waits more, Esc
//! abandons it (its late end thrown away, the app as it was before it).

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent};

/// How long the keys wait for work before its window comes and Esc stops it.
pub(in crate::app) const QUICK: Duration = Duration::from_millis(300);

/// How long work asked to stop is given before the question whether to wait more (ADR-0019 §4).
pub(in crate::app) const STOP_WAIT: Duration = Duration::from_secs(3);

/// Under every waiting window.
pub(in crate::app) const ESC_STOPS: &str = "Esc: stop waiting";

/// Under the window once the work was asked to stop.
pub(in crate::app) const STOPPING: &str = "Stopping\u{2026}";

/// Under the window once the work asked to stop has not, within [`STOP_WAIT`].
pub(in crate::app) const NOT_ANSWERING: [&str; 2] =
    ["It does not answer.", "Enter: wait more, Esc: abandon"];

/// Asks the work to stop; its end comes as usual, saying how far it got.
pub(in crate::app) type Cancel = Box<dyn FnOnce() + Send>;

/// What a key pressed while the work runs does.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::app) enum Press {
    /// Held for after the work, or taken by its window.
    Kept,
    /// The work is let go: its owner forgets it and says so.
    Abandon,
}

/// Where the work's window is: what is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Quick,
    Running,
    Stopping,
    Asking,
}

/// Work the keys wait for: since when, the keys held, how to ask it to stop.
pub(in crate::app) struct Held {
    since: Instant,
    pub(in crate::app) keys: Vec<KeyEvent>,
    cancel: Option<Cancel>,
    stopping: Option<Instant>,
    drawn: Stage,
}

impl std::fmt::Debug for Held {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Held")
            .field("since", &self.since)
            .field("keys", &self.keys)
            .field("stoppable", &self.cancel.is_some())
            .field("stopping", &self.stopping)
            .finish()
    }
}

impl Held {
    /// Work started at `since`; `cancel` asks it to stop (none: Esc lets it go at once).
    pub(in crate::app) fn new(since: Instant, cancel: Option<Cancel>) -> Self {
        Self {
            since,
            keys: Vec::new(),
            cancel,
            stopping: None,
            drawn: Stage::Quick,
        }
    }

    /// Work that can be asked to stop once it is known how (a plugin's call, numbered once it is sent):
    /// `cancel` made and kept unless it has one already or Esc came.
    pub(in crate::app) fn arm(&mut self, cancel: impl FnOnce() -> Option<Cancel>) {
        if self.cancel.is_none() && self.stopping.is_none() {
            self.cancel = cancel();
        }
    }

    /// The work goes on from now (a plugin's key loop has taken a key): its window's time starts anew.
    pub(in crate::app) fn restart(&mut self) {
        self.since = Instant::now();
    }

    /// How long the work has gone on (since it began or went on again): a busy bar's time.
    pub(in crate::app) fn elapsed(&self) -> Duration {
        self.since.elapsed()
    }

    fn stage(&self) -> Stage {
        match self.stopping {
            Some(asked) if asked.elapsed() >= STOP_WAIT => Stage::Asking,
            Some(_) => Stage::Stopping,
            None if self.since.elapsed() >= QUICK => Stage::Running,
            None => Stage::Quick,
        }
    }

    /// A key pressed meanwhile: held, or Esc past [`QUICK`] - the work asked to stop (the keys held go),
    /// or let go. While it stops the keys go; once it does not answer, Enter waits more and Esc abandons.
    pub(in crate::app) fn press(&mut self, key: KeyEvent) -> Press {
        let plain = key.modifiers.is_empty();
        let esc = key.code == KeyCode::Esc && plain;
        match self.stage() {
            Stage::Asking if esc => Press::Abandon,
            Stage::Asking => {
                if key.code == KeyCode::Enter && plain {
                    self.stopping = Some(Instant::now());
                }
                Press::Kept
            }
            Stage::Stopping => Press::Kept,
            Stage::Running if esc => match self.cancel.take() {
                Some(cancel) => {
                    cancel();
                    self.keys.clear();
                    self.stopping = Some(Instant::now());
                    Press::Kept
                }
                None => Press::Abandon,
            },
            Stage::Quick | Stage::Running => {
                self.keys.push(key);
                Press::Kept
            }
        }
    }

    /// The window's own last lines (Esc's, the question): none before [`QUICK`].
    pub(in crate::app) fn tail(&self) -> Option<Vec<String>> {
        self.window(&[])
    }

    /// The window's lines under the work's own `lines`: none before [`QUICK`].
    pub(in crate::app) fn window(&self, lines: &[String]) -> Option<Vec<String>> {
        let tail: &[&str] = match self.stage() {
            Stage::Quick => return None,
            Stage::Running => &[ESC_STOPS],
            Stage::Stopping => &[STOPPING],
            Stage::Asking => &NOT_ANSWERING,
        };
        Some(
            lines
                .iter()
                .cloned()
                .chain(tail.iter().map(ToString::to_string))
                .collect(),
        )
    }

    /// Whether the work asked to stop has not, within [`STOP_WAIT`] ("It does not answer").
    pub(in crate::app) fn asking(&self) -> bool {
        self.stage() == Stage::Asking
    }

    /// Whether the window has changed since it was last drawn (it came, the work stops, the question):
    /// the screen is drawn again once for it.
    pub(in crate::app) fn due(&mut self) -> bool {
        let now = self.stage();
        let changed = now != self.drawn;
        self.drawn = now;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn slow(cancel: Option<Cancel>) -> Held {
        Held::new(Instant::now() - QUICK, cancel)
    }

    #[test]
    fn keys_are_held_and_esc_is_one_of_them_before_the_window() {
        let mut held = Held::new(Instant::now(), None);
        assert_eq!(held.window(&[]), None);
        assert_eq!(held.press(key(KeyCode::Down)), Press::Kept);
        assert_eq!(held.press(key(KeyCode::Esc)), Press::Kept);
        assert_eq!(held.keys, [key(KeyCode::Down), key(KeyCode::Esc)]);
    }

    #[test]
    fn esc_lets_go_work_that_cannot_be_asked_to_stop() {
        let mut held = slow(None);
        assert_eq!(
            held.window(&["Reading".to_string()]),
            Some(vec!["Reading".to_string(), ESC_STOPS.to_string()])
        );
        let ctrl_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::CONTROL);
        assert_eq!(held.press(ctrl_esc), Press::Kept, "not Esc itself");
        assert_eq!(held.press(key(KeyCode::Esc)), Press::Abandon);
    }

    #[test]
    fn work_asked_to_stop_is_given_its_time_and_the_keys_go() {
        let asked = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&asked);
        let mut held = slow(Some(Box::new(move || flag.store(true, Ordering::SeqCst))));
        held.press(key(KeyCode::Down));
        assert!(held.due(), "its window came");
        assert!(!held.due(), "drawn once");
        assert_eq!(held.press(key(KeyCode::Esc)), Press::Kept);
        assert!(asked.load(Ordering::SeqCst), "asked to stop");
        assert!(held.keys.is_empty(), "the keys held went");
        assert_eq!(held.window(&[]), Some(vec![STOPPING.to_string()]));
        assert!(held.due());
        assert_eq!(held.press(key(KeyCode::Esc)), Press::Kept, "given its time");
        assert_eq!(held.press(key(KeyCode::Up)), Press::Kept);
        assert!(held.keys.is_empty(), "keys go while it stops");
    }

    #[test]
    fn work_that_does_not_stop_in_time_is_waited_for_more_or_abandoned() {
        let mut held = slow(Some(Box::new(|| {})));
        held.press(key(KeyCode::Esc));
        held.stopping = held.stopping.map(|asked| asked - STOP_WAIT);
        let question: Vec<String> = NOT_ANSWERING.iter().map(ToString::to_string).collect();
        assert_eq!(held.window(&[]), Some(question));
        assert!(held.due(), "the question drawn");
        assert_eq!(held.press(key(KeyCode::Enter)), Press::Kept, "wait more");
        assert_eq!(held.window(&[]), Some(vec![STOPPING.to_string()]));
        held.stopping = held.stopping.map(|asked| asked - STOP_WAIT);
        assert_eq!(held.press(key(KeyCode::Esc)), Press::Abandon);
    }
}
