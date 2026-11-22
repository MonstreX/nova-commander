//! An extraction from an archive's panel as arclite's `Plugin::extract` runs it: for F5 / F6 (FAR's
//! `GetFilesW`) and Alt+F6 its Extract dialog first (the folder made whole, a separate folder for the
//! archive, the password given), without it for the viewer and the editor; the log of its failures after,
//! and the archive deleted when asked and nothing failed (its panel closed to the archive's folder).

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Error, ErrorKind, GetMode, Host, Which};

use crate::asking::HostAsks;
use crate::extract::{Abort, extract_noting};
use crate::extract_dialog::{answers, dialog, handle};
use crate::extract_options::{ExtractOptions, ExtractSettings, Tri};
use crate::names::auto_rename;
use crate::opened::Opened;

/// What to extract: the panel's folder and the entries named in it.
pub struct Request<'a> {
    pub path: &'a str,
    pub names: &'a [String],
    pub destination: PathBuf,
    pub mode: GetMode,
    /// The program's folder for a folder typed without its whole path (FAR's current folder).
    pub folder: Option<&'a Path>,
    /// F6: what was extracted deleted from the archive after (arclite's `move`).
    pub moving: bool,
}

/// What is left to do once an extraction ended without failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    Nothing,
    /// The archive deleted (asked for in the dialog).
    DeleteArchive,
    /// The archive written anew without what was extracted (F6), this file to put in its place.
    Replace(PathBuf),
}

fn stopped() -> Error {
    Error::new(ErrorKind::Cancelled, "Stopped")
}

/// arclite's `get_separate_dir_path`: the archive's name without its extension in `folder`, a new name
/// when it is taken.
pub(crate) fn separate_dir(folder: &Path, archive: &Path) -> PathBuf {
    let stem = archive.file_stem().unwrap_or(archive.as_os_str());
    auto_rename(&folder.join(stem))
}

/// `folder` made whole: `.` when empty, from the program's folder when relative (FAR's
/// `get_absolute_path`).
pub(crate) fn whole(folder: PathBuf, base: Option<&Path>) -> PathBuf {
    let folder = if folder.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        folder
    };
    if folder.is_absolute() {
        return folder;
    }
    let joined = base.map_or_else(|| folder.clone(), |base| base.join(&folder));
    std::path::absolute(&joined).unwrap_or(joined)
}

/// The answers of a disk's archives' first by a plugin (Shift+F2, arclite's `static batch_options`),
/// for the others it gets silently: the options as that one ended with them, the folder given in its
/// dialog (FAR's `DestPath` handed on to the next).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub options: ExtractOptions,
    pub base: PathBuf,
}

/// arclite's `update_dst_dir`: a folder of the archive's own when asked, or (asked neither way) for
/// several entries of a disk's archives at once.
fn separate_wanted(options: &ExtractOptions, request: &Request) -> bool {
    let single = request.names.len() == 1;
    options.separate_dir == Tri::True
        || (options.separate_dir == Tri::Undef && !single && request.mode.top_level)
}

/// The Extract dialog's answers for `options` and the folder given in it, made whole; none for Cancel.
fn ask(
    host: Host,
    opened: &Opened,
    options: ExtractOptions,
    request: &Request,
) -> Option<(ExtractOptions, PathBuf)> {
    let closed = host.dialog_with(&dialog(&options), |event, controls| {
        handle(&host, &options, &event, controls)
    });
    let mut answered = answers(&options, &closed)?;
    answered.dst_dir = whole(answered.dst_dir, request.folder);
    let base = answered.dst_dir.clone();
    if separate_wanted(&answered, request) {
        answered.dst_dir = separate_dir(&answered.dst_dir, &opened.path);
    }
    Some((answered, base))
}

/// A disk's archives' next by the same plugin (silent, top level): the first one's answers, its folder
/// of the archive's own when wanted (arclite: `options = batch_options; update_dst_dir()`).
fn batched(kept: &Batch, opened: &Opened, request: &Request) -> ExtractOptions {
    let mut options = kept.options.clone();
    if separate_wanted(&options, request) {
        options.dst_dir = separate_dir(&kept.base, &opened.path);
    }
    options
}

/// arclite's "Go to target directory" (`open_dir`): the active panel to the folder, onto the one entry
/// extracted when it was one.
fn go_there(host: Host, options: &ExtractOptions, request: &Request) {
    if options.open_dir != Tri::True {
        return;
    }
    let cursor = match request.names {
        [one] => Some(one.as_str()),
        _ => None,
    };
    host.set_folder(
        Which::Active,
        &options.dst_dir.display().to_string(),
        cursor,
    );
}

