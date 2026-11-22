//! F3 / F4 on a file of a panel that is no disk's (a plugin's; an archive's with stage 6), as FAR's (its
//! `GetFiles` with `OPM_VIEW` / `OPM_EDIT`, review R55): the file is copied to a folder of the program's
//! own in the temporary folder, the viewer or the editor opens the copy; once they are closed a copy
//! edited goes back to its panel (FAR's `PutFiles`) and the folder goes. A copy that cannot go back stays
//! where it is, and the error says where. Copying either way is stopped by Esc (ADR-0019).

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use super::{App, Screen};
use crate::vfs::{self, VfsPath, WriteMode};

/// The window while the file is copied.
const EXTRACTING: &str = "Copying the file\u{2026}";

/// The window while the edited copy goes back.
const PUTTING_BACK: &str = "Putting the file back\u{2026}";

/// The size of one piece copied, between looks at Esc.
const PIECE: usize = 1 << 20;

/// A copy the viewer or the editor has open.
#[derive(Debug)]
pub(super) struct Extracted {
    /// The program's folder that holds it.
    folder: PathBuf,
    copy: PathBuf,
    /// Where it came from.
    origin: VfsPath,
    edit: bool,
    /// The copy as it was made: changed since, it was saved.
    made: (Option<SystemTime>, u64),
}

/// The copy's time and length.
fn stamp(copy: &Path) -> (Option<SystemTime>, u64) {
    std::fs::metadata(copy).map_or((None, 0), |meta| (meta.modified().ok(), meta.len()))
}

/// `reader` into `writer` piece by piece, Esc looked at between them (`vfs::stop`).
fn copy_stoppable(reader: &mut dyn Read, writer: &mut dyn Write) -> io::Result<()> {
    let stop = vfs::stop::current();
    let mut piece = vec![0; PIECE];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(vfs::stop::stopped());
        }
        let read = reader.read(&mut piece)?;
        if read == 0 {
            return writer.flush();
        }
        writer.write_all(&piece[..read])?;
    }
}

/// A new folder of the program's in the temporary folder: made new, never one there already (R7
/// review C5: an earlier run's folder with the same process number - a kept edited copy in it - was
/// taken over, its copy overwritten, opened as the new one, and removed with it).
pub(super) fn new_folder() -> io::Result<PathBuf> {
    new_folder_in(&std::env::temp_dir())
}

fn new_folder_in(temp: &Path) -> io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        let folder = temp.join(format!("nova-commander-{}-{number}", std::process::id()));
        match std::fs::create_dir(&folder) {
            Ok(()) => return Ok(folder),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

/// `origin` copied into a new folder of the program's under its own name: the folder and the copy.
fn extract(origin: &VfsPath, name: &str) -> io::Result<(PathBuf, PathBuf)> {
    let provider = vfs::provider_for(origin)?;
    let mut reader = provider.open_read(origin)?;
    let folder = new_folder()?;
    // The name as the panel shows it: a part of a path, nothing more.
    let copy = folder.join(Path::new(name).file_name().unwrap_or(name.as_ref()));
    let copied =
        std::fs::File::create(&copy).and_then(|mut file| copy_stoppable(&mut reader, &mut file));
    if let Err(error) = copied {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(error);
    }
    Ok((folder, copy))
}

/// `place` (`provider:archive!/path`, a place inside an archive's file) copied as [`extract`] does from
/// the archive opened by `opener`, closed after.
fn extract_from_archive(
    opener: &dyn vfs::FileOpener,
    place: &VfsPath,
    name: &str,
) -> io::Result<(PathBuf, PathBuf)> {
    let archive = (place.file_path()).and_then(|(archive, _)| archive.as_local());
    let not_opened = || {
        let shown = archive.map_or_else(|| place.to_string(), |file| file.display().to_string());
        io::Error::new(io::ErrorKind::NotFound, format!("Cannot open {shown}"))
    };
    let archive = archive.ok_or_else(not_opened)?;
    let stop = vfs::stop::current();
    if stop.load(Ordering::Relaxed) {
        return Err(vfs::stop::stopped());
    }
    let opened = opener.open(archive, &stop).ok_or_else(|| {
        if stop.load(Ordering::Relaxed) {
            vfs::stop::stopped()
        } else {
            not_opened()
        }
    })?;
    extract(&opened.session_of(place), name)
}

/// A name beside `origin` no entry has: `<name>.nc-new`, then numbered.
fn free_name(provider: &dyn vfs::Provider, origin: &VfsPath) -> io::Result<VfsPath> {
    let name = origin.file_name().unwrap_or_default();
    let parent = origin.parent().unwrap_or_else(|| origin.root());
    for number in 0..100 {
        let fresh = match number {
            0 => parent.join(&format!("{name}.nc-new")),
            _ => parent.join(&format!("{name}.nc-new{number}")),
        };
        match provider.stat(&fresh) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(fresh),
            Err(error) => return Err(error),
            Ok(_) => {}
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("No free name beside {origin}"),
    ))
}

