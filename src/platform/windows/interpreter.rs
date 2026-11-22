//! cmd, the command interpreter (ADR-0006), as FAR knows it (`execute.cpp`): which lines it gets, its own
//! commands, how a program is found, `%VAR%`, the folders it cannot stand in, `cd /d`.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf, Prefix};

/// cmd's own commands (FAR's `PredefinedCmdCommands`): never a program, even if one has the name.
const CMD_COMMANDS: [&str; 41] = [
    "ASSOC", "CALL", "CD", "CHCP", "CHDIR", "CLS", "COLOR", "COPY", "DATE", "DEL", "DIR", "DPATH",
    "ECHO", "ERASE", "EXIT", "FOR", "FTYPE", "IF", "KEYS", "MD", "MKDIR", "MKLINK", "MOVE", "PATH",
    "PAUSE", "POPD", "PROMPT", "PUSHD", "RD", "REM", "REN", "RENAME", "RMDIR", "SET", "START",
    "TIME", "TITLE", "TYPE", "VER", "VERIFY", "VOL",
];

/// FAR's (`os::env::get_pathext`) and cmd's extensions when `PATHEXT` is empty.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC";

/// Environment variable names do not differ by case (`Path` is `PATH`), as cmd's `set`.
pub const VARIABLE_NAMES_DIFFER_BY_CASE: bool = false;

/// `cd /d <dir>` changes the drive too (FAR's `IntChDir` takes the switch as cmd does).
pub const CD_TAKES_DRIVE_SWITCH: bool = true;

/// Shown instead of running a line where the command interpreter cannot stand.
pub const INTERPRETER_FOLDER_REFUSAL: &str = "The command interpreter cannot run commands in this \
     folder (a network or \\\\?\\ path): it would run them in the Windows folder. The command was not \
     run. Use a drive letter, or the guest shell (Alt+Shift+T) with PowerShell.";

/// Whether `word` is one of cmd's own commands.
fn is_interpreter_command(word: &str) -> bool {
    CMD_COMMANDS
        .iter()
        .any(|command| command.eq_ignore_ascii_case(word))
}

/// FAR's `PartCmdLine`: the program of a simple command line, without its quotes, and its parameters
/// as typed. `None` leaves the line to the command interpreter: `<`, `>`, `|` or `&` outside quotes, or
/// one of its own commands.
pub(super) fn part_line(line: &str) -> Option<(String, String)> {
    let line = line.trim_start();
    let mut quoted = false;
    let mut end = None;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '<' | '>' | '|' | '&' if !quoted => return None,
            ' ' | '\t' if !quoted && end.is_none() => end = Some(index),
            _ => {}
        }
    }
    let (word, parameters) = match end {
        Some(end) => (&line[..end], &line[end + 1..]),
        None => (line, ""),
    };
    if is_interpreter_command(word) {
        return None;
    }
    Some((word.replace('"', ""), parameters.to_string()))
}

/// cmd's search: a name with a folder in it only there (relative to `dir`), a bare name in `dir` and
/// then along `PATH`, each with the `PATHEXT` extensions in their own order (`.COM` before `.EXE`).
pub(super) fn find_program(name: &str, dir: &Path) -> Option<PathBuf> {
    let mut folders = vec![dir.to_path_buf()];
    if !name.contains(['\\', '/', ':']) {
        let path = std::env::var_os("PATH").unwrap_or_default();
        folders.extend(std::env::split_paths(&path));
    }
    let path_ext = std::env::var("PATHEXT").unwrap_or_default();
    let extensions = search_extensions(&path_ext);
    folders
        .iter()
        .find_map(|folder| find_in_folder(&folder.join(name), &extensions))
}

/// The extensions to try, in `PATHEXT`'s own order.
fn search_extensions(path_ext: &str) -> Vec<String> {
    let path_ext = if path_ext.trim().is_empty() {
        DEFAULT_PATHEXT
    } else {
        path_ext
    };
    path_ext
        .split(';')
        .map(|extension| extension.trim().trim_start_matches('.').to_string())
        .filter(|extension| !extension.is_empty())
        .collect()
}

