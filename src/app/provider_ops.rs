//! The panel's own operations on a provider's folder (a plugin's panel, ADR-0016 §13, 5c-2): F7's folder
//! made through its provider on a worker (a plugin answers on its own thread), the panel read again on
//! the new folder. Copying, moving and deleting go through the jobs, which already work by provider.

use std::io;

use super::App;
use crate::vfs::{self, VfsPath};

impl App {
    /// F7 on a provider's panel: `name` made in its folder, a nested `a\b` (or `a/b`) as FAR's
    /// `ShellMakeDir` makes it (P6.0d): the folders on the way that are there taken, the last one made
    /// (one that is there: the error), a leading separator from the panel's root. The cursor goes on the
    /// first one.
    pub(super) fn make_dir_mounted(&mut self, names: &[String]) {
        let folder = self.active_panel().path.clone();
        // Each name: where it starts and its folders (F7's several names, R7 review D4).
        let mut all = Vec::new();
        for name in names {
            let name = name.trim();
            let parts: Vec<String> = (name.split(['\\', '/']))
                .filter(|part| !part.is_empty())
                .map(ToString::to_string)
                .collect();
            let base = match name.starts_with(['\\', '/']) {
                true => folder.root(),
                false => folder.clone(),
            };
            if let Some(wrong) = parts.iter().find(|part| !base.accepts_name(part)) {
                let error = io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{wrong} cannot be one folder's name here"),
                );
                return self.show_error("Make directory", &error);
            }
            if !parts.is_empty() {
                all.push((base, parts));
            }
        }
        let Some((base, first)) = all
            .last()
            .map(|(base, parts)| (base.clone(), parts[0].clone()))
        else {
            return;
        };
        // A plugin's call: Esc asks it to stop (review R14).
        self.wait_for_stoppable(Vec::new(), move || {
            let made = (all.iter()).try_for_each(|(made_in, parts)| make_nested(made_in, parts));
            Box::new(move |app: &mut App| match made {
                Err(error) if vfs::stop::is_stopped(&error) => app.show_stopped(),
                Ok(()) => {
                    app.reread_panels();
                    let panel = app.active_panel_mut();
                    if panel.path == folder
                        && base == folder
                        && let Err(error) = panel.change_dir(&folder, Some(&first))
                    {
                        app.show_error("Make directory", &error);
                    }
                }
                Err(error) => {
                    // The folders made before the failure shown (FAR's `Update` after its loop).
                    app.reread_panels();
                    app.show_error("Make directory", &error);
                }
            })
        });
        self.label_waiting("Making the folder…");
    }
}

/// The folders `parts` made one in another in `base` (FAR's `ShellMakeDir`): one on the way that is
/// there (a folder) is taken, the last is always made.
fn make_nested(base: &VfsPath, parts: &[String]) -> io::Result<()> {
    let provider = vfs::provider_for(base)?;
    let mut path = base.clone();
    for (index, part) in parts.iter().enumerate() {
        path = path.join(part);
        let last = index + 1 == parts.len();
        if !last && provider.stat(&path).is_ok_and(|entry| entry.is_dir) {
            continue;
        }
        provider.make_dir(&path)?;
    }
    Ok(())
}