/// The edited `copy` back to `origin`: written under a name beside it no entry had, then put in its
/// place (the original stays whole until the new one is); what this made and did not place goes.
fn put_back(copy: &Path, origin: &VfsPath) -> io::Result<()> {
    let provider = vfs::provider_for(origin)?;
    let fresh = free_name(provider.as_ref(), origin)?;
    let mut reader = std::fs::File::open(copy)?;
    // Made here, or it failed before anything was made (`New` never takes an existing entry).
    let mut writer = provider.open_write(&fresh, WriteMode::New)?;
    let written = copy_stoppable(&mut reader, &mut writer);
    drop(writer);
    let placed = written.and_then(|()| provider.rename(&fresh, origin, true));
    if placed.is_err() {
        // The stop that came must not break the cleanup.
        let _ = vfs::stop::unstopped(|| provider.remove(&fresh, vfs::Removal::File));
    }
    placed
}

impl App {
    /// F3 (`edit` false) or F4 on the file under the cursor of a panel that is no disk's: copied, then
    /// opened (true); false when the cursor is on none such.
    pub(super) fn open_extracted(&mut self, edit: bool) -> bool {
        let panel = self.active_panel();
        if panel.path.as_local().is_some() {
            return false;
        }
        let Some(entry) = panel
            .current()
            .filter(|entry| !entry.is_dir && !entry.is_parent && entry.real_path.is_none())
        else {
            return false;
        };
        let (name, origin) = (entry.name.clone(), panel.path.join(&entry.name));
        // A plugin that writes its files out (FAR's `GetFile` with `OPM_VIEW`): the copy is its.
        if !self.get_to_view(origin.clone(), name.clone(), edit) {
            self.open_copy_of(origin, name, edit);
        }
        true
    }