/// FAR's `FindObject` in one folder: a name with an extension as it is first, then the name with each
/// extension added.
fn find_in_folder(base: &Path, extensions: &[String]) -> Option<PathBuf> {
    if base.extension().is_some() && base.is_file() {
        return Some(base.to_path_buf());
    }
    extensions.iter().find_map(|extension| {
        let mut candidate = OsString::from(base.as_os_str());
        candidate.push(format!(".{extension}"));
        let candidate = PathBuf::from(candidate);
        candidate.is_file().then_some(candidate)
    })
}

/// A folder cmd cannot have as its current folder: anything but a plain drive path (`C:\...`). For a
/// network path (`\\server\share`), `\\?\` or `\\.\` cmd warns and runs the line in the Windows
/// folder instead (`del *` there); FAR lets that happen.
pub(super) fn interpreter_cannot_run_in(dir: &Path) -> bool {
    !matches!(
        dir.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
    )
}

/// FAR's `EnumModules`: cmd's own commands and the programs along `PATH` that begin with `start`.
pub fn completion_programs(start: &str, path: &std::ffi::OsStr) -> Vec<String> {
    let lower = start.to_lowercase();
    let mut names: Vec<String> = CMD_COMMANDS
        .iter()
        .filter(|command| command.to_lowercase().starts_with(&lower))
        .map(ToString::to_string)
        .collect();
    let extensions: Vec<String> = search_extensions(&std::env::var("PATHEXT").unwrap_or_default())
        .iter()
        .map(|extension| extension.to_lowercase())
        .collect();
    for folder in std::env::split_paths(path) {
        for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_program = Path::new(&name).extension().is_some_and(|extension| {
                extensions.contains(&extension.to_string_lossy().to_lowercase())
            });
            if is_program && name.to_lowercase().starts_with(&lower) {
                names.push(name);
            }
        }
    }
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by_key(|name| name.to_lowercase());
    names
}

/// FAR's `EnumEnvironment`: `%NAME%` of those of `names` that begin as `start` does (`%` included).
pub fn completion_variables(start: &str, names: &[String]) -> Vec<String> {
    if !start.starts_with('%') {
        return Vec::new();
    }
    let lower = start.to_lowercase();
    names
        .iter()
        .map(|name| format!("%{name}%"))
        .filter(|name| name.to_lowercase().starts_with(&lower))
        .collect()
}

