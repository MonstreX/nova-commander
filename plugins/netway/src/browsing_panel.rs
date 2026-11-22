//! Synchronized browsing and "Compare directories" at work on the panels (see `browsing`): Ctrl+Alt+B or the
//! commands menu turns browsing on and off; a move of the server's panel takes the disk's panel along at the
//! panel's synchro; comparing reads both folders and selects what differs.

use nc_plugin_sdk::{KeyAnswer, Message, Which};

use crate::NetWay;
use crate::browsing::{self, COMPARE_TITLE};

impl NetWay {
    fn tell(&self, title: &str, text: &str) {
        let lines: Vec<&str> = text.split('\n').collect();
        self.host
            .show(&Message::new(title, &lines).buttons(&["&OK"]));
    }

    /// Ctrl+Alt+B: synchronized browsing on or off, told (NetBox's message).
    pub(crate) fn toggle_browsing(&mut self, instance: &str) -> KeyAnswer {
        let Some(remote) = self.remotes.get_mut(instance) else {
            return KeyAnswer::default();
        };
        remote.synchronized = !remote.synchronized;
        let text = match remote.synchronized {
            true => browsing::ON,
            false => browsing::OFF,
        };
        self.tell("Information", text);
        KeyAnswer::default()
    }

    fn stop_browsing(&mut self, instance: &str, text: &str) {
        if let Some(remote) = self.remotes.get_mut(instance) {
            remote.synchronized = false;
        }
        self.tell("Error", text);
    }

    /// The disk's panel taken from `before` to the folder matching `now` (NetBox's `SetDirectoryEx`):
    /// a missing folder made when the user says so, browsing off when it cannot follow.
    pub(crate) fn follow(&mut self, instance: &str, before: &str, now: &str) {
        let other = self.host.panel(Which::Passive);
        if !browsing::regular(other.as_ref()) {
            self.tell("Error", browsing::LOCAL_REQUIRED);
            return;
        }
        let local = other.map(|state| state.folder).unwrap_or_default();
        let Some(target) = browsing::local_path(before, now, &local) else {
            self.stop_browsing(instance, browsing::FAILED);
            return;
        };
        if self.host.set_folder(Which::Passive, &target, None) {
            return;
        }
        let question = browsing::create_question(&target);
        let lines: Vec<&str> = question.split('\n').collect();
        let message = Message::new("Information", &lines).buttons(&["&Yes", "&No"]);
        if self.host.show(&message) != Some(0) {
            if let Some(remote) = self.remotes.get_mut(instance) {
                remote.synchronized = false;
            }
            return;
        }
        let made = std::fs::create_dir_all(&target).is_ok();
        if !made || !self.host.set_folder(Which::Passive, &target, None) {
            self.stop_browsing(instance, browsing::FAILED);
        }
    }

    /// NetBox's "Compare directories": the differing files selected in both panels, their counts told.
    pub(crate) fn compare_directories(&mut self, instance: &str) -> KeyAnswer {
        if !browsing::regular(self.host.panel(Which::Passive).as_ref()) {
            self.tell("Error", browsing::LOCAL_REQUIRED);
            return KeyAnswer::default();
        }
        let (Some(remote), Some(runtime)) = (self.remotes.get(instance), self.runtime.as_ref())
        else {
            return KeyAnswer::default();
        };
        let host = self.host;
        let listed = runtime.wait(remote.fs.list(&remote.folder), &|| host.cancelled());
        let remotes = match listed {
            Ok(Ok(listed)) => listed,
            Ok(Err(error)) => {
                self.error(&error);
                return KeyAnswer::default();
            }
            Err(_) => return KeyAnswer::default(),
        };
        let locals = self.host.panel_items(Which::Passive);
        let (local, server) = browsing::differences(&locals, &remotes, false);
        if local.is_empty() && server.is_empty() {
            self.tell(COMPARE_TITLE, browsing::NO_DIFFERENCES);
            return KeyAnswer::default();
        }
        for (which, names) in [(Which::Passive, &local), (Which::Active, &server)] {
            self.host.select_all(which, false);
            self.host.select(which, names, true);
        }
        self.tell(
            COMPARE_TITLE,
            &browsing::compared(local.len(), server.len()),
        );
        KeyAnswer::default()
    }
}
