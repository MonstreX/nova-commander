//! The window over the screen while a plugin's call is waited for past [`QUICK`] (R54; the owner's
//! rule: every waiting window says what goes on and shows that it does): the plugin's title, then the
//! item at hand and how far with FAR's bar of the percent when the call tells it, else "The plugin is
//! working..." with a busy bar and the time taken; Esc's line under them.
//!
//! [`QUICK`]: super::super::held::QUICK

use super::super::App;
use super::super::dialog::{WaitBar, WaitWindow};
use super::keys::PLUGIN_WORKING;
use crate::plugin_host::CallProgress;

/// The window's lines for a call that tells its progress: its plugin's title, the item at hand, how far.
fn progress_lines(title: String, told: &CallProgress) -> Vec<String> {
    let count = if told.total > 0 {
        let percent = told.done.saturating_mul(100) / told.total;
        format!("{} of {} ({percent}%)", told.done, told.total)
    } else {
        format!("{} done", told.done)
    };
    let item = Some(told.text.clone()).filter(|text| !text.is_empty());
    std::iter::once(title).chain(item).chain([count]).collect()
}

/// The bar of a call: its percent when it tells a total, else a busy bar going `for` so long.
fn bar_of(told: Option<&CallProgress>, going: std::time::Duration) -> WaitBar {
    match told.filter(|told| told.total > 0) {
        Some(told) => WaitBar::Percent((told.done.saturating_mul(100) / told.total).min(100)),
        None => WaitBar::Busy(going),
    }
}

impl App {
    /// The window up while a call is waited for, with no window over the screen: none before
    /// [`QUICK`](super::super::held::QUICK), nor while a plugin's key loop waits for the user's key
    /// (it does not work then).
    pub(in crate::app) fn plugin_wait_window(&self) -> Option<WaitWindow> {
        let wait = self.plugins.key_wait.as_ref()?;
        if self.overlay.is_some() || self.plugin_reads_input() {
            return None;
        }
        let tail = wait.held.tail()?;
        let title = (self.plugins.host.plugins.iter())
            .find(|loaded| loaded.name.eq_ignore_ascii_case(&wait.plugin))
            .map_or_else(|| wait.plugin.clone(), |loaded| loaded.info.title.clone());
        let told = self.plugins.host.progress(&wait.plugin, wait.serial);
        let mut lines = match &told {
            Some(told) => progress_lines(title, told),
            None => vec![title, PLUGIN_WORKING.to_string()],
        };
        // "It does not answer": it may be started anew, the line before the stage's own.
        if wait.held.asking() {
            lines.push(super::manager::RESTART.to_string());
        }
        Some(WaitWindow {
            lines,
            bar: Some(bar_of(told.as_ref(), wait.held.elapsed())),
            tail,
        })
    }

    /// The window's text, its lines and Esc's (tests).
    #[cfg(test)]
    pub(in crate::app) fn plugin_wait_lines(&self) -> Option<Vec<String>> {
        let window = self.plugin_wait_window()?;
        Some(window.lines.into_iter().chain(window.tail).collect())
    }

    /// Whether the wait's window has just come or changed (its progress, its busy bar's step): the
    /// screen is drawn again for it.
    pub(super) fn plugin_wait_due(&mut self) -> bool {
        if self.overlay.is_some() || self.plugin_reads_input() {
            return false;
        }
        let plugins = &mut self.plugins;
        let Some(wait) = plugins.key_wait.as_mut() else {
            return false;
        };
        let told = plugins.host.progress(&wait.plugin, wait.serial);
        let moved = told != wait.told;
        wait.told = told;
        let step = wait.held.elapsed().as_millis() / super::super::waiting::BUSY_STEP.as_millis();
        let shown = wait.held.tail().is_some();
        let stepped = shown && std::mem::replace(&mut wait.step, step) != step;
        wait.held.due() | moved | stepped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner's rule: a call that tells a total shows its percent; one that does not, a busy bar.
    #[test]
    fn a_call_shows_its_percent_or_a_busy_bar() {
        let told = |done, total| CallProgress {
            done,
            total,
            text: String::new(),
        };
        let going = std::time::Duration::from_secs(3);
        assert_eq!(bar_of(Some(&told(1, 4)), going), WaitBar::Percent(25));
        assert_eq!(bar_of(Some(&told(9, 4)), going), WaitBar::Percent(100));
        assert_eq!(bar_of(Some(&told(5, 0)), going), WaitBar::Busy(going));
        assert_eq!(bar_of(None, going), WaitBar::Busy(going));
    }
}
