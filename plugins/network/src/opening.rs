//! Where a panel just opened goes (FAR's `NetBrowser` constructor, `GotoLocalNetwork`, `GotoComputer`,
//! `SetCursorToShare`): the folder asked for, else where the last panel closed, else - FAR's first panel
//! only (`IsFirstRun`) - this computer's workgroup, when that answers. The program opens a plugin's panel
//! at its root: once the root of that panel is read, a synchro (carrying the panel's instance) takes it
//! there, the cursor on the share named, whatever its case.

use nc_plugin_sdk::system::network::NetResource;
use nc_plugin_sdk::{PanelState, Which};

use crate::browse::{Network, name_of};
use crate::options::Options;
use crate::{NAME, NetworkPlugin, start, timed};

/// A panel just opened goes to `path`, the cursor on `cursor`; with `check`, only when that resource
/// answers its listing (this computer's workgroup, opened unasked).
#[derive(Debug, Clone)]
pub struct Pending {
    pub instance: String,
    pub path: String,
    pub cursor: Option<String>,
    pub check: Option<NetResource>,
}

/// The panel to take there: a panel of the plugin's at its root, the active one first.
pub fn target_panel(panels: &[(Which, Option<PanelState>)]) -> Option<Which> {
    let at_root = |state: &PanelState| state.plugin.as_deref() == Some(NAME) && state.folder == "/";
    let mut found: Vec<(bool, Which)> = (panels.iter())
        .filter_map(|(which, state)| {
            let state = state.as_ref().filter(|state| at_root(state))?;
            Some((!state.active, *which))
        })
        .collect();
    found.sort_by_key(|(passive, _)| *passive);
    found.first().map(|(_, which)| *which)
}

/// The name of `wanted` among `listed`, whatever its case (FAR's `SetCursorToShare`).
pub fn cursor_name(listed: &[NetResource], options: &Options, wanted: &str) -> String {
    (listed.iter())
        .map(|one| name_of(one, options))
        .find(|name| name.eq_ignore_ascii_case(wanted))
        .unwrap_or_else(|| wanted.to_string())
}

impl NetworkPlugin {
    /// A new panel: at `start`, else where the last one closed, else (the first panel only) at this
    /// computer's workgroup.
    pub(crate) fn open(&mut self, start: Option<(String, Option<String>)>) -> String {
        self.opened += 1;
        let instance = self.opened.to_string();
        self.instances
            .insert(instance.clone(), crate::Instance::default());
        let first = std::mem::replace(&mut self.first_run, false);
        let start = start.or_else(|| self.last.clone().map(|path| (path, None)));
        let pending = match start {
            Some((path, cursor)) => Some(Pending {
                instance: instance.clone(),
                path,
                cursor,
                check: None,
            }),
            None if first && self.options.local_network => {
                let favorites = self.favorites.borrow().clone();
                start::local_network(&self.net, &self.options, &favorites).map(
                    |(path, resource)| {
                        self.browser.remember(&path, resource.clone());
                        // The Favorites folder answers at once: nothing to wait for.
                        let check = (!crate::favorites::is_folder(&resource)).then_some(resource);
                        Pending {
                            instance: instance.clone(),
                            path,
                            cursor: None,
                            check,
                        }
                    },
                )
            }
            None => None,
        };
        self.pending = pending.filter(|pending| pending.path != "/");
        instance
    }

    /// The panel `instance` read at `path`: at its root with a folder to go to, the synchro asked.
    pub(crate) fn opened_at(&self, instance: &str, path: &str) {
        let waits = (self.pending.as_ref()).is_some_and(|pending| pending.instance == instance);
        if waits && path == "/" {
            self.host.synchro().ask(instance.parse().unwrap_or(0));
        }
    }

    /// The synchro of the panel `instance`: taken to its folder.
    pub(crate) fn go_to_start(&mut self, instance: u64) {
        // Another panel's (an older one's late synchro): the pending start stays the newer panel's.
        let ours =
            (self.pending.as_ref()).is_some_and(|pending| pending.instance == instance.to_string());
        if !ours {
            return;
        }
        let Some(pending) = self.pending.take() else {
            return;
        };
        let panels: Vec<(Which, Option<PanelState>)> = [Which::Active, Which::Passive]
            .into_iter()
            .map(|which| (which, self.host.panel(which)))
            .collect();
        let Some(which) = target_panel(&panels) else {
            return;
        };
        if let Some(check) = &pending.check {
            match self.net.enumerate_within(Some(check), timed::CHECK_WAIT) {
                Ok(listed) => self.net.keep(Some(check), Ok(listed)),
                Err(_) => return,
            }
        }
        let cursor = pending
            .cursor
            .as_ref()
            .map(|wanted| self.share_name(&pending.path, wanted));
        // Refused, the panel stays where it is and the program tells why: nothing more to do here.
        let _ = (self.host).set_folder(which, &pending.path, cursor.as_deref());
    }

    /// The share `wanted` of the computer at `path` as its listing names it; the listing kept for the
    /// panel's reading that follows.
    fn share_name(&mut self, path: &str, wanted: &str) -> String {
        let resource = self
            .browser
            .resolve(
                &crate::favorites::favored(&self.net, &self.favorites.borrow()),
                &self.options,
                path,
            )
            .ok()
            .flatten();
        let Some(resource) = resource else {
            return wanted.to_string();
        };
        match self.net.enumerate(Some(&resource)) {
            Ok(listed) => {
                let shown = crate::browse::with_hidden_shares(
                    &self.net,
                    &self.options,
                    Some(&resource),
                    listed.clone(),
                );
                let name = cursor_name(&shown, &self.options, wanted);
                self.net.keep(Some(&resource), Ok(listed));
                name
            }
            Err(_) => wanted.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "opening_tests.rs"]
mod tests;
