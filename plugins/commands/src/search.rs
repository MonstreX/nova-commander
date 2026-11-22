//! `whereis:` as FAR's farcmds looks (`__proc_WhereIs`): the name as it is, then with each extension of
//! `PATHEXT`; each in the current folder, the program's folder (FAR's `%FARHOME%`), the folders of
//! `PATH`, then where Windows' `SearchPath` looks by itself (the program's folder, the system's folders,
//! Windows' folder); last the programs registered in `App Paths` (the user's, the machine's).

use std::path::{Path, PathBuf};

use crate::text;

/// The names to try: `name`, then `name` with each extension of `PATHEXT`.
fn candidates(name: &str) -> Vec<String> {
    let extensions = std::env::var("PATHEXT").unwrap_or_default();
    std::iter::once(name.to_string())
        .chain(
            (extensions.split(';'))
                .filter(|extension| !extension.is_empty())
                .map(|extension| format!("{name}{extension}")),
        )
        .collect()
}

/// The folders of a variable that is a list (`PATH`).
fn listed(variable: &str) -> Vec<PathBuf> {
    let value = std::env::var(variable).unwrap_or_default();
    (value.split(';'))
        .map(|folder| text::unquote(folder.trim()))
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Where Windows' `SearchPath` looks without folders given: the program's folder, the current one, the
/// system's (32-bit, 16-bit), Windows' own, `PATH`.
fn system_folders(program: &Path, current: &Path) -> Vec<PathBuf> {
    let windows = std::env::var("SystemRoot").map(PathBuf::from).ok();
    [program.to_path_buf(), current.to_path_buf()]
        .into_iter()
        .chain(windows.iter().flat_map(|windows| {
            [
                windows.join("System32"),
                windows.join("System"),
                windows.clone(),
            ]
        }))
        .chain(listed("PATH"))
        .collect()
}

/// `name` in the first of `folders` that has it (a folder too when `folders_too`).
fn first_in(name: &str, folders: &[PathBuf], folders_too: bool) -> Option<PathBuf> {
    let takes = |path: &Path| path.is_file() || (folders_too && path.is_dir());
    let path = Path::new(name);
    if path.is_absolute() {
        return takes(path).then(|| path.to_path_buf());
    }
    (folders.iter())
        .map(|folder| folder.join(name))
        .find(|path| takes(path))
}

/// The object `name` looked for from the folder `current`, `program` the program's folder; a folder taken
/// when `folders_too` (`whereis:` does, a command's program does not). None: not found.
pub fn find(name: &str, current: &Path, program: &Path, folders_too: bool) -> Option<PathBuf> {
    let expanded = text::expand(name);
    let ours: Vec<PathBuf> = [current.to_path_buf(), program.to_path_buf()]
        .into_iter()
        .chain(listed("PATH"))
        .collect();
    let system = system_folders(program, current);
    let found = (candidates(&expanded).iter()).find_map(|candidate| {
        first_in(candidate, &ours, folders_too)
            .or_else(|| first_in(candidate, &system, folders_too))
    });
    found.or_else(|| registered(name))
}

/// The program registered under `name` (or `name` with an extension of `PATHEXT`) in `App Paths`.
#[cfg(windows)]
fn registered(name: &str) -> Option<PathBuf> {
    (candidates(name).iter())
        .find_map(|candidate| nc_plugin_sdk::system::app_path(candidate))
        .map(|path| PathBuf::from(text::expand(&path)))
}

#[cfg(not(windows))]
fn registered(_name: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_folder_comes_first_then_the_extensions() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("current");
        let program = temp.path().join("program");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&program).unwrap();
        std::fs::write(program.join("tool.txt"), "").unwrap();
        std::fs::write(current.join("tool.txt"), "").unwrap();
        assert_eq!(
            find("tool.txt", &current, &program, true),
            Some(current.join("tool.txt"))
        );
        std::fs::write(program.join("only.exe"), "").unwrap();
        let found = find("only", &current, &program, true).unwrap();
        // The name as asked, with PATHEXT's extension (FAR's `SearchPath` gives it so).
        assert_eq!(
            found.to_string_lossy().to_lowercase(),
            program.join("only.exe").to_string_lossy().to_lowercase(),
            "the program's folder, PATHEXT's .EXE"
        );
    }

    #[test]
    fn a_folder_is_found_only_when_asked() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("sub")).unwrap();
        let found = find("sub", temp.path(), temp.path(), true);
        assert_eq!(found, Some(temp.path().join("sub")));
        assert_eq!(find("sub", temp.path(), temp.path(), false), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_systems_folders_have_its_programs() {
        let temp = tempfile::tempdir().unwrap();
        let found = find("cmd", temp.path(), temp.path(), true).unwrap();
        assert!(
            found.to_string_lossy().to_lowercase().ends_with("cmd.exe"),
            "{found:?}"
        );
    }
}