/// Replaces `%NAME%` with what `value` gives for it; unknown names stay as written, as in cmd.
pub fn expand_variables(text: &str, value: &dyn Fn(&str) -> Option<String>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let found = after.find('%').and_then(|end| {
            let name = &after[..end];
            let found = (!name.is_empty()).then(|| value(name)).flatten()?;
            Some((found, end))
        });
        match found {
            Some((found, end)) => {
                result.push_str(&found);
                rest = &after[end + 1..];
            }
            None => {
                result.push('%');
                rest = after;
            }
        }
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parted(line: &str) -> Option<(String, String)> {
        part_line(line)
    }

    fn pair(program: &str, parameters: &str) -> Option<(String, String)> {
        Some((program.to_string(), parameters.to_string()))
    }

    #[test]
    fn a_simple_line_is_its_program_and_parameters() {
        assert_eq!(parted("notepad a.txt"), pair("notepad", "a.txt"));
        assert_eq!(
            parted("  \"C:\\Program Files\\x.exe\" -a  \"b c\""),
            pair("C:\\Program Files\\x.exe", "-a  \"b c\"")
        );
        assert_eq!(
            parted("notepad \"a&b.txt\""),
            pair("notepad", "\"a&b.txt\"")
        );
        assert_eq!(parted("tool"), pair("tool", ""));
        assert_eq!(parted("   "), pair("", ""));
    }

    #[test]
    fn complex_lines_and_cmds_own_commands_are_left_to_it() {
        for complex in ["dir | more", "notepad > x", "a && b", "x < in"] {
            assert_eq!(parted(complex), None, "{complex}");
        }
        for own in ["dir /b", "Start x", "ECHO a", "cls"] {
            assert_eq!(parted(own), None, "{own}");
        }
        assert_eq!(parted("\"dir\" /b"), pair("dir", "/b"), "quoted: a program");
    }

    #[test]
    fn programs_come_from_cmd_and_from_the_path_given() {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let found = completion_programs("notep", &path);
        assert!(
            found
                .iter()
                .any(|name| name.eq_ignore_ascii_case("notepad.exe")),
            "{found:?}"
        );
        let own = completion_programs("mkli", &path);
        assert!(own.iter().any(|name| name == "MKLINK"), "{own:?}");
        assert!(completion_programs("zzz-no-such-4e1", &path).is_empty());
        assert!(
            !completion_programs("notep", "".as_ref())
                .iter()
                .any(|name| name.eq_ignore_ascii_case("notepad.exe")),
            "an empty PATH finds nothing"
        );
    }

    #[test]
    fn variables_are_offered_only_for_a_word_with_a_percent_sign() {
        let names = ["PATH".to_string(), "HOME".to_string()];
        assert!(completion_variables("PAT", &names).is_empty());
        assert_eq!(completion_variables("%pat", &names), ["%PATH%"]);
    }

    #[test]
    fn extensions_are_tried_in_pathext_order() {
        assert_eq!(
            search_extensions(".COM;.EXE; .bat;;"),
            ["COM", "EXE", "bat"]
        );
        assert_eq!(search_extensions("")[..2], ["COM", "EXE"]);
        let temp = tempfile::tempdir().unwrap();
        for name in ["tool.exe", "tool.com", "my.app", "my.app.exe"] {
            std::fs::write(temp.path().join(name), "").unwrap();
        }
        let found = |name: &str, path_ext: &str| {
            find_in_folder(&temp.path().join(name), &search_extensions(path_ext))
                .map(|path| path.file_name().unwrap().to_string_lossy().to_lowercase())
        };
        assert_eq!(found("tool", ".COM;.EXE").as_deref(), Some("tool.com"));
        assert_eq!(found("tool", ".EXE;.COM").as_deref(), Some("tool.exe"));
        assert_eq!(found("my.app", ".EXE").as_deref(), Some("my.app"));
        assert_eq!(found("none", ".EXE"), None);
    }

    #[test]
    fn the_folder_comes_before_path() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("notepad.bat"), "@echo off\r\n").unwrap();
        // The extension comes as `PATHEXT` spells it.
        let found = find_program("notepad", temp.path()).unwrap();
        assert!(
            found
                .to_string_lossy()
                .eq_ignore_ascii_case(&temp.path().join("notepad.bat").to_string_lossy())
        );
        let elsewhere = tempfile::tempdir().unwrap();
        let found = find_program("notepad", elsewhere.path()).unwrap();
        assert!(
            found
                .to_string_lossy()
                .to_lowercase()
                .ends_with("notepad.exe")
        );
        assert_eq!(
            find_program("sub\\notepad", elsewhere.path()),
            None,
            "a folder: only there"
        );
    }

    #[test]
    fn the_interpreter_runs_only_in_plain_drive_folders() {
        assert!(!interpreter_cannot_run_in(Path::new(r"C:\dir")));
        assert!(!interpreter_cannot_run_in(Path::new(r"e:\")));
        for refused in [
            r"\\server\share\dir",
            r"\\?\UNC\server\share",
            r"\\?\C:\dir",
            r"\\.\C:\dir",
            r"\dir",
            "dir",
        ] {
            assert!(interpreter_cannot_run_in(Path::new(refused)), "{refused}");
        }
    }

    #[test]
    fn variables_are_expanded_as_cmd_does() {
        let value = |name: &str| (name == "B").then(|| "bee".to_string());
        assert_eq!(expand_variables("x%B%y", &value), "xbeey");
        assert_eq!(expand_variables("%C% 100% %%", &value), "%C% 100% %%");
        assert_eq!(expand_variables("%B%%B%", &value), "beebee");
    }
}
