//! A new archive as arclite makes it (`create_archive` from the Plugins menu, `put_files` into a panel of
//! no archive yet; `Archive::create`, update.cpp): the "Create archive" dialog for the sources of a
//! disk's folder, "Target file already exists. Do you want to overwrite it?" for a file there, its
//! folder made, the sources walked into it, the archive written with "{N%} Creating archive...", the
//! sources moved deleted (nothing failed or was skipped) and the log of failures offered.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{ButtonSet, Error, ErrorKind, Host, Message};

use crate::asking::{HostAsks, fault_lines};
use crate::create_dialog::CreateDialog;
use crate::create_formats::Archivers;
use crate::create_options::{Asked, NewArchive, NewSettings, Profiles, default_name};
use crate::extract::{Abort, Fault};
use crate::extract_options::Overwrite;
use crate::options::TITLE;
use crate::tree::Tree;
use crate::update::{Existing, Faults, prepare_in, under_folder};
use crate::update_options::{UpdateOptions, UpdateSettings};
use crate::update_write::{Stop, delete_sources};

/// What to make an archive of: the disk's folder the sources are in and the sources; 7-Zip's library
/// when it is found (its formats offered, every one but zip written through it).
pub struct Request<'a> {
    pub folder: &'a Path,
    pub sources: &'a [PathBuf],
    pub asked: Asked,
    pub library: Option<&'a nc_plugin_sdk::sevenzip::Library>,
}

fn stopped() -> Error {
    Error::new(ErrorKind::Cancelled, "Stopped")
}

fn failed(fault: &Fault) -> Error {
    Error::new(ErrorKind::Other, fault_lines(fault).join("\n"))
}

