//! A disk's archives at once (arclite's `bulk_extract`, `extract(arc_list, options)` and `bulk_test`):
//! the Plugins menu's "Extract archive(s)" and "Test archive(s)" on the active panel's selected files
//! (folders passed over); and a panel's entries tested (`test_files`, Shift+F3's "Test archive files").
//! Each archive is opened by itself; one that is no archive, or fails, goes into the log, which is
//! offered at the end (none: the panel read again, "Archive test complete" after a test); Esc stops all.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Error, ErrorKind, Host, PanelKind, PanelState, Which};

use crate::asking::HostAsks;
use crate::extract::{Abort, Fault, extract_noting};
use crate::extract_dialog::{answers, dialog, handle};
use crate::extract_options::{ExtractOptions, ExtractSettings, Tri};
use crate::extraction::{separate_dir, whole};
use crate::opened::Opened;
use crate::options::TITLE;
use crate::testing::{TEST_OK, Untested, test};

/// How the plugin opens an archive by itself (its code pages): none when the file is no archive.
pub type Load<'a> = dyn Fn(&Path) -> Result<Option<Opened>, Error> + 'a;

fn stopped() -> Error {
    Error::new(ErrorKind::Cancelled, "Stopped")
}

/// The active panel's selected files (the one under the cursor when none is), folders passed over,
/// when it shows real files (arclite's `is_real_file_panel`: a disk's, or a plugin's of real names):
/// their paths and the panel's folder; none when there is none.
pub fn panel_archives(host: &Host) -> Option<(PathBuf, Vec<PathBuf>)> {
    let folder = real_folder(&host.panel(Which::Active)?)?;
    let files: Vec<PathBuf> = (host.selected_items(Which::Active).into_iter())
        .filter(|item| !item.folder && !item.parent)
        .map(|item| {
            item.path
                .map_or_else(|| folder.join(&item.name), PathBuf::from)
        })
        .collect();
    (!files.is_empty()).then_some((folder, files))
}

/// The disk's folder of a panel of real files (arclite's `is_real_file_panel`: a disk's, or a plugin's
/// of real names, its folder the disk's under it); none for another panel.
fn real_folder(state: &PanelState) -> Option<PathBuf> {
    let real = state.kind == PanelKind::Files && (state.plugin.is_none() || state.real_names);
    real.then(|| {
        PathBuf::from(if state.plugin.is_none() {
            &state.folder
        } else {
            &state.disk_folder
        })
    })
}

/// arclite's `bulk_extract`: the Extract dialog (the passive panel's folder when it shows a disk's,
/// the kept answers, "Delete files from archive" not offered), then every archive extracted.
pub fn extract_all(
    host: Host,
    (folder, archives): (&Path, &[PathBuf]),
    load: &Load,
) -> Result<(), Error> {
    let settings = ExtractSettings::load(&host);
    let passive = host
        .panel(Which::Passive)
        .and_then(|state| real_folder(&state));
    let start = ExtractOptions {
        dst_dir: passive.unwrap_or_default(),
        ignore_errors: settings.ignore_errors,
        overwrite: settings.overwrite,
        move_files: Tri::Undef,
        password: String::new(),
        separate_dir: settings.separate_dir,
        delete_archive: false,
        disable_delete_archive: false,
        open_dir: if settings.open_dir {
            Tri::True
        } else {
            Tri::False
        },
    };
    let closed = host.dialog_with(&dialog(&start), |event, controls| {
        handle(&host, &start, &event, controls)
    });
    let mut options = answers(&start, &closed).ok_or_else(stopped)?;
    options.dst_dir = whole(options.dst_dir, Some(folder));
    extract_each(host, archives, options, load)
}

/// arclite's `extract(arc_list, options)`: each archive's root extracted, into a folder named after it
/// when asked or (asked neither way) when its root has more than one entry; deleted after when asked
/// and it added no failure; the log, or the panel read again; the cursor taken there when asked.
pub fn extract_each(
    host: Host,
    archives: &[PathBuf],
    mut options: ExtractOptions,
    load: &Load,
) -> Result<(), Error> {
    let base = options.dst_dir.clone();
    let mut log = Vec::new();
    let mut single = None;
    for archive in archives {
        let mut opened = match load(archive) {
            Ok(Some(opened)) => opened,
            Ok(None) => {
                let words = vec![crate::NOT_ARCHIVE.to_string(), shown(archive)];
                log.push(Fault::message(words));
                continue;
            }
            Err(error) => {
                log.push(not_opened(archive, &error, false));
                continue;
            }
        };
        if !options.password.is_empty() {
            opened.password = Some(options.password.clone());
        }
        let nodes = opened.tree.nodes[0].children.clone();
        if let ([_], [node]) = (archives, nodes.as_slice()) {
            single = Some(opened.tree.nodes[*node].name.clone());
        }
        options.dst_dir = target(&options, &base, archive, nodes.len());
        let mut asks = HostAsks::new(host, archive);
        let (faults, _) =
            extract_noting(&mut opened, &nodes, &options, &mut asks).map_err(|Abort| stopped())?;
        let failed = !faults.is_empty();
        log.extend(faults);
        if deletes_archive(&options, failed) {
            drop(opened);
            // arclite's `delete_file_nt`: a file that cannot be deleted stays, nothing said. A panel of
            // the plugin's showing it goes on with what it read (its file gone).
            let _ = std::fs::remove_file(archive);
        }
    }
    if log.is_empty() {
        host.reread(Which::Active, false);
    } else {
        HostAsks::new(host, Path::new("")).show_log(&log);
    }
    if options.open_dir == Tri::True {
        let (folder, cursor) = gone_to(archives.len(), single, base, options.dst_dir);
        host.set_folder(
            Which::Active,
            &folder.display().to_string(),
            cursor.as_deref(),
        );
    }
    Ok(())
}

