//! Adding a disk's files to an archive's panel as arclite's `put_files` runs it (FAR's `PutFilesW`): the
//! "Add files to archive" dialog with the archive's own method and encryption (always: F4's copy put
//! back too, with Overwrite), the sources walked into the panel's folder, the archive written anew beside
//! itself; once it is in its place the sources moved are deleted (none failed, none skipped) and the log
//! of failures offered.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Error, ErrorKind, GetMode, Host};

use crate::asking::{HostAsks, fault_lines};
use crate::extract::{Abort, Fault};
use crate::opened::Opened;
use crate::update::{Faults, prepare, under_folder};
use crate::update_dialog::{answers, dialog, handle};
use crate::update_options::{UpdateOptions, UpdateSettings};
use crate::update_write::{Stop, delete_sources, write};

/// What to add: the panel's folder, the sources and the folder they came from.
pub struct Request<'a> {
    pub path: &'a str,
    pub sources: &'a [PathBuf],
    pub folder: Option<&'a Path>,
    pub moving: bool,
    pub mode: GetMode,
}

/// The new archive written beside the old one, and what is left to do once it is in its place.
pub struct Written {
    pub temp: PathBuf,
    sources: Vec<PathBuf>,
    log: Vec<Fault>,
    /// The sources go after (moved, nothing failed or skipped).
    delete: bool,
    ignore_errors: bool,
}

/// arclite's `MSG_ERROR_NOT_UPDATABLE`: an archive its format's library does not write, or one inside
/// another.
pub const NOT_WRITTEN_YET: &str = "Archive modification not supported";

/// arclite's `MSG_ERROR_UPDATE_UNSUPPORTED_FOR_SINGLEFILEARCHIVE`.
pub const SINGLE_FILE: &str = "Can't change single file archive format";

/// arclite's `is_single_file_format`.
pub fn is_single_file_format(format: &str) -> bool {
    matches!(format, "bzip2" | "gzip" | "xz" | "SWFc")
}

/// Whether the archive can be written anew (arclite's `updatable`: one archive of the file's own level of
/// a format 7-Zip's library writes).
pub(crate) fn changeable(opened: &Opened) -> Result<(), Error> {
    if opened.writes.is_none() {
        return Err(Error::new(ErrorKind::Unsupported, NOT_WRITTEN_YET));
    }
    Ok(())
}

/// arclite's rule for the sources after an update: deleted when moved, nothing failed, none skipped.
fn delete_after(moving: bool, log: &[Fault], skipped: bool) -> bool {
    moving && log.is_empty() && !skipped
}

fn stopped() -> Error {
    Error::new(ErrorKind::Cancelled, "Stopped")
}

fn failed(fault: &Fault) -> Error {
    Error::new(ErrorKind::Other, fault_lines(fault).join("\n"))
}

/// arclite's `put_files` up to the new archive written: none when there is nothing to add.
pub fn write_update(
    host: Host,
    opened: &Opened,
    request: &Request,
) -> Result<Option<Written>, Error> {
    let only_up = request.sources.len() == 1 && request.sources[0].ends_with("..");
    if request.sources.is_empty() || only_up {
        return Ok(None);
    }
    changeable(opened)?;
    single_file(opened, request)?;
    let settings = UpdateSettings::load(&host);
    // FAR's "Copy files opened for writing" (on by default; the plugins cannot read it).
    let mut options = options_for(opened, &settings, request.mode.edit);
    options.move_files = request.moving;
    // arclite's `archive->m_password`: the one the archive was opened or read with.
    options.password = opened.password.clone().unwrap_or_default();
    let closed = host.dialog_with(&dialog(&options), |event, controls| {
        handle(&host, &options, &event, controls)
    });
    let options = answers(&options, &closed).ok_or_else(stopped)?;
    write_with(host, opened, request, &options, true)
}

