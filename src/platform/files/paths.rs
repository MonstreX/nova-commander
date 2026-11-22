//! Path rules that differ by OS: what a root is, a drive typed by its letter alone, the letter of a
//! path's drive. `std` already knows the separators (`std::path::is_separator`, `MAIN_SEPARATOR`) and
//! the prefixes (`C:`, `\\server\share`); the long-path form a raw Win32 call needs is the Windows
//! implementation's own business.

use std::path::{Component, Path, PathBuf, Prefix};

/// Whether `path` is the root of its volume (`C:\`, `\\server\share\`, `/`): there is no folder above.
pub fn is_root(path: &Path) -> bool {
    path.parent().is_none()
}

/// `D:\` for `D:` typed alone (cmd and FAR go to that drive); `None` for anything else and on systems
/// without drive letters.
pub fn bare_drive_root(input: &str) -> Option<PathBuf> {
    let bytes = input.as_bytes();
    let bare = bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    (cfg!(windows) && bare).then(|| PathBuf::from(format!("{input}\\")))
}

/// `C` of `C:\folder` (upper case); `None` for a path with no drive letter (a network path, a volume
/// with no letter, any path on Unix).
pub fn volume_letter(path: &Path) -> Option<char> {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) => Some(char::from(letter).to_ascii_uppercase()),
            _ => None,
        },
        _ => None,
    }
}

/// FAR's `GetPathRoot` without its end separator: `C:` of `C:\dir`, `\\server\share` of a network
/// path; empty for a path with neither (a relative one, any path on Unix).
pub fn root_name(dir: &str) -> String {
    match Path::new(dir).components().next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str().to_string_lossy().into_owned(),
        _ => String::new(),
    }
}

/// Whether `path` is on a network share named by its server (`\\server\share\dir`), not by a drive.
pub fn is_share(path: &Path) -> bool {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => {
            matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))
        }
        _ => false,
    }
}

/// Whether a typed folder part ends at `ch`: at a separator, and where there are drives at a drive's
/// colon (`C:name`).
pub fn ends_folder_part(ch: char) -> bool {
    std::path::is_separator(ch) || (cfg!(windows) && ch == ':')
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn roots_and_letters_of_windows_paths() {
        assert!(is_root(Path::new(r"C:\")));
        assert!(is_root(Path::new(r"\\server\share\")));
        assert!(!is_root(Path::new(r"C:\Windows")));
        assert_eq!(bare_drive_root("d:"), Some(PathBuf::from(r"d:\")));
        assert_eq!(bare_drive_root("dd"), None);
        assert_eq!(volume_letter(Path::new(r"c:\folder")), Some('C'));
        assert_eq!(volume_letter(Path::new(r"\\server\share")), None);
        assert_eq!(volume_letter(Path::new(r"\\?\Volume{0}\")), None);
    }

    #[test]
    fn roots_shares_and_names_of_windows_paths() {
        assert_eq!(root_name(r"C:\dir"), "C:");
        assert_eq!(root_name(r"\\pc\s\x"), r"\\pc\s");
        assert_eq!(root_name(r"dir\x"), "");
        assert!(is_share(Path::new(r"\\pc\s\x")));
        assert!(!is_share(Path::new(r"C:\x")));
        assert!(ends_folder_part(':') && ends_folder_part('/') && !ends_folder_part('a'));
        let is_plain_name = crate::platform::files::is_plain_name;
        assert!(is_plain_name("a b.txt"));
        for odd in ["", ".", "..", "x/", r"a\b", "ab:c", "C:"] {
            assert!(!is_plain_name(odd), "{odd}");
        }
    }
}