/// The entries `request` names written out (FAR's `GetFilesW`, Alt+F6), then, nothing failed, the
/// archive deleted when asked or (F6) written anew without them; `Cancelled` when the user stopped it.
/// `batch` holds a disk's archives' first answers for the next ones by this plugin (Shift+F2).
pub fn run(
    host: Host,
    opened: &mut Opened,
    request: &Request,
    batch: &mut Option<Batch>,
) -> Result<After, Error> {
    let folder = (opened.tree.find_dir(request.path)).ok_or_else(|| {
        Error::new(
            ErrorKind::NotFound,
            format!("{} is not there", request.path),
        )
    })?;
    // F6 deletes what it extracts: an entry it cannot tell from another is refused first.
    if request.moving && crate::updating::changeable(opened).is_ok() {
        crate::deleting::refuse_twins(opened, folder, request.names)?;
    }
    let nodes: Vec<usize> = (request.names.iter())
        .filter(|name| name.as_str() != "..")
        .filter_map(|name| child(opened, folder, name))
        .collect();
    if nodes.is_empty() {
        return Ok(After::Nothing);
    }
    let settings = ExtractSettings::load(&host);
    let mut options =
        ExtractOptions::starting(request.destination.clone(), request.mode, &settings);
    // arclite: `updatable() ? (move ? triTrue : triFalse) : triUndef`.
    options.move_files = match (crate::updating::changeable(opened), request.moving) {
        (Err(_), _) => Tri::Undef,
        (Ok(()), true) => Tri::True,
        (Ok(()), false) => Tri::False,
    };
    let dialog_shown = request.mode.with_dialog();
    if dialog_shown {
        let (answered, base) = ask(host, opened, options, request).ok_or_else(stopped)?;
        options = answered;
        if !options.password.is_empty() {
            opened.password = Some(options.password.clone());
        }
        if request.mode.top_level {
            *batch = Some(Batch {
                options: options.clone(),
                base,
            });
        }
    } else if let (true, Some(kept)) = (request.mode.top_level, batch.as_ref()) {
        options = batched(kept, opened, request);
    }
    let mut asks = HostAsks::new(host, &opened.path);
    let (log, skipped) =
        extract_noting(opened, &nodes, &options, &mut asks).map_err(|Abort| stopped())?;
    if !log.is_empty() && !request.mode.silent && dialog_shown {
        asks.show_log(&log);
    }
    go_there(host, &options, request);
    if !log.is_empty() {
        return Ok(After::Nothing);
    }
    if options.delete_archive {
        return Ok(After::DeleteArchive);
    }
    if options.move_files == Tri::True {
        let written = crate::deleting::delete_extracted(host, opened, &nodes, &skipped)?;
        return Ok(written.map_or(After::Nothing, After::Replace));
    }
    Ok(After::Nothing)
}

/// The entry `name` of the folder `folder`: by its name as written, else whatever its case.
pub(crate) fn child(opened: &Opened, folder: usize, name: &str) -> Option<usize> {
    let children = &opened.tree.nodes[folder].children;
    let named = |exact: bool| {
        children.iter().copied().find(|&node| {
            let own = &opened.tree.nodes[node].name;
            if exact {
                own == name
            } else {
                own.to_lowercase() == name.to_lowercase()
            }
        })
    };
    named(true).or_else(|| named(false))
}

