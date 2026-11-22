//! NetBox's "Synchronize" of a connected panel (`TWinSCPFileSystem::FullSynchronize`): the disk's panel
//! required, the dialog (its direction from the server's panel unless one is kept), the options kept when
//! asked, both trees compared ("Comparing"), "No differences found." or the checklist when previewing, the
//! checked items done in the copying's window ("Synchronizing"), both panels read again.

use std::path::Path;

use nc_plugin_sdk::{Dialog, DialogEvent, KeyAnswer, KeyPress, Message, Which};

use crate::NetWay;
use crate::copy_panel::Work;
use crate::copy_progress::Shown;
use crate::sync_collect::{Item, Mode, Node, collect, same_name};
use crate::sync_dialog::{self, Chosen};
use crate::sync_work::{checklist_menu, local_tree, remote_tree, toggled};

/// NetBox's `NB_FULL_SYNCHRONIZE_TITLE`, `NB_SYNCHRONIZE_PROGRESS_COMPARE_TITLE`, `NB_COMPARE_NO_DIFFERENCES`.
const TITLE: &str = "Synchronize";
const COMPARING: &str = "Comparing";
const NO_DIFFERENCES: &str = "No differences found.";

/// The tree's first level kept to the `names` selected (no names: all).
fn only(nodes: Vec<Node>, names: &[String]) -> Vec<Node> {
    match names.is_empty() {
        true => nodes,
        false => (nodes.into_iter())
            .filter(|node| {
                names
                    .iter()
                    .any(|name| crate::sync_collect::same_name(name, &node.name))
            })
            .collect(),
    }
}

impl NetWay {
    /// NetBox's `SynchronizeAllowSelectedOnly` and its names: the server's panel's selection and the
    /// disk's (either may have one), each name once.
    fn both_selections(&self, press: &KeyPress) -> Vec<String> {
        let mut names = press.selected.clone();
        let disk = self.host.panel_items(Which::Passive);
        for item in disk
            .into_iter()
            .filter(|item| item.selected && !item.parent)
        {
            if !names.iter().any(|name| same_name(name, &item.name)) {
                names.push(item.name);
            }
        }
        names
    }

    fn say(&self, title: &str, text: &str) {
        let lines: Vec<&str> = text.split('\n').collect();
        self.host
            .show(&Message::new(title, &lines).buttons(&["&OK"]));
    }

    /// The dialog: what the user chose, none on Cancel; the options kept when asked (and the direction
    /// when the user says so, NetBox's `NB_SAVE_SYNCHRONIZE_MODE`).
    fn synchronize_dialog(
        &self,
        instance: &str,
        (local, remote): (&str, &str),
        press: &KeyPress,
    ) -> Option<Chosen> {
        let (params, kept_mode) = sync_dialog::kept(&self.host);
        // From the server's panel the disk is the target (NetBox's `FullSynchronize(Source)`).
        let mode = kept_mode.unwrap_or(Mode::Local);
        let protocol = (self.remotes.get(instance)).map(|remote| remote.fs.protocol());
        let times = !matches!(protocol, Some("WebDAV" | "S3"));
        let items = sync_dialog::items(
            (local, remote),
            (mode, params),
            (times, !self.both_selections(press).is_empty()),
        );
        let dialog = Dialog::new(TITLE, items).size(76, 21);
        let closed = self.host.dialog_with(&dialog, |event, controls| {
            if matches!(event, DialogEvent::Init | DialogEvent::Changed(_)) {
                sync_dialog::update(controls);
            }
            true
        });
        if closed.button != Some(sync_dialog::OK) {
            return None;
        }
        let chosen = sync_dialog::chosen(&closed.values);
        if chosen.save {
            let overridden = kept_mode.is_none() && chosen.settings.mode != mode;
            let lines: Vec<&str> = sync_dialog::SAVE_MODE.split('\n').collect();
            let question = Message::new("Confirm", &lines).buttons(&["&Yes", "&No"]);
            let mode_too =
                kept_mode.is_some() || (overridden && self.host.show(&question) == Some(0));
            sync_dialog::keep(&self.host, &chosen, mode_too);
        }
        Some(chosen)
    }

    /// Both trees compared into the checklist; none when it failed (told) or was stopped.
    fn compared(&self, instance: &str, chosen: &Chosen, names: &[String]) -> Option<Vec<Item>> {
        let (remote, runtime) = (self.remotes.get(instance)?, self.runtime.as_ref()?);
        let host = self.host;
        host.busy(
            COMPARING,
            &[
                &format!("Local:  {}", chosen.local),
                &format!("Remote: {}", chosen.remote),
            ],
        );
        let local = match local_tree(Path::new(&chosen.local)) {
            Ok(tree) => only(tree, names),
            Err(error) => {
                self.say("Error", &format!("{}: {error}", chosen.local));
                return None;
            }
        };
        let stop = || host.cancelled();
        let listed = runtime.wait(remote_tree(&remote.fs, &chosen.remote, &stop), &stop);
        let server = match listed {
            Ok(Ok(tree)) => only(tree, names),
            Ok(Err(error)) => {
                self.say("Error", &error);
                return None;
            }
            Err(_) => return None,
        };
        Some(collect(&local, &server, &chosen.settings, ""))
    }

