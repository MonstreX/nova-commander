//! Links (`far/flink.cpp`): a link's target and copying a link as a link (FAR's "exact copy" when
//! "Copy contents of symbolic links" is off).

use std::io;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::CreateDirectoryExW;

use super::wide_path;

/// What a link points to, as FAR shows it (`NormalizeSymlinkName`: `\??\C:\x` and `\\?\C:\x` become
/// `C:\x`; relative targets stay relative).
pub fn link_target(link: &Path) -> io::Result<PathBuf> {
    let target = std::fs::read_link(link)?;
    let text = target.to_string_lossy();
    let plain = ["\\??\\", "\\\\?\\"].iter().find_map(|prefix| {
        let rest = text.strip_prefix(prefix)?;
        let bytes = rest.as_bytes();
        (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
            .then(|| PathBuf::from(rest))
    });
    Ok(plain.unwrap_or(target))
}

/// Creates `destination` as a copy of the link `source` (not of what it points to): a link to a folder
/// (symbolic link or junction) through `CreateDirectoryExW` with the link as the template, as FAR does;
/// a link to a file as a symbolic link with the same target.
pub fn copy_link(source: &Path, destination: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if !super::attributes(source, &metadata).is_directory() {
        // FAR's form of the target (`C:\x`, not `\\?\C:\x`), so the copy shows it as the original does.
        return std::os::windows::fs::symlink_file(link_target(source)?, destination);
    }
    let template = wide_path(source);
    let created = wide_path(destination);
    // SAFETY: both paths are NUL-terminated UTF-16 strings that outlive the call; no security
    // attributes are passed.
    let ok = unsafe { CreateDirectoryExW(template.as_ptr(), created.as_ptr(), std::ptr::null()) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap()
            .status;
        assert!(status.success());
    }

    #[test]
    fn a_copied_junction_is_a_junction_to_the_same_folder() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("inside.txt"), "x").unwrap();
        let link = temp.path().join("link");
        junction(&link, &target);

        let copy = temp.path().join("copy");
        copy_link(&link, &copy).unwrap();

        assert!(std::fs::symlink_metadata(&copy).unwrap().is_symlink());
        assert_eq!(link_target(&copy).unwrap(), target);
        assert!(copy.join("inside.txt").is_file());
    }

    /// A symbolic link needs Developer Mode or administrator rights.
    #[test]
    fn a_copied_file_link_keeps_its_relative_target() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file.txt"), "x").unwrap();
        let link = temp.path().join("link.txt");
        std::os::windows::fs::symlink_file("file.txt", &link).unwrap();

        let copy = temp.path().join("copy.txt");
        copy_link(&link, &copy).unwrap();

        assert_eq!(link_target(&copy).unwrap(), PathBuf::from("file.txt"));
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "x");
    }

    #[test]
    fn a_missing_link_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        assert!(link_target(&temp.path().join("missing")).is_err());
        assert!(copy_link(&temp.path().join("missing"), &temp.path().join("copy")).is_err());
    }
}
