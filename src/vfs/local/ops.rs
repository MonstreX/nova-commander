//! File operations (critical area: data loss).
//!
//! Safety rules enforced here, not in the UI:
//! - an existing directory is never made again (`make_dir` fails with `AlreadyExists`);
//! - a drive root is never deleted or moved to the Recycle Bin;
//! - `remove` takes one object at a time: a folder only when it is empty (or is a link), so what is
//!   inside is always the caller's decision (`jobs::delete`, FAR's questions);
//! - a file is saved through a uniquely named temporary sibling put in place at once, so a failed save
//!   keeps the old content and never removes a file this operation did not create; an existing file is
//!   replaced keeping its ACL and owner, attributes, creation time and named streams (`ReplaceFileW`),
//!   and one with several names (hard links) is written in place so that every name has the new
//!   content (as FAR's editor writes).
//!
//! Links (symbolic links, junctions) are deleted without touching what they point to. Copy and move
//! are `jobs::transfer` (ADR-0009).

use crate::platform;
use crate::platform::files::paths;
use crate::vfs::Removal;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// How many temporary names `write_atomic` tries before giving up.
const TEMP_NAME_ATTEMPTS: u32 = 100;

/// Creates `parent/name` (nested names like `a\b` are allowed). Fails if it already exists. Spaces
/// around the name go (FAR's `with_trim`); a last dot stays: `d.` is not its sibling `d`.
pub fn make_dir(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid_input("directory name is empty"));
    }
    let path = parent.join(name);
    let exact = platform::files::exact_path(&path);
    ensure_absent(&exact)?;
    fs::create_dir_all(&exact)?;
    Ok(path)
}

/// Removes one object for the provider's `remove`: a file or a link to a file, an empty folder or a link
/// to a folder (never followed), or a file or folder into the Recycle Bin. A read-only object loses
/// the attribute first, as FAR does after its question: Windows refuses to delete it otherwise.
pub fn remove(path: &Path, how: Removal) -> io::Result<()> {
    if paths::is_root(path) {
        return Err(invalid_input("refusing to delete a drive root"));
    }
    // The name exactly: `a ` must never reach its sibling `a` (FAR's `nt_path`).
    let exact = platform::files::exact_path(path);
    match how {
        Removal::Trash if ends_in_space_or_dot(path) => Err(io::Error::new(
            io::ErrorKind::InvalidFilename,
            "the Recycle Bin cannot take a name that ends with a space or a dot",
        )),
        Removal::Trash => platform::files::move_to_trash(path),
        Removal::File => {
            clear_read_only(&exact)?;
            fs::remove_file(&exact)
        }
        Removal::EmptyDir => {
            clear_read_only(&exact)?;
            fs::remove_dir(&exact)
        }
    }
}

/// A name anywhere in the path that the shell might take for another one (Win32 drops such ends of
/// the last name; whether the shell keeps them inside the path is not worth finding out on a user's
/// files).
fn ends_in_space_or_dot(path: &Path) -> bool {
    path.components().any(|component| match component {
        std::path::Component::Normal(name) => {
            let name = name.to_string_lossy();
            name.ends_with(' ') || name.ends_with('.')
        }
        _ => false,
    })
}

fn clear_read_only(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.permissions().readonly() {
        platform::files::set_read_only(path, false)?;
    }
    Ok(())
}

/// Replaces the content of `path`: write a temporary sibling, flush it to disk, put it in place (keeping the
/// old file's metadata); a file with several names is written in place. Refuses to replace a symbolic
/// link (the rename would turn the link into a plain file).
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    save_streamed(path, &mut |out| out.write_all(contents), &mut || {})
}

/// The viewer's bytes typed (ADR-0027) written into `path` in place (R7 review E6: in `vfs`, a critical
/// area): each run of them where it is; refused when the file is
/// no longer as long as it was read (`read`): another file, or cut (R7 review C9: the bytes went into
/// whatever was there, past a shorter one's end with zeros before them).
pub fn write_bytes_in_place(
    path: &Path,
    changed: &std::collections::BTreeMap<u64, u8>,
    read: Option<u64>,
) -> io::Result<()> {
    use std::io::{Seek, SeekFrom};
    // The name exactly: `a ` never reaches its sibling `a` (R7 review C4).
    let exact = platform::files::exact_path(path);
    let mut file = std::fs::OpenOptions::new().write(true).open(exact)?;
    let now = file.metadata()?.len();
    if read.is_some_and(|read| read != now)
        || changed.keys().next_back().is_some_and(|&at| at >= now)
    {
        return Err(io::Error::other(
            "The file changed since it was read: the bytes are not written",
        ));
    }
    let mut runs: Vec<(u64, Vec<u8>)> = Vec::new();
    for (&at, &byte) in changed {
        match runs.last_mut() {
            Some((start, bytes)) if *start + bytes.len() as u64 == at => bytes.push(byte),
            _ => runs.push((at, vec![byte])),
        }
    }
    for (at, bytes) in runs {
        file.seek(SeekFrom::Start(at))?;
        file.write_all(&bytes)?;
    }
    file.sync_all()
}