/// The sources' names as the dialog names the archive after them.
fn names_of(sources: &[PathBuf]) -> Vec<String> {
    (sources.iter())
        .map(|source| {
            (source.file_name()).map_or_else(
                || source.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

/// arclite's options and its dialog: the new archive and its whole path; none when it was cancelled.
fn asked(host: Host, request: &Request) -> Option<(NewArchive, PathBuf)> {
    let folder = request.folder.display().to_string();
    let name = default_name(&folder, &names_of(request.sources));
    let update = UpdateSettings::load(&host);
    let new = NewSettings::load(&host);
    let archivers = (request.library).map_or_else(Archivers::zip_only, |library| {
        Archivers::of(&library.formats(), request.sources)
    });
    // arclite's `find_by_name(update_arc_format_name)`, else 7z; zip where neither is offered.
    let format = [new.format.as_str(), "7z"]
        .into_iter()
        .find_map(|name| archivers.find(name))
        .map_or_else(|| archivers.zip.name.clone(), |found| found.name.clone());
    // FAR's "Copy files opened for writing" (on by default; the plugins cannot read it).
    let start = NewArchive::starting(
        (name.clone(), &format),
        (&update, &new),
        request.asked,
        true,
    );
    let profile = &start.profile;
    let old_ext = archivers.extension(&format)
        + if profile.create_sfx && format == "7z" {
            crate::sfx::SFX_EXT
        } else if profile.enable_volumes {
            crate::update_volumes::VOLUME_EXT
        } else {
            ""
        };
    let modules = crate::sfx::Modules(
        request
            .library
            .map(|library| library.sfx_modules())
            .unwrap_or_default(),
    );
    let mut dialog = CreateDialog {
        levels: start.levels.clone(),
        sfx: start.profile.sfx.clone(),
        modules,
        start,
        profiles: Profiles::load(&host),
        default_name: name,
        folder: request.folder.to_path_buf(),
        archivers,
        old_ext,
    };
    let shown = dialog.dialog();
    let closed = host.dialog_with(&shown, |event, controls| {
        dialog.handle(&host, &event, controls)
    });
    dialog.answers(&closed)
}

/// Puts the new archive written beside its place (the first path) there (the second): the plugin
/// closes and reads again its panels of that archive (`swap_in`), the tests rename it.
pub type Place<'a> = dyn FnMut(&Path, &Path) -> Result<(), Error> + 'a;

/// arclite's `Archive::create` once asked: the sources walked, the archive's folder made, the archive
/// written beside its place and put there by `place`, the sources moved deleted when nothing failed
/// or was skipped - never the archive itself or a folder it is in.
pub fn written(
    archive: &Path,
    request: &Request,
    options: &UpdateOptions,
    faults: &mut Faults<'_>,
    place: &mut Place<'_>,
) -> Result<(), Error> {
    let (src_dir, names) = under_folder(request.sources, Some(request.folder));
    let tree = Tree::new(&[]);
    let existing = Existing {
        tree: &tree,
        items: &[],
    };
    let sources = (src_dir.as_path(), names.as_slice());
    let plan = prepare_in(existing, "", sources, Overwrite::Overwrite, faults)
        .map_err(|Abort| stopped())?;
    // arclite's `prepare_dst_dir`, after its `PrepareUpdate`: Esc in the walk leaves no folder.
    if let Some(parent) = archive.parent() {
        let made = std::fs::create_dir_all(parent);
        made.map_err(|error| failed(&Fault::system(&error, parent)))?;
    }
    let written = match request.library {
        Some(library) => crate::update_library::write_new(library, archive, &plan, options, faults),
        None => Err(Stop::Failed(Fault::message(vec![
            crate::options_dialogs::NOT_FOUND.to_string(),
        ]))),
    };
    let made = written.map_err(|stop| match stop {
        Stop::Abort => stopped(),
        Stop::Failed(fault) => failed(&fault),
    })?;
    let files = match made {
        crate::update_library::Made::Beside(temp) => {
            place(&temp, archive)?;
            vec![archive.to_path_buf()]
        }
        crate::update_library::Made::InVolumes => volumes_of(archive),
    };
    if options.move_files && faults.log.is_empty() && !plan.skipped {
        let moved: Vec<PathBuf> = names.iter().map(|name| src_dir.join(name)).collect();
        let made: Vec<Vec<String>> = files.iter().map(|file| key_of(file)).collect();
        delete_moved(&moved, &made, faults).map_err(|Abort| stopped())?;
    }
    Ok(())
}

/// The volumes written of `archive` (`name.001` on, as long as they are there).
fn volumes_of(archive: &Path) -> Vec<PathBuf> {
    (0..)
        .map(|index| crate::update_volumes::volume_path(archive, index))
        .take_while(|volume| volume.exists())
        .collect()
}

/// `path` as a key to compare: its parts, whatever their case.
fn key_of(path: &Path) -> Vec<String> {
    (path.components())
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// The sources moved deleted (arclite's `DeleteSrcFiles`), but not a file of the new archive (`made`:
/// its one file or each of its volumes) when it is one of them, nor a folder one is in: what else is in
/// such a folder goes, the folder stays (arclite would delete the archive it just made).
fn delete_moved(
    sources: &[PathBuf],
    made: &[Vec<String>],
    faults: &mut Faults<'_>,
) -> Result<(), Abort> {
    for source in sources {
        let own = key_of(source);
        if made.contains(&own) {
            continue;
        }
        if made.iter().any(|file| file.starts_with(&own)) {
            let children: Vec<PathBuf> = (std::fs::read_dir(source).into_iter().flatten())
                .flatten()
                .map(|entry| entry.path())
                .collect();
            delete_moved(&children, made, faults)?;
            continue;
        }
        delete_sources(std::slice::from_ref(source), faults)?;
    }
    Ok(())
}

/// arclite's `create_archive` / `put_files` for a new archive: its whole path once made (put in its
/// place by `place`); none when there was nothing to make it of; `Cancelled` when the user stopped it.
pub fn create(
    host: Host,
    request: &Request,
    place: &mut Place<'_>,
) -> Result<Option<PathBuf>, Error> {
    let only_up = request.sources.len() == 1 && request.sources[0].ends_with("..");
    if request.sources.is_empty() || only_up {
        return Ok(None);
    }
    let (answers, archive) = asked(host, request).ok_or_else(stopped)?;
    if archive.exists() {
        let question = Message::new(
            TITLE,
            &["Target file already exists. Do you want to overwrite it?"],
        )
        .button_set(ButtonSet::YesNo);
        if host.show(&question) != Some(0) {
            return Err(stopped());
        }
    }
    let modules = request
        .library
        .map(|library| library.sfx_modules())
        .unwrap_or_default();
    let options = answers.update_options(&crate::sfx::Modules(modules));
    let mut asks = HostAsks::new(host, &archive).creating();
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: options.ignore_errors,
        log: Vec::new(),
    };
    written(&archive, request, &options, &mut faults, place)?;
    let log = std::mem::take(&mut faults.log);
    if !log.is_empty() {
        asks.show_log(&log);
    }
    Ok(Some(archive))
}

#[cfg(test)]
#[path = "creating_tests.rs"]
mod tests;
