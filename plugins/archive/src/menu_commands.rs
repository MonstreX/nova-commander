//! The Plugins menu's commands of the archive plugin (arclite's `OPEN_PLUGINSMENU`): "Open archive",
//! "Create archive" (and Shift+F1's new archive), "Extract archive(s)", "Test archive(s)" and "Convert
//! archive into SFX", with what they look at on the panels.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Error, ErrorKind, PutRequest, Which};

use crate::{ArchivePlugin, NOT_ARCHIVE, bulk, create_options, creating};

impl ArchivePlugin {
    /// The Plugins menu's "Open archive" (`OPEN_PLUGINSMENU`): the file under the active panel's cursor
    /// on a disk, whatever its name.
    pub(crate) fn open_current(&mut self, every: bool) -> Result<Option<String>, String> {
        let Some(path) = self.current_file() else {
            return Ok(None);
        };
        let how = crate::loading::How {
            every,
            ..crate::loading::How::default()
        };
        match self.open_with(&path, &how) {
            Ok(Some(instance)) => Ok(Some(instance)),
            Ok(None) => Err(NOT_ARCHIVE.to_string()),
            Err(error) if error.kind == nc_plugin_sdk::ErrorKind::Cancelled => Ok(None),
            Err(error) => Err(error.message),
        }
    }

    /// The Plugins menu's "Create archive" (arclite's `create_archive`): the active panel's selected
    /// entries on a disk (the one under the cursor when none is), then the cursor on the archive in
    /// each panel showing its folder.
    pub(crate) fn create_from_panel(&mut self) -> Result<(), String> {
        let Some(state) = self.host.panel(Which::Active) else {
            return Ok(());
        };
        if state.plugin.is_some() {
            return Ok(());
        }
        let folder = PathBuf::from(&state.folder);
        let sources: Vec<PathBuf> = (self.host.selected_items(Which::Active).iter())
            .filter(|item| !item.parent)
            .map(|item| folder.join(&item.name))
            .collect();
        let library = self.library().clone();
        let request = creating::Request {
            folder: &folder,
            sources: &sources,
            asked: create_options::Asked::Menu,
            library: Some(&library),
        };
        let host = self.host;
        let mut place = |temp: &Path, archive: &Path| self.swap_in(archive, temp);
        match creating::create(host, &request, &mut place) {
            Ok(Some(archive)) => {
                self.show_made(&archive, &[Which::Active, Which::Passive]);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) if error.kind == ErrorKind::Cancelled => Ok(()),
            Err(error) => Err(error.message),
        }
    }

    /// arclite's `put_files` into a panel of no archive yet (Shift+F1): the "Create archive" dialog
    /// for the sources, then the cursor on the archive in the active panel showing its folder.
    pub(crate) fn put_new(&mut self, request: &PutRequest) -> Result<(), Error> {
        let first_folder = request.sources.first().and_then(|source| source.parent());
        let folder = (request.folder.as_deref())
            .or(first_folder)
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let library = self.library().clone();
        let create = creating::Request {
            folder: &folder,
            sources: &request.sources,
            asked: create_options::Asked::Put,
            library: Some(&library),
        };
        let host = self.host;
        let mut place = |temp: &Path, archive: &Path| self.swap_in(archive, temp);
        if let Some(archive) = creating::create(host, &create, &mut place)? {
            self.show_made(&archive, &[Which::Active]);
        }
        Ok(())
    }

    /// The Plugins menu's "Convert archive into SFX" (arclite's `convert_to_sfx`): the active panel's
    /// selected files of real names, folders passed over; nothing without one.
    pub(crate) fn convert_to_sfx(&self) {
        let Some((_, files)) = bulk::panel_archives(&self.host) else {
            return;
        };
        let modules = crate::sfx::Modules(self.library().sfx_modules());
        crate::sfx_convert::convert_all(self.host, &files, &modules);
    }

    /// The Plugins menu's "Extract archive(s)" (`extracting`) or "Test archive(s)" (arclite's
    /// `bulk_extract`, `bulk_test`): the active panel's selected files of real names, folders passed
    /// over; nothing without one.
    pub(crate) fn bulk(&self, extracting: bool) -> Result<(), String> {
        let Some((folder, archives)) = bulk::panel_archives(&self.host) else {
            return Ok(());
        };
        let load = |path: &Path| self.load(path);
        let done = if extracting {
            bulk::extract_all(self.host, (&folder, &archives), &load)
        } else {
            bulk::test_all(self.host, &archives, &load)
        };
        match done {
            Err(error) if error.kind == ErrorKind::Cancelled => Ok(()),
            other => other.map_err(|error| error.message),
        }
    }

    /// The cursor on the archive made in each of `panels` showing its folder (arclite's
    /// `panel_go_to_file`: the folder read again there).
    pub(crate) fn show_made(&self, archive: &Path, panels: &[Which]) {
        let (Some(folder), Some(name)) = (archive.parent(), archive.file_name()) else {
            return;
        };
        let folder = folder.display().to_string();
        let name = name.to_string_lossy();
        for &which in panels {
            let there = self.host.panel(which).is_some_and(|state| {
                state.plugin.is_none()
                    && state.folder.trim_end_matches('\\').to_lowercase()
                        == folder.trim_end_matches('\\').to_lowercase()
            });
            if there {
                self.host.set_folder(which, &folder, Some(&name));
            }
        }
    }

    /// The file under the active panel's cursor when the panel shows a disk's folder.
    pub(crate) fn current_file(&self) -> Option<PathBuf> {
        let state = self.host.panel(Which::Active)?;
        if state.plugin.is_some() {
            return None;
        }
        let items = self.host.panel_items(Which::Active);
        let item = items.get(state.current)?;
        (!item.folder && !item.parent).then(|| Path::new(&state.folder).join(&item.name))
    }
}