/// [`write_atomic`] for content too big to hold: `write` streams it into the temporary sibling (an
/// error, a stop too, removes it and leaves the file as it was); `written` runs once it is on the
/// disk, before it takes the file's place (the editor reads the old file then: its undo's bytes).
/// A file with several names gets the content copied over it from the temporary file, which goes.
pub fn save_streamed(
    path: &Path,
    write: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    written: &mut dyn FnMut(),
) -> io::Result<()> {
    // The name exactly: saving `a ` must never reach its sibling `a`.
    let exact = platform::files::exact_path(path);
    if fs::symlink_metadata(&exact).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "{} is a link; saving through links is not supported",
                path.display()
            ),
        ));
    }
    let existing = fs::symlink_metadata(&exact).is_ok_and(|metadata| metadata.is_file());
    let names = existing && platform::files::name_count(&exact).is_ok_and(|names| names > 1);
    let (temp_path, file) = create_temp_sibling(&exact)?;
    // From here on the temporary file is ours, so removing it on failure is safe.
    let mut out = io::BufWriter::new(file);
    let done = write(&mut out)
        .and_then(|()| out.into_inner().map_err(io::IntoInnerError::into_error))
        .and_then(|file| file.sync_all());
    if let Err(error) = done {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    written();
    if names {
        return in_place_end(write_in_place(&exact, &temp_path), &temp_path);
    }
    let result = put_in_place(&temp_path, &exact, existing);
    let Err(error) = result else {
        return Ok(());
    };
    // The old file gone and the new one not in its place: the temporary file holds the only copy, kept
    // and named in the error.
    if existing && fs::symlink_metadata(&exact).is_err() {
        return Err(kept_in(error, &temp_path));
    }
    // Best effort cleanup of our own temporary file; the original error is what the caller needs.
    let _ = fs::remove_file(&temp_path);
    Err(error)
}

/// The saved temporary file in place of `path`: replacing an existing file keeps its metadata; where the
/// system cannot replace (another OS, a file system without it), a rename as before.
fn put_in_place(temp_path: &Path, path: &Path, existing: bool) -> io::Result<()> {
    if !existing {
        return fs::rename(temp_path, path);
    }
    match platform::files::replace_file(path, temp_path) {
        Ok(()) => Ok(()),
        // The old file went and the new one did not take its name: it is put there now.
        Err(platform::files::Replaced::Moved) => fs::rename(temp_path, path),
        // Nothing replaced (no such call here, a file system without it): a rename, as before.
        Err(platform::files::Replaced::Untouched) => fs::rename(temp_path, path),
    }
}

/// `error` with the place of the temporary file that holds the saved content.
fn kept_in(error: io::Error, temp_path: &Path) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{error}; the saved content is in {}", temp_path.display()),
    )
}

/// How writing over a file failed.
enum InPlace {
    /// Before anything was written over it.
    Untouched(io::Error),
    /// Once it was cut.
    Cut(io::Error),
}

/// The temporary file after it was copied over the file: gone, unless the file was cut and written in
/// part (a full disk, the network) - then it holds the only whole copy, kept and named (R7 review C3:
/// it was removed, both versions lost).
fn in_place_end(copied: Result<(), InPlace>, temp_path: &Path) -> io::Result<()> {
    match copied {
        Ok(()) => {
            let _ = fs::remove_file(temp_path);
            Ok(())
        }
        // Nothing written over the file yet: it is as it was.
        Err(InPlace::Untouched(error)) => {
            let _ = fs::remove_file(temp_path);
            Err(error)
        }
        Err(InPlace::Cut(error)) => Err(kept_in(error, temp_path)),
    }
}

/// The saved temporary file copied over the file itself (every name of it sees it; not atomic).
fn write_in_place(path: &Path, saved: &Path) -> Result<(), InPlace> {
    let mut source = fs::File::open(saved).map_err(InPlace::Untouched)?;
    let mut file = (fs::OpenOptions::new().write(true).truncate(true))
        .open(path)
        .map_err(InPlace::Untouched)?;
    io::copy(&mut source, &mut file).map_err(InPlace::Cut)?;
    file.sync_all().map_err(InPlace::Cut)
}