    /// The checklist until Enter (true) or Esc (false).
    fn checklist(&self, items: &mut [Item]) -> bool {
        let mut focus = 0;
        loop {
            let Some(chosen) = self.host.menu(&checklist_menu(items, focus)) else {
                return false;
            };
            focus = chosen.item.unwrap_or(focus);
            match chosen.key {
                Some(key) => toggled(items, chosen.item, key),
                None => return chosen.item.is_some(),
            }
        }
    }

    /// "Keep Remote Directory Up To Date" (the commands menu): the dialog, then watching in the copying's
    /// window until Esc; the server's panel read again.
    pub(crate) fn keep_up_to_date(&mut self, instance: &str, press: &KeyPress) -> KeyAnswer {
        use crate::keep_up_to_date as keep;
        let other = self.host.panel(Which::Passive);
        if !crate::browsing::regular(other.as_ref()) {
            self.say("Error", crate::browsing::LOCAL_REQUIRED);
            return KeyAnswer::default();
        }
        let local = other.map(|state| state.folder).unwrap_or_default();
        let Some(remote) = self
            .remotes
            .get(instance)
            .map(|remote| remote.folder.clone())
        else {
            return KeyAnswer::default();
        };
        let (params, _) = sync_dialog::kept(&self.host);
        let options = keep::kept_options(&self.host);
        let items = keep::items(
            (&local, &remote),
            (params, options),
            !self.both_selections(press).is_empty(),
        );
        let dialog = Dialog::new(keep::TITLE, items).size(76, 15);
        let closed = self.host.dialog(&dialog);
        if closed.button != Some(keep::START) {
            return KeyAnswer::default();
        }
        let watch = keep::watch_of(&closed.values);
        if watch.save {
            keep::keep(&self.host, &watch, params);
        }
        let names = match watch.selected_only {
            true => self.both_selections(press),
            false => Vec::new(),
        };
        let shown = Shown {
            moving: false,
            temporary: false,
            remote_source: false,
            operation: Some("Synchronizing"),
        };
        let work = Work::KeepUpToDate {
            watch: &watch,
            names: &names,
        };
        let _ = self.transfer(
            instance,
            shown,
            crate::options::copy_params(&self.host),
            work,
        );
        KeyAnswer {
            reread: true,
            ..KeyAnswer::default()
        }
    }

    /// "Synchronize" (the commands menu): see the module.
    pub(crate) fn synchronize(&mut self, instance: &str, press: &KeyPress) -> KeyAnswer {
        let other = self.host.panel(Which::Passive);
        if !crate::browsing::regular(other.as_ref()) {
            self.say("Error", crate::browsing::LOCAL_REQUIRED);
            return KeyAnswer::default();
        }
        let local = other.map(|state| state.folder).unwrap_or_default();
        let Some(remote) = self
            .remotes
            .get(instance)
            .map(|remote| remote.folder.clone())
        else {
            return KeyAnswer::default();
        };
        let Some(chosen) = self.synchronize_dialog(instance, (&local, &remote), press) else {
            return KeyAnswer::default();
        };
        let names = match chosen.selected_only {
            true => self.both_selections(press),
            false => Vec::new(),
        };
        let Some(mut items) = self.compared(instance, &chosen, &names) else {
            return KeyAnswer::default();
        };
        // Without the preview only what would be done counts (an unchecked deletion is not).
        let nothing = match chosen.preview {
            true => items.is_empty(),
            false => !items.iter().any(|item| item.checked),
        };
        if nothing {
            self.say(TITLE, NO_DIFFERENCES);
            return KeyAnswer::default();
        }
        if chosen.preview && !self.checklist(&mut items) {
            return KeyAnswer::default();
        }
        let shown = Shown {
            moving: false,
            temporary: false,
            remote_source: true,
            operation: Some("Synchronizing"),
        };
        let work = Work::Synchronize {
            items: &items,
            local: Path::new(&chosen.local),
            remote: &chosen.remote,
            timestamps: chosen.settings.timestamps,
        };
        let params = crate::options::copy_params(&self.host);
        let _ = self.transfer(instance, shown, params, work);
        KeyAnswer {
            reread: true,
            reread_passive: true,
            ..KeyAnswer::default()
        }
    }
}
