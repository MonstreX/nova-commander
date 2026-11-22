//! Go to on what "Search in archives" found (FAR's `FD_BUTTON_GOTO` with its `ArcListItem`): the panel to
//! the archive's file, the archive opened over it by the plugin that found it, then the found object's
//! folder inside with the cursor on it. F3 / F4 copy it from the archive opened again (`extracted`).

use super::super::App;
use crate::vfs::VfsPath;

impl App {
    /// Go to on `folder` (and `name` in it) when it is a place inside an archive's file
    /// (`provider:archive!/path`); false for any other place.
    pub(super) fn go_to_in_archive(&mut self, folder: &VfsPath, name: Option<&str>) -> bool {
        let Some((archive, inner)) = folder.file_path() else {
            return false;
        };
        let (Some(plugin), Some(file)) = (folder.provider(), archive.as_local()) else {
            return true;
        };
        let (Some(parent), Some(archive_name)) = (archive.parent(), archive.file_name()) else {
            return true;
        };
        let (plugin, file) = (plugin.to_string(), file.to_path_buf());
        let inner = inner.to_string();
        self.go_to_found(&parent, Some(&archive_name));
        // The panel did not get there (its error said why): nothing opened.
        let side = self.command_side();
        if self.active != side || !self.panels[side.index()].path.same_place(&parent) {
            return true;
        }
        let place = (inner, name.map(str::to_string));
        self.open_found_in_archive(&plugin, &file, place);
        true
    }
}