/// arclite's `delete_archive` after an extraction asked for it, the archive closed first (`archive`, its
/// `Opened` dropped): the file deleted where it can be (`delete_file_nt`: quietly), its panel closed to
/// the archive's folder either way (arclite's `need_close_panel`, `Far::close_panel` of its own panel:
/// the one the call is for, wherever the cursor went meanwhile).
pub fn delete_archive(host: Host, archive: &Path) {
    let _ = std::fs::remove_file(archive);
    let folder = archive.parent().map(|parent| parent.display().to_string());
    let own = [Which::Left, Which::Right]
        .into_iter()
        .find(|&which| host.panel(which).is_some_and(|state| state.call_target));
    host.close_panel(own.unwrap_or(Which::Active), folder.as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_made_whole_and_a_separate_one_named_after_the_archive() {
        let base = Path::new(r"C:\base");
        assert_eq!(
            whole(PathBuf::from(r"D:\x"), Some(base)),
            PathBuf::from(r"D:\x")
        );
        assert_eq!(
            whole(PathBuf::from("sub"), Some(base)),
            PathBuf::from(r"C:\base\sub")
        );
        assert_eq!(whole(PathBuf::new(), Some(base)), PathBuf::from(r"C:\base"));
        let temp = tempfile::tempdir().unwrap();
        let archive = Path::new(r"C:\a\data.zip");
        assert_eq!(separate_dir(temp.path(), archive), temp.path().join("data"));
        std::fs::create_dir(temp.path().join("data")).unwrap();
        assert_eq!(
            separate_dir(temp.path(), archive),
            temp.path().join("data.0")
        );
    }

    /// The separate folder is the name without its last extension only; a name without one as it is.
    #[test]
    fn a_separate_folder_drops_the_last_extension_only() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path();
        let named = |archive: &str| separate_dir(folder, Path::new(archive));
        assert_eq!(named(r"C:\a\b.tar.gz"), folder.join("b.tar"));
        assert_eq!(named(r"C:\a\plain"), folder.join("plain"));
        std::fs::write(folder.join("plain"), "taken by a file").unwrap();
        assert_eq!(named(r"C:\a\plain"), folder.join("plain.0"));
        let base = Path::new(r"C:\base");
        assert_eq!(
            whole(PathBuf::from(r"..\up"), Some(base)),
            PathBuf::from(r"C:\up"),
            "made whole: `..` folded"
        );
    }

    /// An entry named by the program: by its name as written first, else whatever its case; none when
    /// the folder has no such one.
    #[test]
    fn an_entry_is_found_by_its_name_then_whatever_its_case() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("c.zip");
        let entries: Vec<crate::test_archives::Entry<'_>> = ["Read.me", "read.ME", "sub/inner.txt"]
            .into_iter()
            .map(|name| (name, Some(name.as_bytes())))
            .collect();
        crate::test_archives::zip(&path, &entries);
        let opened = crate::test_archives::opened(&path);
        let root = opened.tree.find_dir("/").unwrap();
        let name_of = |node: Option<usize>| node.map(|node| opened.tree.nodes[node].name.clone());
        let found = |name: &str| name_of(child(&opened, root, name));
        assert_eq!(found("read.ME").as_deref(), Some("read.ME"), "as written");
        assert_eq!(found("Read.me").as_deref(), Some("Read.me"), "as written");
        let folded = found("READ.ME").unwrap();
        assert!(folded.eq_ignore_ascii_case("read.me"), "{folded}");
        assert_eq!(found("SUB").as_deref(), Some("sub"));
        assert_eq!(found("inner.txt"), None, "not in this folder");
        assert_eq!(found("none"), None);
    }

    /// Shift+F2's next archive by the plugin (arclite's `batch_options`, `update_dst_dir`): the first
    /// one's answers, a folder of this archive's own under the folder given when it has several entries
    /// (asked neither way) or one was asked for; else the first one's folder as it ended.
    #[test]
    fn a_disks_next_archive_takes_the_first_ones_answers() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("next.zip");
        crate::test_archives::zip(&path, &[("one.txt", Some(b"one"))]);
        let opened = crate::test_archives::opened(&path);
        let settings = ExtractSettings::default();
        let top = GetMode {
            top_level: true,
            silent: true,
            ..GetMode::default()
        };
        let mut first = ExtractOptions::starting(PathBuf::from(r"C:\given\first"), top, &settings);
        first.separate_dir = Tri::Undef;
        first.password = "pw".to_string();
        let kept = Batch {
            options: first,
            base: temp.path().to_path_buf(),
        };
        let names = |count: usize| -> Vec<String> { (0..count).map(|n| n.to_string()).collect() };
        let several = names(2);
        let request = |names: &'static [String]| Request {
            path: "/",
            names,
            destination: PathBuf::new(),
            mode: top,
            folder: None,
            moving: false,
        };
        let several: &'static [String] = several.leak();
        let one: &'static [String] = names(1).leak();
        let options = batched(&kept, &opened, &request(several));
        assert_eq!(options.dst_dir, temp.path().join("next"), "its own folder");
        assert_eq!(options.password, "pw", "the first one's answers");
        let options = batched(&kept, &opened, &request(one));
        assert_eq!(
            options.dst_dir,
            PathBuf::from(r"C:\given\first"),
            "as it ended"
        );
        let mut asked = kept.clone();
        asked.options.separate_dir = Tri::True;
        let options = batched(&asked, &opened, &request(one));
        assert_eq!(options.dst_dir, temp.path().join("next"), "asked for");
        let mut never = kept;
        never.options.separate_dir = Tri::False;
        let options = batched(&never, &opened, &request(several));
        assert_eq!(
            options.dst_dir,
            PathBuf::from(r"C:\given\first"),
            "asked not to"
        );
    }
}
