//! Making links (`far/flink.cpp`: `MkHardLink`, `MkSymLink`, `CreateReparsePoint`): a hard link, a
//! junction, a symbolic link to a file or a folder. Nothing that exists is ever replaced. A junction to
//! a drive's root, which FAR turns into a volume mount point, comes with F7's link type (2b): no
//! selected item is a drive's root.

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::OwnedHandle;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_WRITE_ATTRIBUTES,
};
use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

use super::device::control;
use crate::platform::files::NewLink;

/// `IO_REPARSE_TAG_MOUNT_POINT`: a junction.
pub(super) const TAG_MOUNT_POINT: u32 = 0xA000_0003;
/// `REPARSE_DATA_BUFFER_HEADER_SIZE`: the tag, the data length and a reserved word.
pub(super) const HEADER: usize = 8;
/// `MAXIMUM_REPARSE_DATA_BUFFER_SIZE`.
pub(super) const MAXIMUM_BUFFER: usize = 16 * 1024;

/// Makes `link` a new link of `kind` to `target`. A symbolic link keeps `target` in FAR's form (`C:\x`,
/// not `\\?\C:\x`; a relative one as it is, counted from the link's folder); a junction leads to the
/// full path.
pub fn make_link(kind: NewLink, target: &Path, link: &Path) -> io::Result<()> {
    let shown = || PathBuf::from(OsString::from_wide(&plain_form(target).1));
    match kind {
        NewLink::Hard => std::fs::hard_link(target, link),
        NewLink::Junction => junction(target, link),
        NewLink::Symbolic if leads_to_folder(target, link) => {
            std::os::windows::fs::symlink_dir(shown(), link)
        }
        NewLink::Symbolic | NewLink::SymbolicFile => {
            std::os::windows::fs::symlink_file(shown(), link)
        }
        NewLink::SymbolicFolder => std::os::windows::fs::symlink_dir(shown(), link),
    }
}

/// FAR's "symbolic link" is a folder link when the target is a folder.
fn leads_to_folder(target: &Path, link: &Path) -> bool {
    match link.parent() {
        Some(dir) if target.is_relative() => dir.join(target).is_dir(),
        _ => target.is_dir(),
    }
}