    /// The file a plugin wrote into the program's folder for F3 / F4 (FAR's `GetFile`: the folder's first
    /// file) viewed or edited; the folder goes when it holds none.
    pub(super) fn open_got(&mut self, view: super::plugins::GetView) {
        let listed = std::fs::read_dir(&view.folder).into_iter().flatten();
        let first = (listed.flatten())
            .find(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .map(|entry| entry.path());
        let Some(copy) = first else {
            let _ = std::fs::remove_dir_all(&view.folder);
            return;
        };
        self.open_copy(Extracted {
            made: stamp(&copy),
            folder: view.folder,
            copy,
            origin: view.origin,
            edit: view.edit,
        });
    }

    /// The file `origin` (named `name`) of a panel that is no disk's copied, then viewed or (`edit`)
    /// edited: F3 / F4 on a panel, or on a plugin's file Find file found.
    /// One found in an archive (`provider:archive!/path`) is copied from the archive opened again for it
    /// without any window by the plugin that found it, closed after (FAR's `GetPluginFile` with
    /// `OpenFilePlugin(OFP_SEARCH)`).
    pub(super) fn open_copy_of(&mut self, origin: VfsPath, name: String, edit: bool) {
        let title = if edit { "Edit" } else { "View" };
        let opener = (origin.file_path()).map(|_| self.archive_opener(origin.provider()));
        self.wait_for_stoppable(vec![EXTRACTING.to_string()], move || {
            let extracted = match &opener {
                Some(opener) => extract_from_archive(opener.as_ref(), &origin, &name),
                None => extract(&origin, &name),
            };
            Box::new(move |app: &mut App| match extracted {
                Ok((folder, copy)) => app.open_copy(Extracted {
                    made: stamp(&copy),
                    folder,
                    copy,
                    origin,
                    edit,
                }),
                Err(error) if vfs::stop::is_stopped(&error) => app.show_stopped(),
                Err(error) => app.show_error(title, &error),
            })
        });
    }

    /// Shift+F4's new file `name` on a plugin's panel (FAR's temporary file with `FFILEEDIT_CANNEWFILE`):
    /// edited as a copy not there yet, which goes to the panel once saved (its first save makes it).
    pub(super) fn edit_new_copy(&mut self, name: &str) -> io::Result<()> {
        let panel = self.active_panel().path.clone();
        if !panel.accepts_name(name) {
            let error = format!("\"{name}\" is not a name of a file in the panel");
            return Err(io::Error::new(io::ErrorKind::InvalidInput, error));
        }
        let origin = panel.join(name);
        let folder = new_folder()?;
        let copy = folder.join(name);
        self.extracted.push(Extracted {
            folder,
            copy: copy.clone(),
            origin,
            edit: true,
            made: (None, 0),
        });
        self.open_new_file(&copy);
        Ok(())
    }

    fn open_copy(&mut self, extracted: Extracted) {
        let copy = extracted.copy.clone();
        let edit = extracted.edit;
        self.extracted.push(extracted);
        if edit {
            self.edit_file(&copy, false);
        } else {
            self.view_file(&copy, false);
        }
    }

    /// Whether `path` is a plugin's file's copy (viewed or edited).
    pub(super) fn is_extracted(&self, path: &Path) -> bool {
        (self.extracted.iter()).any(|copy| copy.copy == path)
    }

    /// Whether `path` is a plugin's file's copy opened for editing: its editor is modal, as FAR's
    /// (`ExecuteModal`), so that it goes back once the editor closes.
    pub(super) fn edits_extracted(&self, path: &Path) -> bool {
        (self.extracted.iter()).any(|copy| copy.edit && copy.copy == path)
    }

    /// Once the panels are back: a copy edited goes back to its panel, its folder goes - once no
    /// viewer or editor behind has it open. True on a change.
    pub(super) fn pump_extracted(&mut self) -> bool {
        if !matches!(self.screen, Screen::Panels) || self.extracted.is_empty() || self.is_waiting()
        {
            return false;
        }
        let closed = (self.extracted.iter()).rposition(|done| !self.window_open_on(&done.copy));
        let Some(at) = closed else {
            return false;
        };
        // Its save's put not ended yet (the editor's "Save" as it closed): its end tells what was taken.
        if self.saving_put(&self.extracted[at].copy) {
            return false;
        }
        let done = self.extracted.remove(at);
        if !done.edit || stamp(&done.copy) == done.made {
            let _ = std::fs::remove_dir_all(&done.folder);
            return false;
        }
        if done.origin.file_path().is_some() {
            // FAR's `SetSaveToSaveAs` for a file found in an archive: never put back; the work is kept.
            let lines = vec![
                "A file found in an archive is not put back.".to_string(),
                format!("The edited copy is kept: {}", done.copy.display()),
            ];
            self.show_dialog(super::dialog::Dialog::message("Edit", lines));
            return true;
        }
        // A panel that takes paths takes the copy back itself (FAR's `PutFiles` with `OPM_EDIT`).
        let target = (done.origin.parent()).unwrap_or_else(|| done.origin.root());
        if (self.panels.iter()).any(|panel| panel.path == target && panel.look.takes_paths) {
            self.put_edited_back(&target, done.folder, done.copy);
            return true;
        }
        self.wait_for_stoppable(vec![PUTTING_BACK.to_string()], move || {
            let put = put_back(&done.copy, &done.origin);
            Box::new(move |app: &mut App| match put {
                Ok(()) => {
                    let _ = std::fs::remove_dir_all(&done.folder);
                    app.reread_panels();
                }
                Err(error) => {
                    // The work is kept: the copy stays, and the error says where.
                    let kept = io::Error::new(
                        error.kind(),
                        format!("{error}\nThe edited copy is kept: {}", done.copy.display()),
                    );
                    app.show_error("Edit", &kept);
                }
            })
        });
        true
    }

    /// The editor saved `saved`: a copy of a file of a panel that takes paths and puts at each save (API
    /// 8; FAR's NetBox's upload at `EE_SAVE`) goes back now, the editor and the copy staying. Once taken
    /// it counts as made then: leaving the editor puts it back again only when saved since.
    pub(super) fn put_on_save(&mut self, saved: &Path) {
        let found = (self.extracted.iter()).find(|done| done.edit && done.copy == saved);
        let Some(done) = found else {
            return;
        };
        if done.origin.file_path().is_some() {
            return;
        }
        let target = (done.origin.parent()).unwrap_or_else(|| done.origin.root());
        let puts = |panel: &super::Panel| {
            panel.path == target && panel.look.takes_paths && panel.look.puts_on_save
        };
        if !self.panels.iter().any(puts) {
            return;
        }
        let (folder, copy) = (done.folder.clone(), done.copy.clone());
        let made = stamp(&copy);
        self.put_saved_back(&target, folder, copy, made);
    }

    /// A copy put back at a save was taken: as made at `made` now.
    pub(in crate::app) fn saved_copy_taken(
        &mut self,
        copy: &Path,
        made: (Option<SystemTime>, u64),
    ) {
        if let Some(done) = (self.extracted.iter_mut()).find(|done| done.copy == copy) {
            done.made = made;
        }
    }

    /// The program's folders of the copies the panels' plugins' panels were opened from (an archive
    /// inside an archive): deleted by [`super::remove_copies_now`] as the program ends, once its plugins
    /// have ended and let them go.
    pub fn plugin_copies(&self) -> Vec<PathBuf> {
        (self.panels.iter())
            .flat_map(super::Panel::plugin_copies)
            .collect()
    }

    /// The program ends: copies not edited go (an edited one not put back stays).
    pub fn close_extracted(&mut self) {
        for done in self.extracted.drain(..) {
            if !done.edit || stamp(&done.copy) == done.made {
                let _ = std::fs::remove_dir_all(&done.folder);
            }
        }
    }
}

#[cfg(test)]
#[path = "extracted_tests.rs"]
mod tests;
