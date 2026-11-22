//! A task's clock for its progress dialog (FAR's `copy_progress::UpdateTime`): the time it worked, without
//! the time it stood waiting for the user (FAR's `WaitUserTime`: a question up, the "Operation has been
//! interrupted" question) or paused. Its "Remaining" and its speed go by the same time.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default)]
pub struct TaskClock {
    started: Option<Instant>,
    /// The time it stood, the current stand left out.
    stood: Duration,
    /// Since when it stands, while it does.
    standing: Option<Instant>,
}

impl TaskClock {
    pub fn start(&mut self, now: Instant) {
        *self = Self {
            started: Some(now),
            ..Self::default()
        };
    }

    pub fn has_started(&self) -> bool {
        self.started.is_some()
    }

    /// Whether the task stands (waits for the user, is paused or held) from `now` on.
    pub fn stand(&mut self, standing: bool, now: Instant) {
        match (standing, self.standing) {
            (true, None) => self.standing = Some(now),
            (false, Some(since)) => {
                self.stood += now.saturating_duration_since(since);
                self.standing = None;
            }
            _ => {}
        }
    }

    /// The time it worked up to `now`; zero before it started.
    pub fn worked(&self, now: Instant) -> Duration {
        let Some(started) = self.started else {
            return Duration::ZERO;
        };
        let standing = self
            .standing
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
        (now.saturating_duration_since(started))
            .saturating_sub(self.stood)
            .saturating_sub(standing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn the_time_it_stood_is_not_counted() {
        let start = Instant::now();
        let mut clock = TaskClock::default();
        assert_eq!(clock.worked(start), Duration::ZERO, "not started");
        clock.start(start);
        clock.stand(true, start + 2 * SECOND);
        assert_eq!(clock.worked(start + 5 * SECOND), 2 * SECOND, "it stands");
        clock.stand(true, start + 6 * SECOND);
        clock.stand(false, start + 7 * SECOND);
        clock.stand(false, start + 8 * SECOND);
        assert_eq!(clock.worked(start + 10 * SECOND), 5 * SECOND);
        assert!(clock.has_started());
    }

    #[test]
    fn a_start_again_counts_from_nothing() {
        let start = Instant::now();
        let mut clock = TaskClock::default();
        clock.start(start);
        clock.stand(true, start);
        clock.start(start + 3 * SECOND);
        assert_eq!(clock.worked(start + 4 * SECOND), SECOND);
    }
}