/// Name of the n-th temporary file candidate for saving `path`.
fn temp_candidate(path: &Path, attempt: u32) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| invalid_input("path has no file name"))?;
    let suffix = format!(".vc-save-{}-{attempt}.tmp", std::process::id());
    // A name the suffix would take past a name's 255 characters goes without it (R7 review C10: such a
    // file could not be saved).
    let long = file_name.to_string_lossy().encode_utf16().count() + suffix.len() > 255;
    let mut temp_name = match long {
        true => std::ffi::OsString::new(),
        false => file_name.to_os_string(),
    };
    temp_name.push(suffix);
    Ok(path.with_file_name(temp_name))
}

/// Creates a temporary file next to `path` under a free name; never opens an existing file.
fn create_temp_sibling(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    for attempt in 0..TEMP_NAME_ATTEMPTS {
        let candidate = temp_candidate(path, attempt)?;
        let opened = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate);
        match opened {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free temporary file name next to the file",
    ))
}

fn ensure_absent(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", path.display()),
        ));
    }
    Ok(())
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn make_tree(root: &Path) -> PathBuf {
        let tree = root.join("tree");
        fs::create_dir_all(tree.join("inner")).unwrap();
        fs::write(tree.join("top.txt"), "top").unwrap();
        fs::write(tree.join("inner").join("deep.txt"), "deep").unwrap();
        tree
    }

    /// Directory junctions need no special privileges, unlike symbolic links.
    fn make_junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap()
            .status;
        assert!(status.success());
    }

    fn entry_count(dir: &Path) -> usize {
        fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn make_dir_creates_directory_and_refuses_existing() {
        let temp = tempfile::tempdir().unwrap();
        let created = make_dir(temp.path(), "  new  ").unwrap();
        assert!(created.is_dir());
        assert_eq!(created, temp.path().join("new"));

        let error = make_dir(temp.path(), "new").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn make_dir_keeps_a_last_dot_and_never_takes_a_sibling_for_the_name() {
        let temp = tempfile::tempdir().unwrap();
        let exact = |name: &str| platform::files::exact_path(&temp.path().join(name));
        fs::create_dir(exact("d")).unwrap();

        make_dir(temp.path(), "d.").unwrap();

        assert!(exact("d.").is_dir(), "created under its own name");
        assert!(exact("d").is_dir());
        assert_eq!(entry_count(temp.path()), 2);
    }

    #[test]
    fn write_atomic_saves_the_exact_name_and_leaves_the_sibling_alone() {
        let temp = tempfile::tempdir().unwrap();
        let exact = |name: &str| platform::files::exact_path(&temp.path().join(name));
        fs::write(exact("a "), "spaced").unwrap();
        fs::write(exact("a"), "plain").unwrap();

        write_atomic(&temp.path().join("a "), b"new").unwrap();

        assert_eq!(read(&exact("a ")), "new");
        assert_eq!(read(&exact("a")), "plain");
        assert_eq!(entry_count(temp.path()), 2, "no temporary file stays");
    }

    #[test]
    fn make_dir_rejects_empty_name() {
        let temp = tempfile::tempdir().unwrap();
        let error = make_dir(temp.path(), "   ").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn remove_refuses_roots_before_any_disk_access() {
        // A drive letter that is not mounted: even a broken root check could not delete anything.
        let mounted: Vec<char> = crate::platform::volumes::volume_roots()
            .iter()
            .filter_map(|root| root.letter)
            .collect();
        let unmounted = ('D'..='Z')
            .find(|letter| !mounted.contains(letter))
            .map(|letter| format!("{letter}:\\"))
            .unwrap();

        for how in [Removal::File, Removal::EmptyDir, Removal::Trash] {
            let error = remove(Path::new(&unmounted), how).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }

    #[test]
    fn remove_takes_a_file_and_an_empty_folder_and_refuses_a_full_one() {
        let temp = tempfile::tempdir().unwrap();
        let tree = make_tree(temp.path());
        let error = remove(&tree, Removal::EmptyDir).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::DirectoryNotEmpty);
        assert_eq!(read(&tree.join("top.txt")), "top");
        assert!(
            remove(&tree, Removal::File).is_err(),
            "a folder is not a file"
        );

        remove(&tree.join("inner").join("deep.txt"), Removal::File).unwrap();
        remove(&tree.join("inner"), Removal::EmptyDir).unwrap();

        assert!(!tree.join("inner").exists());
        assert!(tree.join("top.txt").exists());
    }

    #[test]
    fn remove_deletes_read_only_files_and_folders() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("ro.txt");
        fs::write(&file, "x").unwrap();
        platform::files::set_read_only(&file, true).unwrap();
        let dir = temp.path().join("ro-dir");
        fs::create_dir(&dir).unwrap();
        platform::files::set_read_only(&dir, true).unwrap();

        remove(&file, Removal::File).unwrap();
        remove(&dir, Removal::EmptyDir).unwrap();

        assert!(!file.exists());
        assert!(!dir.exists());
    }

    #[test]
    fn remove_takes_the_name_exactly_never_a_sibling_without_its_last_space_or_dot() {
        let temp = tempfile::tempdir().unwrap();
        let exact = |name: &str| platform::files::exact_path(&temp.path().join(name));
        fs::write(exact("a "), "spaced").unwrap();
        fs::write(exact("a"), "plain").unwrap();
        fs::write(exact("b."), "dotted").unwrap();
        fs::write(exact("b"), "plain").unwrap();
        platform::files::set_read_only(&exact("b"), true).unwrap();
        fs::create_dir(exact("d ")).unwrap();
        fs::create_dir(exact("d")).unwrap();

        remove(&temp.path().join("a "), Removal::File).unwrap();
        remove(&temp.path().join("b."), Removal::File).unwrap();
        remove(&temp.path().join("d "), Removal::EmptyDir).unwrap();

        assert!(fs::symlink_metadata(exact("a ")).is_err());
        assert_eq!(read(&exact("a")), "plain");
        assert!(fs::symlink_metadata(exact("b.")).is_err());
        assert_eq!(read(&exact("b")), "plain");
        assert!(
            fs::metadata(exact("b")).unwrap().permissions().readonly(),
            "the sibling keeps its read-only attribute"
        );
        platform::files::set_read_only(&exact("b"), false).unwrap();
        assert!(fs::symlink_metadata(exact("d ")).is_err());
        assert!(exact("d").is_dir());
    }

    #[test]
    fn the_recycle_bin_is_refused_a_name_it_would_take_for_another() {
        let temp = tempfile::tempdir().unwrap();
        let exact = platform::files::exact_path(&temp.path().join("x "));
        fs::write(&exact, "x").unwrap();
        let error = remove(&temp.path().join("x "), Removal::Trash).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidFilename);
        assert!(exact.exists());
        fs::remove_file(&exact).unwrap();
        // Inside such a folder too.
        let folder = platform::files::exact_path(&temp.path().join("d "));
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("f.txt"), "x").unwrap();
        let inside = temp.path().join("d ").join("f.txt");
        let error = remove(&inside, Removal::Trash).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidFilename);
        assert!(folder.join("f.txt").exists());
    }

    #[test]
    fn remove_takes_a_junction_and_keeps_its_target() {
        let temp = tempfile::tempdir().unwrap();
        let tree = make_tree(temp.path());
        let link = temp.path().join("link");
        make_junction(&link, &tree);

        remove(&link, Removal::EmptyDir).unwrap();

        assert!(fs::symlink_metadata(&link).is_err());
        assert_eq!(read(&tree.join("inner").join("deep.txt")), "deep");
    }

    #[test]
    fn remove_of_a_missing_path_reports_not_found() {
        let temp = tempfile::tempdir().unwrap();
        for how in [Removal::File, Removal::EmptyDir, Removal::Trash] {
            let error = remove(&temp.path().join("missing"), how).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound, "{how:?}: {error}");
        }
    }

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("doc.txt");
        fs::write(&path, "old").unwrap();

        write_atomic(&path, b"new").unwrap();

        assert_eq!(read(&path), "new");
        assert_eq!(entry_count(temp.path()), 1);
    }

    #[test]
    fn write_atomic_creates_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fresh.txt");

        write_atomic(&path, b"data").unwrap();

        assert_eq!(read(&path), "data");
    }

    #[test]
    fn write_atomic_never_touches_a_foreign_file_with_the_temp_name() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("doc.txt");
        fs::write(&path, "old").unwrap();
        let foreign = temp_candidate(&path, 0).unwrap();
        fs::write(&foreign, "someone else's").unwrap();

        write_atomic(&path, b"new").unwrap();

        assert_eq!(read(&path), "new");
        assert_eq!(read(&foreign), "someone else's");
        assert_eq!(entry_count(temp.path()), 2);
    }

    #[cfg(windows)]
    #[test]
    fn failed_write_atomic_keeps_old_content_and_cleans_up() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("locked.txt");
        fs::write(&path, "old").unwrap();
        // Another program holding the file without sharing makes the final rename fail.
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();

        let result = write_atomic(&path, b"new");
        drop(lock);

        assert!(result.is_err());
        assert_eq!(read(&path), "old");
        assert_eq!(entry_count(temp.path()), 1);
    }
}

#[cfg(test)]
#[path = "ops_save_tests.rs"]
mod save_tests;