/// A new folder made a junction as `mklink /J` makes one; the folder goes again when that fails.
fn junction(target: &Path, link: &Path) -> io::Result<()> {
    let (substitute, print) = junction_names(&std::path::absolute(target)?);
    let buffer = mount_point_buffer(&substitute, &print).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "the target's name is too long")
    })?;
    std::fs::create_dir(link)?;
    let set = set_reparse_point(link, &buffer);
    if set.is_err() {
        let _ = std::fs::remove_dir(link);
    }
    set
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// A path as FAR shows and keeps it (`C:\x`, `\\server\share`): without `\\?\` or `\\.\`; and
/// whether it is a network path.
fn plain_form(path: &Path) -> (bool, Vec<u16>) {
    let path: Vec<u16> = path.as_os_str().encode_wide().collect();
    let prefixed = [r"\\?\", r"\\.\"]
        .iter()
        .find_map(|prefix| path.strip_prefix(units(prefix).as_slice()));
    match prefixed {
        Some(rest) => match rest.strip_prefix(units(r"UNC\").as_slice()) {
            Some(share) => (true, [units(r"\\"), share.to_vec()].concat()),
            None => (false, rest.to_vec()),
        },
        None => (path.starts_with(&units(r"\\")), path.clone()),
    }
}

/// The names a junction keeps (FAR's `kernel_path(nt_path(...))` and the full path): `\??\C:\x` and
/// `C:\x`; `\??\UNC\server\share` and `\\server\share`.
pub(super) fn junction_names(full: &Path) -> (Vec<u16>, Vec<u16>) {
    let (unc, plain) = plain_form(full);
    let substitute = if unc {
        [units(r"\??\UNC\"), plain[2..].to_vec()].concat()
    } else {
        [units(r"\??\"), plain.clone()].concat()
    };
    (substitute, plain)
}

/// A `REPARSE_DATA_BUFFER` for a junction, laid out as Windows lays its own: the substitute name, a
/// NUL, the print name, a NUL. `None` when the names do not fit.
pub(super) fn mount_point_buffer(substitute: &[u16], print: &[u16]) -> Option<Vec<u8>> {
    let substitute_length = substitute.len() * 2;
    let print_offset = substitute_length + 2;
    let print_length = print.len() * 2;
    // The four offsets and lengths, then both names with their NULs.
    let data_length = 8 + print_offset + print_length + 2;
    if HEADER + data_length > MAXIMUM_BUFFER {
        return None;
    }
    let word = |value: usize| u16::try_from(value).ok().map(u16::to_le_bytes);
    let mut buffer = TAG_MOUNT_POINT.to_le_bytes().to_vec();
    for value in [
        data_length,
        0,
        0,
        substitute_length,
        print_offset,
        print_length,
    ] {
        buffer.extend(word(value)?);
    }
    for unit in substitute.iter().chain(&[0]).chain(print).chain(&[0]) {
        buffer.extend(unit.to_le_bytes());
    }
    Some(buffer)
}

pub(super) fn set_reparse_point(path: &Path, buffer: &[u8]) -> io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    control(
        &OwnedHandle::from(file),
        FSCTL_SET_REPARSE_POINT,
        buffer,
        &mut [],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::files::link_target;

    fn text(units: &[u16]) -> String {
        String::from_utf16_lossy(units)
    }

    #[test]
    fn a_junction_keeps_the_kernel_name_and_the_full_path() {
        let (substitute, print) = junction_names(Path::new(r"C:\work\x"));
        assert_eq!(
            (text(&substitute), text(&print)),
            (r"\??\C:\work\x".into(), r"C:\work\x".into())
        );
        let (substitute, print) = junction_names(Path::new(r"\\?\C:\long"));
        assert_eq!(
            (text(&substitute), text(&print)),
            (r"\??\C:\long".into(), r"C:\long".into())
        );
        let (substitute, print) = junction_names(Path::new(r"\\server\share\x"));
        assert_eq!(text(&substitute), r"\??\UNC\server\share\x");
        assert_eq!(text(&print), r"\\server\share\x");
        let (substitute, _) = junction_names(Path::new(r"\\?\UNC\server\share"));
        assert_eq!(text(&substitute), r"\??\UNC\server\share");
        let (substitute, print) = junction_names(Path::new(r"\\.\C:\x"));
        assert_eq!(
            (text(&substitute), text(&print)),
            (r"\??\C:\x".into(), r"C:\x".into())
        );
    }

    #[test]
    fn the_buffer_is_laid_out_as_windows_lays_it() {
        let units = |s: &str| -> Vec<u16> { s.encode_utf16().collect() };
        let buffer = mount_point_buffer(&units(r"\??\C:\x"), &units(r"C:\x")).unwrap();
        let word = |at: usize| u16::from_le_bytes([buffer[at], buffer[at + 1]]);
        assert_eq!(&buffer[..4], &TAG_MOUNT_POINT.to_le_bytes());
        assert_eq!(usize::from(word(4)), buffer.len() - HEADER);
        assert_eq!((word(8), word(10), word(12), word(14)), (0, 16, 18, 8));
        assert_eq!(buffer.len(), HEADER + 8 + 16 + 2 + 8 + 2);
        let long = vec![u16::from(b'x'); MAXIMUM_BUFFER];
        assert_eq!(mount_point_buffer(&long, &long), None);
    }

    #[test]
    fn a_junction_leads_to_the_folder_by_its_full_path() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("inside.txt"), "x").unwrap();
        let link = temp.path().join("link");

        make_link(NewLink::Junction, &target, &link).unwrap();

        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(link_target(&link).unwrap(), target);
        assert_eq!(
            std::fs::read_to_string(link.join("inside.txt")).unwrap(),
            "x"
        );
        assert_eq!(
            crate::platform::files::reparse_tag(&link),
            Some(TAG_MOUNT_POINT)
        );
    }

    #[test]
    fn nothing_that_exists_is_replaced_and_a_failed_junction_leaves_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let taken = temp.path().join("taken");
        std::fs::create_dir(&taken).unwrap();
        std::fs::write(taken.join("keep.txt"), "x").unwrap();
        for kind in [NewLink::Junction, NewLink::Symbolic, NewLink::Hard] {
            assert!(make_link(kind, &target, &taken).is_err(), "{kind:?}");
        }
        assert!(taken.join("keep.txt").is_file());
        let too_long = PathBuf::from(format!(r"C:\{}", "x".repeat(MAXIMUM_BUFFER)));
        let link = temp.path().join("link");
        assert!(make_link(NewLink::Junction, &too_long, &link).is_err());
        assert!(!link.exists());
    }

    #[test]
    fn a_hard_link_is_the_same_file() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let link = temp.path().join("link.txt");

        make_link(NewLink::Hard, &file, &link).unwrap();

        assert_eq!(
            crate::platform::files::file_identity(&link).unwrap(),
            crate::platform::files::file_identity(&file).unwrap()
        );
    }

    /// What `dir` shows a link leads to: the print name, as the user sees it.
    fn shown_target(dir: &Path) -> String {
        let out = std::process::Command::new("cmd")
            .args(["/C", "dir", "/AL"])
            .arg(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A symbolic link needs Developer Mode or administrator rights.
    #[test]
    fn a_symbolic_link_keeps_its_target_in_fars_form() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let verbatim = crate::platform::files::exact_path(&file);
        assert!(
            verbatim.to_string_lossy().starts_with(r"\\?\"),
            "{verbatim:?}"
        );

        make_link(NewLink::Symbolic, &verbatim, &temp.path().join("link")).unwrap();

        let shown = shown_target(temp.path());
        assert!(shown.contains(&format!("[{}]", file.display())), "{shown}");
        assert!(!shown.contains(r"?\"), "{shown}");
    }

    /// A symbolic link needs Developer Mode or administrator rights.
    #[test]
    fn a_symbolic_link_is_a_folder_link_when_its_target_is_a_folder() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("dir")).unwrap();
        std::fs::write(temp.path().join("file.txt"), "x").unwrap();
        let folder = temp.path().join("to_dir");
        let file = temp.path().join("to_file");
        let forced = temp.path().join("forced");

        make_link(NewLink::Symbolic, Path::new("dir"), &folder).unwrap();
        make_link(NewLink::Symbolic, &temp.path().join("file.txt"), &file).unwrap();
        make_link(NewLink::SymbolicFolder, Path::new("file.txt"), &forced).unwrap();

        let attributes = |path: &Path| {
            use std::os::windows::fs::MetadataExt;
            std::fs::symlink_metadata(path).unwrap().file_attributes()
        };
        const DIRECTORY: u32 = 0x10;
        assert_ne!(attributes(&folder) & DIRECTORY, 0);
        assert_eq!(attributes(&file) & DIRECTORY, 0);
        assert_ne!(attributes(&forced) & DIRECTORY, 0, "the kind asked for");
        assert_eq!(
            link_target(&folder).unwrap(),
            PathBuf::from("dir"),
            "kept as given"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "x");
    }
}
