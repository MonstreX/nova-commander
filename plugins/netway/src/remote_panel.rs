//! A connected session's panel at work: its folders read over SFTP (Esc stops a reading), the folder it
//! opens at reached once the program has read its root (as the network plugin's start: the program opens a
//! plugin's panel at its root, so that root is answered empty at once and a synchro takes the panel on),
//! the connection closed with its panel.

use nc_plugin_sdk::{Error, ErrorKind, FileEntry, PanelState, Which};

use crate::NetWay;

/// The plugin's name as the program's panels report it.
const NAME: &str = "netway";

/// The panel to take to its start: one of the plugin's at its root, the active one first.
pub fn target_panel(panels: &[(Which, Option<PanelState>)]) -> Option<Which> {
    let at_root = |state: &PanelState| {
        let ours = (state.plugin.as_deref()).is_some_and(|name| name.eq_ignore_ascii_case(NAME));
        ours && state.folder == "/"
    };
    let mut found: Vec<(bool, Which)> = (panels.iter())
        .filter_map(|(which, state)| {
            let state = state.as_ref().filter(|state| at_root(state))?;
            Some((!state.active, *which))
        })
        .collect();
    found.sort_by_key(|(passive, _)| *passive);
    found.first().map(|(_, which)| *which)
}

impl NetWay {
    /// The remote folder `path` of the panel `instance`.
    pub(crate) fn list_remote(
        &mut self,
        instance: &str,
        path: &str,
    ) -> Result<Vec<FileEntry>, Error> {
        if !self.reopen_silently(instance, path) {
            return Err(Error::new(ErrorKind::Other, crate::reopen::LOST));
        }
        let Some(remote) = self.remotes.get(instance) else {
            let text = format!("{instance}: the session is no longer connected");
            return Err(Error::new(ErrorKind::NotFound, text));
        };
        if remote.start.is_some() && path == "/" {
            self.host.synchro().ask(remote.number);
            return Ok(Vec::new());
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return Err(Error::new(ErrorKind::Other, "No connection"));
        };
        let host = self.host;
        let listed = runtime
            .wait(remote.fs.list(path), &|| host.cancelled())
            .map_err(|_| Error::new(ErrorKind::Cancelled, "Cancelled"))?
            .map_err(|error| Error::new(ErrorKind::Other, format!("{path}: {error}")))?;
        // The first reading sets the panel's folder; its moves are the folders its title shows (`seen`),
        // not the ones Find or a folder's size read.
        if let Some(remote) = self.remotes.get_mut(instance)
            && remote.folder.is_empty()
        {
            remote.moved_to(path);
        }
        Ok(listed.iter().map(crate::remote::entry).collect())
    }

    /// The synchro of the panel numbered `number`: taken to the folder it opens at; the other panel
    /// following its move (synchronized browsing).
    pub(crate) fn go_to_start(&mut self, number: u64) {
        let Some((instance, remote)) =
            (self.remotes.iter_mut()).find(|(_, remote)| remote.number == number)
        else {
            return;
        };
        if let Some(now) = remote.seen.get_mut().take() {
            let before = remote.folder.clone();
            remote.moved_to(&now);
            let unfollowed = remote.take_unfollowed(&now);
            if remote.synchronized && before != now && !unfollowed {
                let instance = instance.clone();
                self.follow(&instance, &before, &now);
            }
            return;
        }
        let Some(start) = remote.start.take() else {
            return;
        };
        let panels: Vec<(Which, Option<PanelState>)> = [Which::Active, Which::Passive]
            .into_iter()
            .map(|which| (which, self.host.panel(which)))
            .collect();
        if let Some(which) = target_panel(&panels) {
            // Refused, the panel stays at the root and the program tells why.
            let _ = self.host.set_folder(which, &start, None);
        }
    }

    /// WinSCP's `UpdateDirectories` ("Remember last used directory"): the stored session opens next time
    /// at the folder its panel had.
    fn remember_folder(&self, remote: &crate::remote::Remote) {
        let (Some(stored), false) = (&remote.stored, remote.folder.is_empty()) else {
            return;
        };
        if !remote.plan.options.update_directories {
            return;
        }
        let Ok(loaded) = self.store.load(stored) else {
            return;
        };
        let mut session = loaded.session;
        let mut options = session.options();
        if options.remote_directory == remote.folder {
            return;
        }
        options.remote_directory = remote.folder.clone();
        session.set_options(&options);
        let _ = self.store.save(stored, &session);
    }

    /// The panel `instance` closed: its connection too, without waiting for the server.
    pub(crate) fn disconnect(&mut self, instance: &str) {
        let Some(remote) = self.remotes.remove(instance) else {
            return;
        };
        self.remember_folder(&remote);
        if let Some(runtime) = self.runtime.as_ref() {
            runtime.spawn(async move {
                if let Some(connection) = &remote.connection {
                    connection.close().await;
                }
                drop(remote.fs);
            });
        }
    }
}

#[cfg(test)]
#[path = "remote_panel_tests.rs"]
mod tests;
