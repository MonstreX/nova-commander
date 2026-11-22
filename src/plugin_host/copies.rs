//! A plugin's library loaded from a copy beside it (P6.0e): the file the user installed is never held, so
//! a plugin can be replaced, unloaded and loaded again while the program runs, and one left hanging can be
//! started anew (a library's own copy is a module of its own: the system loads one path once). The copy
//! lies in the plugin's own folder, where the libraries it needs are found; it is removed as its thread
//! ends, and the copies a program before left (a crash, a plugin left hanging) as the plugins are loaded.
//! A folder the program cannot write to has its library loaded as it is.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// What a copy's name has: `.<library>.nc-<pid>-<n>.<extension>`.
const MARK: &str = ".vc-";

/// The copies made by this run, for their names.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// The library to load for `library`: a copy beside it, or (none can be made) itself; whether a copy.
pub(super) fn loadable(library: &Path) -> (PathBuf, bool) {
    let (Some(folder), Some(name)) = (library.parent(), library.file_name()) else {
        return (library.to_path_buf(), false);
    };
    let name = name.to_string_lossy();
    let (stem, extension) = name.rsplit_once('.').unwrap_or((&name, ""));
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    let copy = folder.join(format!(
        ".{stem}{MARK}{}-{number}.{extension}",
        std::process::id()
    ));
    match std::fs::copy(library, &copy) {
        Ok(_) => (copy, true),
        Err(_) => (library.to_path_buf(), false),
    }
}

/// The copies in `folder` that no program holds any more removed (one loaded now cannot be).
pub(super) fn sweep(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.contains(MARK) && entry.path().is_file() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_lies_beside_its_library_and_the_left_ones_are_swept() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("probe.dll");
        std::fs::write(&library, b"code").unwrap();
        let (first, copied) = loadable(&library);
        let (second, _) = loadable(&library);
        assert!(copied);
        assert_ne!(first, second, "each a module of its own");
        assert_eq!(first.parent(), Some(temp.path()));
        let name = first.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(".probe.vc-") && name.ends_with(".dll"),
            "{name}"
        );
        assert_eq!(std::fs::read(&first).unwrap(), b"code");
        std::fs::write(temp.path().join(".notes"), "").unwrap();
        sweep(temp.path());
        assert!(!first.exists() && !second.exists());
        assert!(
            library.exists() && temp.path().join(".notes").exists(),
            "only copies"
        );
        let missing = temp.path().join("none.dll");
        assert_eq!(loadable(&missing), (missing.clone(), false), "as it is");
    }
}
