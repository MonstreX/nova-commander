//! Where Git for Windows keeps its bash: the folder its installer writes to the registry
//! (`HKLM` or, for one user, `HKCU` `Software\GitForWindows`, `InstallPath`), else beside a `git.exe`
//! along `PATH` (`<git>\cmd\git.exe`, `<git>\mingw64\bin\git.exe`).

use std::path::{Path, PathBuf};

use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use super::registry::string;

const GIT_KEY: &str = r"Software\GitForWindows";
/// The launcher that sets up Git's environment (`MSYSTEM`, its `PATH`) and starts its bash.
const BASH: &str = r"bin\bash.exe";

/// Git Bash's program file, if Git for Windows is installed.
pub fn git_bash() -> Option<PathBuf> {
    let installed = [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
        .into_iter()
        .filter_map(|key| string(key, GIT_KEY, "InstallPath"))
        .map(PathBuf::from);
    installed
        .chain(beside_git())
        .map(|folder| folder.join(BASH))
        .find(|bash| bash.is_file())
}

/// The Git folders above each `git.exe` along `PATH`.
fn beside_git() -> Vec<PathBuf> {
    let Some(paths) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    std::env::split_paths(&paths)
        .filter(|dir| dir.join("git.exe").is_file())
        .flat_map(|dir| {
            let up: Vec<PathBuf> = dir
                .ancestors()
                .skip(1)
                .take(2)
                .map(Path::to_path_buf)
                .collect();
            up
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_value_is_none() {
        assert_eq!(
            string(HKEY_CURRENT_USER, r"Software\NoSuchKeyOfVc", "x"),
            None
        );
    }

    #[test]
    fn what_is_found_is_a_bash_program() {
        if let Some(bash) = git_bash() {
            assert!(bash.ends_with(BASH), "{}", bash.display());
            assert!(bash.is_file());
        }
    }
}