/// arclite's options of an update of `opened` (`put_files`): its format's, its method, solid and
/// encryption by `load_update_props` (a zip's method Deflate: its handler names none).
pub(crate) fn options_for(opened: &Opened, settings: &UpdateSettings, edit: bool) -> UpdateOptions {
    let format = opened.writes.as_deref().unwrap_or("zip");
    let props = match opened.library() {
        Some((chain, _)) => crate::update_props::load_update_props(
            chain.archive(),
            format.eq_ignore_ascii_case("zip"),
        ),
        None => crate::update_props::ArchiveProps {
            level: 7,
            method: "Deflate".to_string(),
            solid: false,
            encrypted: false,
        },
    };
    UpdateOptions::for_library(settings, (format, &props), edit, true)
}

/// arclite's `delete_files`: an archive written anew without some of its items is one that can be
/// changed and not of a single file's format (its one file is all it holds).
pub(crate) fn deletable(opened: &Opened) -> Result<(), Error> {
    changeable(opened)?;
    if opened.writes.as_deref().is_some_and(is_single_file_format) {
        return Err(Error::new(ErrorKind::Unsupported, SINGLE_FILE));
    }
    Ok(())
}

/// arclite's single file archive's rule: a gzip, bzip2 or xz takes only its one file again, by its name.
fn single_file(opened: &Opened, request: &Request) -> Result<(), Error> {
    let Some(format) = opened.writes.as_deref() else {
        return Ok(());
    };
    if !is_single_file_format(format) {
        return Ok(());
    }
    let own = opened.items.first().and_then(|item| item.path.last());
    let named = |source: &PathBuf| {
        source
            .file_name()
            .is_some_and(|name| own.is_some_and(|own| name.to_string_lossy() == own.as_str()))
    };
    if request.sources.len() != 1 || !request.sources.iter().all(named) {
        return Err(Error::new(ErrorKind::Unsupported, SINGLE_FILE));
    }
    Ok(())
}

/// The sources walked into the folder `request.path` and the archive written anew with `options`
/// (arclite's `Archive::update` once its options are known): none when there is nothing to add (the
/// log of what failed then offered with `show_log`).
pub fn write_with(
    host: Host,
    opened: &Opened,
    request: &Request,
    options: &UpdateOptions,
    show_log: bool,
) -> Result<Option<Written>, Error> {
    let (src_dir, names) = under_folder(request.sources, request.folder);
    let mut asks = HostAsks::new(host, &opened.path);
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: options.ignore_errors,
        log: Vec::new(),
    };
    let plan = prepare(
        opened,
        request.path,
        (&src_dir, &names),
        options.overwrite,
        &mut faults,
    )
    .map_err(|Abort| stopped())?;
    if plan.added.is_empty() {
        // Nothing to write; what failed is still told.
        let log = std::mem::take(&mut faults.log);
        if show_log && !log.is_empty() {
            asks.show_log(&log);
        }
        return Ok(None);
    }
    let temp = write(opened, &plan, options, &mut faults).map_err(|stop| match stop {
        Stop::Abort => stopped(),
        Stop::Failed(fault) => failed(&fault),
    })?;
    Ok(Some(Written {
        temp,
        sources: names.iter().map(|name| src_dir.join(name)).collect(),
        delete: delete_after(options.move_files, &faults.log, plan.skipped),
        ignore_errors: faults.ignore_errors,
        log: faults.log,
    }))
}

/// The new archive in its place: the sources moved deleted (arclite's `DeleteSrcFiles`), then the log
/// of failures offered.
pub fn after(host: Host, archive: &Path, written: Written) -> Result<(), Error> {
    after_with(host, archive, written, true)
}

/// [`after`], the log offered only with `show_log` (`arc:u` keeps it: arclite's `cmdline_update`
/// shows none).
pub fn after_with(
    host: Host,
    archive: &Path,
    written: Written,
    show_log: bool,
) -> Result<(), Error> {
    let mut asks = HostAsks::new(host, archive);
    let mut faults = Faults {
        asks: &mut asks,
        ignore_errors: written.ignore_errors,
        log: written.log,
    };
    if written.delete {
        delete_sources(&written.sources, &mut faults).map_err(|Abort| stopped())?;
    }
    let log = std::mem::take(&mut faults.log);
    if show_log && !log.is_empty() {
        asks.show_log(&log);
    }
    Ok(())
}

#[cfg(test)]
#[path = "updating_tests.rs"]
mod tests;
