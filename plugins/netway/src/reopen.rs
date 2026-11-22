//! A session the server dropped opened again (WinSCP's `TTerminal::QueryReopen`, `DoQueryReopen`,
//! `Reopen`; NetBox's "Endurance settings"): before an operation of the panel, a lost session is logged
//! into again with the user and the password that logged in, at the panel's folder - after WinSCP's
//! "Lost connection." with "&Reconnect" / "&Abort", or with "Reconnect after" set, after that many
//! seconds counted down in a window Esc stops; at most "Maximum number of retries" in a row (0: no end).
//! A panel's reading shows no windows (the program's rule for `list`): there the session is opened again
//! without a question, its own questions (a changed host key, a password no longer taken) refused, and
//! when that fails the reading says "Lost connection.".

use std::time::{Duration, Instant};

/// An FTP session not used this long is asked whether it still answers before an operation.
pub(crate) const FTP_IDLE: Duration = Duration::from_secs(10);

use nc_plugin_sdk::Message;

use crate::NetWay;
use crate::ask::Asker;
use crate::endurance::{self, Reconnect};
use crate::login::{self, Plan, Started};

/// WinSCP's `LOST_CONNECTION`.
pub const LOST: &str = "Lost connection.";

/// The countdown's line.
pub fn countdown(seconds: u64) -> String {
    format!("Reconnecting in {seconds} s...")
}

impl NetWay {
    /// The plan of the panel `instance`'s session at `folder`, if it is lost and may be tried again;
    /// counted as a try.
    fn lost_plan(&mut self, instance: &str, folder: &str, settings: Reconnect) -> Option<Plan> {
        if !self.is_lost(instance) {
            return None;
        }
        let remote = self.remotes.get_mut(instance)?;
        if settings.retries > 0 && remote.retries >= settings.retries {
            return None;
        }
        remote.retries += 1;
        Some(Plan {
            folder: folder.to_string(),
            ..remote.plan.clone()
        })
    }

    /// The new connection put in place of the lost one.
    fn reopened(&mut self, instance: &str, started: Started) {
        if let Some(remote) = self.remotes.get_mut(instance) {
            remote.connection = started.connection;
            remote.fs = started.fs;
            remote.retries = 0;
        }
    }

    /// The SSH connection closed, or an idle FTP control connection no longer answering.
    fn is_lost(&self, instance: &str) -> bool {
        let Some(remote) = self.remotes.get(instance) else {
            return false;
        };
        if (remote.connection.as_ref()).is_some_and(crate::ssh::Connection::is_closed) {
            return true;
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return false;
        };
        let host = self.host;
        let alive = runtime.wait(remote.fs.alive(FTP_IDLE), &|| host.cancelled());
        alive == Ok(false)
    }

    /// The panel's reading's reopening: no windows, no questions answered.
    pub(crate) fn reopen_silently(&mut self, instance: &str, folder: &str) -> bool {
        if !self.is_lost(instance) {
            return true;
        }
        let settings = endurance::reconnect(&self.host);
        // One try per reading: with no window there is no pause to wait between tries, nor a way to
        // stop them (a stop stays latched for the reading).
        let Some(plan) = self.lost_plan(instance, folder, settings) else {
            return false;
        };
        let Some(runtime) = self.runtime.as_ref() else {
            return false;
        };
        // Questions go nowhere: answered as refused.
        let (asker, questions) = Asker::new();
        drop(questions);
        let host = self.host;
        let work = async { login::start(&plan, &asker).await };
        let Ok(Ok(started)) = runtime.wait(work, &|| host.cancelled()) else {
            return false;
        };
        self.reopened(instance, started);
        true
    }

    /// WinSCP's `QueryReopen` before an operation of the panel `instance` at `folder`: true when its
    /// session is there (again).
    pub(crate) fn reopen(&mut self, instance: &str, folder: &str) -> bool {
        if !self.is_lost(instance) {
            return true;
        }
        let settings = endurance::reconnect(&self.host);
        loop {
            let Some(plan) = self.lost_plan(instance, folder, settings) else {
                self.error(LOST);
                return false;
            };
            if !self.reopen_asked(settings) {
                return false;
            }
            let title =
                (self.remotes.get(instance)).map_or(String::new(), |remote| remote.name.clone());
            if let Some(started) = self.log_in(&plan, &title) {
                self.reopened(instance, started);
                return true;
            }
        }
    }

    /// WinSCP's question with "&Reconnect" / "&Abort", or its timeout counted down (Esc: no).
    fn reopen_asked(&self, settings: Reconnect) -> bool {
        if settings.auto == 0 {
            let message = Message::new("Error", &[LOST])
                .warning()
                .buttons(&["&Reconnect", "&Abort"]);
            return self.host.show(&message) == Some(0);
        }
        let until = Instant::now() + Duration::from_millis(u64::from(settings.auto));
        let mut shown = None;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            if self.host.cancelled() {
                self.host.resume();
                return false;
            }
            let seconds = left.as_secs() + 1;
            if shown != Some(seconds) {
                shown = Some(seconds);
                self.host.busy("Error", &[LOST, &countdown(seconds)]);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }
}

#[cfg(test)]
#[path = "reopen_tests.rs"]
mod tests;
