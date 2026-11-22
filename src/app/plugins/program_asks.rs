//! What a plugin asks of the program beyond its panels (API 3, P6.0e; FAR's `AdvControl`,
//! `FileFilterControl`, `CopyToClipboard`, `ShowHelp`): keys done as typed once its call has returned,
//! the screen's size, a panel's filter, the clipboard, its help, a command run as the command line runs
//! it, and the call back on its thread a worker of its asked for (`synchro`).

use std::time::SystemTime;

use ratatui::crossterm::event::KeyEvent;

use super::panel_control::PanelAsk;
use crate::app::App;
use crate::plugin_host::panel_requests::{PanelAnswer, PanelRequest};

impl App {
    /// The answer to a request of the program's, not of a panel's (none for a panel's).
    pub(super) fn program_answer(&mut self, ask: &PanelAsk) -> Option<PanelAnswer> {
        let done = |ok: bool| Some(PanelAnswer::Done(ok));
        match &ask.request {
            PanelRequest::PostKeys(names) => done(self.post_keys(&ask.plugin, names)),
            PanelRequest::Screen => Some(PanelAnswer::Screen {
                columns: self.width,
                rows: self.height,
            }),
            PanelRequest::Filter { entries, .. } => {
                let filter = &self.panels[ask.side.index()].filter;
                let now = SystemTime::now();
                let matches = (entries.iter())
                    .map(|entry| filter.takes(entry, now))
                    .collect();
                Some(PanelAnswer::Matches(matches))
            }
            PanelRequest::Clipboard(text) => done(self.clipboard.set_text(text).is_ok()),
            // The program has no help yet (the owner: after the main features).
            PanelRequest::Help(_) => done(false),
            PanelRequest::Plugins(action, text) => {
                let (action, text) = (*action, text.clone());
                done(self.plugins_control(&ask.plugin, action, &text))
            }
            PanelRequest::Confirmation(name) => {
                Some(PanelAnswer::Confirms(self.confirmation(name)))
            }
            PanelRequest::MatchMasks { masks, name } => Some(PanelAnswer::Confirms(
                self.file_mask(masks).map(|mask| mask.matches(name)),
            )),
            // Started only with no other work waited for (it waits for its target's volumes): kept until
            // then (R7 review B2: a second wait took the first one's place, a debug build panicked).
            PanelRequest::Transfer(_) if self.waiting.is_some() => None,
            PanelRequest::Transfer(transfer) => done(self.plugin_transfer(&ask.plugin, transfer)),
            PanelRequest::State(_) | PanelRequest::Items { .. } | PanelRequest::Do(..) => None,
        }
    }

    /// FAR's `FSSF_CONFIRMATIONS` by its names (any case): whether the program asks before the work;
    /// none for a name it does not know.
    pub(super) fn confirmation(&self, name: &str) -> Option<bool> {
        let prefs = &self.prefs;
        let delete = &self.config.delete_settings;
        Some(match name.to_ascii_lowercase().as_str() {
            "delete" => delete.confirm,
            "deletefolder" => delete.confirm_folders,
            "ro" => delete.confirm_read_only,
            "drag" => prefs.confirm_drag,
            "esc" => prefs.confirm_interrupt,
            "exit" => prefs.confirm_exit,
            "historyclear" => prefs.confirm_history_clear,
            "allowreedit" => prefs.confirm_reedit,
            "removeconnection" => prefs.confirm_connection,
            _ => return None,
        })
    }

    /// The keys `plugin` named in `names` (separated by spaces) kept to be done as typed once its call has
    /// ended; false when a name is no key (none kept).
    fn post_keys(&mut self, plugin: &str, names: &str) -> bool {
        let keys: Option<Vec<KeyEvent>> = (names.split_whitespace())
            .map(|name| super::menu::parse(name).map(super::menu::BreakKey::event))
            .collect();
        let Some(keys) = keys else {
            return false;
        };
        let posted = keys.into_iter().map(|key| (plugin.to_string(), key));
        self.plugins.posted_keys.extend(posted);
        true
    }

    /// The keys plugins posted, done as typed in their order once the call of the plugin that posted each
    /// has ended and its answer is taken (FAR's synchronous call leaves them in the console's queue): a
    /// posted Esc is never the user's Esc on that call.
    pub(super) fn do_posted_keys(&mut self) -> bool {
        let mut any = false;
        while let Some((plugin, key)) = self.plugins.posted_keys.front().cloned() {
            if self.plugins.host.in_call(&plugin) || self.plugin_key_waits_for(&plugin) {
                break;
            }
            self.plugins.posted_keys.pop_front();
            self.dispatch_key(key);
            any = true;
        }
        any
    }

    /// A plugin's command run as the command line runs it (FAR's farcmds on the user screen), the line
    /// the user typed left as it is; not a plugin's prefix again.
    pub(super) fn run_command_for_plugin(&mut self, line: &str) -> bool {
        self.run_menu_line_in(line, None)
    }
}

#[cfg(test)]
#[path = "program_asks_tests.rs"]
mod tests;
