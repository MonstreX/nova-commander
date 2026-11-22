//! What the results dialog gives of the objects found: the file F3 / F4 open, the found files for the
//! viewer's Gray + / Gray -, and FAR's "Panel"'s entries (an archive's file for what was found in it).

use super::{FindResults, ResultLine};
use crate::jobs::find::Found;
use crate::vfs::VfsPath;

/// What F3 / F4 open of a found file: a plugin's by its real name when it has one (FAR's
/// `OPIF_REALNAMES`), else its own path (in an archive, its place inside the archive's file).
fn opened_path(found: &Found) -> VfsPath {
    (found.entry.real_path.as_deref()).map_or_else(|| found.path(), VfsPath::local)
}

/// The archive's file and place of an object found in an archive without a real name (FAR's
/// `Arc && !(Arc->Flags & OPIF_REALNAMES)`).
fn in_archive(item: &Found) -> Option<(&VfsPath, &str)> {
    (item.entry.real_path.is_none())
        .then(|| item.folder.file_path())
        .flatten()
}

impl FindResults {
    /// The found file under the cursor, a plugin's by its real name when it has one; none on a folder.
    pub(super) fn current_file(&self) -> Option<VfsPath> {
        match self.lines.get(self.cursor) {
            Some(ResultLine::Found(at)) if !self.found[*at].entry.is_dir => {
                Some(opened_path(&self.found[*at]))
            }
            _ => None,
        }
    }

    /// FAR's `FD_BUTTON_PANEL`: the found files, folders only when they were looked for (not in hex),
    /// each by its whole path on a disk - a plugin's entry by its real name (FAR's `OPIF_REALNAMES`), one
    /// without it left out.
    /// Each with what the search saw of it, named by its whole path, which is its real name (FAR's
    /// `FD_BUTTON_PANEL` hands the search's `PluginPanelItem`s). One found in an archive without a
    /// real name is the archive's file, with no attributes, whatever it is (FAR: "the panel removes
    /// the twins"); one with a real name is itself.
    pub fn panel_entries(&self) -> Vec<crate::vfs::Entry> {
        (self.found.iter())
            .filter(|item| in_archive(item).is_some() || !item.entry.is_dir || self.panel_folders)
            .filter_map(|item| {
                if let Some((archive, _)) = in_archive(item) {
                    let path = archive.as_local()?.to_path_buf();
                    return Some(crate::vfs::Entry {
                        name: path.display().to_string(),
                        real_path: Some(path),
                        is_dir: false,
                        attributes: Default::default(),
                        ..item.entry.clone()
                    });
                }
                let path = (item.entry.real_path.clone())
                    .or_else(|| item.path().as_local().map(std::path::Path::to_path_buf))?;
                Some(crate::vfs::Entry {
                    name: path.display().to_string(),
                    real_path: Some(path),
                    ..item.entry.clone()
                })
            })
            .collect()
    }

    /// The found files, for the viewer's Gray + / Gray - (FAR's `ViewList`).
    pub fn found_files(&self) -> Vec<VfsPath> {
        (self.found.iter())
            .filter(|item| !item.entry.is_dir)
            .map(opened_path)
            .collect()
    }
}