/// Where an archive's root goes (arclite's `extract(arc_list)`): a folder named after it when asked,
/// or (asked neither way) when its root has more than one of `entries`; else `base` itself.
fn target(options: &ExtractOptions, base: &Path, archive: &Path, entries: usize) -> PathBuf {
    let separate =
        options.separate_dir == Tri::True || (options.separate_dir == Tri::Undef && entries > 1);
    if separate {
        separate_dir(base, archive)
    } else {
        base.to_path_buf()
    }
}

/// Whether the archive goes after its extraction: asked, and it added no failure.
fn deletes_archive(options: &ExtractOptions, failed: bool) -> bool {
    options.delete_archive && !failed
}

/// Where "Go to target directory" takes the active panel (arclite's `panel_go_to_dir` /
/// `panel_go_to_file`): the folder given for several archives; else the last one's own folder, onto the
/// one entry of a lone archive's root.
fn gone_to(
    archives: usize,
    single: Option<String>,
    base: PathBuf,
    last: PathBuf,
) -> (PathBuf, Option<String>) {
    if archives > 1 {
        (base, None)
    } else {
        (last, single)
    }
}

/// arclite's `bulk_test`: every archive's root tested, each failure in the log with the archive named
/// first; no failure: the panel read again and "Archive test complete".
pub fn test_all(host: Host, archives: &[PathBuf], load: &Load) -> Result<(), Error> {
    let mut log = Vec::new();
    for archive in archives {
        let mut opened = match load(archive) {
            Ok(Some(opened)) => opened,
            Ok(None) => {
                let mut fault = Fault::message(vec![crate::NOT_ARCHIVE.to_string()]);
                fault.objects = vec![shown(archive)];
                log.push(fault);
                continue;
            }
            Err(error) => {
                log.push(not_opened(archive, &error, true));
                continue;
            }
        };
        let nodes = opened.tree.nodes[0].children.clone();
        let mut asks = HostAsks::new(host, archive);
        match test(&mut opened, &nodes, &mut asks) {
            Ok(()) => {}
            Err(Untested::Stopped) => return Err(stopped()),
            Err(Untested::Failed(mut fault)) => {
                fault.objects = vec![shown(archive)];
                log.push(fault);
            }
        }
    }
    if log.is_empty() {
        host.reread(Which::Active, false);
        host.message(TITLE, &[TEST_OK]);
    } else {
        HostAsks::new(host, Path::new("")).show_log(&log);
    }
    Ok(())
}

/// arclite's `test_files`: the entries `names` of the folder `path` tested; done: "Archive test
/// complete"; a failure shown with the archive named first (its `error_dlg`); true when it passed.
pub fn test_entries(
    host: Host,
    opened: &mut Opened,
    (path, names): (&str, &[String]),
) -> Result<bool, Error> {
    let folder = (opened.tree.find_dir(path))
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("{path} is not there")))?;
    // Every name the panel handed is the archive's (arclite's items cannot miss): one not there now
    // (a panel not read again) is not passed over as tested.
    let nodes = (names.iter())
        .filter(|name| name.as_str() != "..")
        .map(|name| {
            crate::extraction::child(opened, folder, name).ok_or_else(|| {
                Error::new(ErrorKind::NotFound, format!("{name} is not in the archive"))
            })
        })
        .collect::<Result<Vec<usize>, Error>>()?;
    let archive = opened.path.clone();
    let mut asks = HostAsks::new(host, &archive);
    match test(opened, &nodes, &mut asks) {
        Ok(()) => {
            host.message(TITLE, &[TEST_OK]);
            Ok(true)
        }
        Err(Untested::Stopped) => Ok(false),
        Err(Untested::Failed(mut fault)) => {
            fault.objects = vec![shown(&archive)];
            crate::asking::show_fault(&host, &fault);
            Ok(false)
        }
    }
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// A file that could not be read as an archive: the system's words and its path (named first in a
/// test's log, `objects`).
fn not_opened(archive: &Path, error: &Error, named_first: bool) -> Fault {
    let mut fault = Fault::message(Vec::new());
    fault.system = Some(error.message.clone());
    if named_first {
        fault.objects = vec![shown(archive)];
    } else {
        fault.messages = vec![shown(archive)];
    }
    fault
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
