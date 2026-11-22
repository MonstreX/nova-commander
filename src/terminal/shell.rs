//! How each kind of shell speaks (ADR-0006): its prompt, its `cd`, the keys that empty its input line.

use std::path::Path;

use crate::platform;

const POWERSHELL_FILE_SYSTEM: &str = "Microsoft.PowerShell.Core\\FileSystem::";
/// Ends the lines the program types into PowerShell by itself (a comment there), so they stay out of
/// its history.
const OWN_LINE_MARK: &str = " #vc";
/// Keeps those lines out of PSReadLine's history (Up), passing others to the handler already set
/// (the default one keeps lines with secrets out; PSReadLine keeps it as a delegate, called with
/// `Invoke`). Given at start, so nothing shows on the screen.
macro_rules! powershell_history_filter {
    () => {
        "try { \
        $global:NcHistoryHandler = (Get-PSReadLineOption).AddToHistoryHandler; \
        Set-PSReadLineOption -AddToHistoryHandler { param($line) \
        if ($line.EndsWith(' #vc')) { return $false }; \
        if ($global:NcHistoryHandler) { return $global:NcHistoryHandler.Invoke($line) }; $true } \
        } catch {}"
    };
}
/// Tests leave the user's history file as it was: their lines stay in the session only.
pub(super) const POWERSHELL_HISTORY_FILTER: &str = if cfg!(test) {
    concat!(
        powershell_history_filter!(),
        " try { Set-PSReadLineOption -HistorySaveStyle SaveNothing } catch {}"
    )
} else {
    powershell_history_filter!()
};

/// Whether the directory a prompt shows is `dir` (case aside where the system ignores it, and a
/// separator at the end aside).
pub fn is_same_dir(shown: &str, dir: &Path) -> bool {
    platform::files::same_path(shown, &dir.display().to_string())
}

/// The language of a shell, as the user's own shells name it too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Cmd,
    PowerShell,
    /// bash of MSYS or Cygwin (Git Bash): Windows paths through `cygpath`.
    Msys,
    /// bash of WSL: Windows paths through `wslpath`.
    Wsl,
}

impl Dialect {
    pub const ALL: [Self; 4] = [Self::Cmd, Self::PowerShell, Self::Msys, Self::Wsl];

    /// Its name in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Self::Cmd => "cmd",
            Self::PowerShell => "powershell",
            Self::Msys => "msys",
            Self::Wsl => "wsl",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|dialect| dialect.id() == id)
            .unwrap_or(Self::Cmd)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Cmd => "cmd",
            Self::PowerShell => "PowerShell",
            Self::Msys => "bash (MSYS, Cygwin)",
            Self::Wsl => "WSL",
        }
    }

    /// The command that changes the shell's directory to `dir`. Only for shells whose prompt shows the
    /// directory (see [`Dialect::prompt_dir`]). The screen hides it (`Session::view`); its
    /// form is one people do not type (`cd/d`, the PowerShell mark), so their own `cd` lines stay.
    /// PowerShell's `Set-Location` leaves the process in the folder it was in, which Windows keeps
    /// open (the panels could not move it); the line moves the process too.
    pub fn change_dir(self, dir: &Path) -> String {
        let dir = dir.display().to_string();
        match self {
            Self::Cmd => format!("cd/d \"{dir}\""),
            // Where PowerShell really is: if `Set-Location` fails, the process stays put quietly.
            Self::PowerShell => format!(
                "Set-Location -LiteralPath {}; [Environment]::CurrentDirectory = $PWD.ProviderPath{OWN_LINE_MARK}",
                single_quoted(&dir)
            ),
            Self::Wsl => format!("cd \"$(wslpath {})\"", posix_single_quoted(&dir)),
            Self::Msys => format!("cd \"$(cygpath -u {})\"", posix_single_quoted(&dir)),
        }
    }

    /// Whether the default prompt shows the folder (cmd, PowerShell; not bash).
    pub fn prompt_shows_dir(self) -> bool {
        matches!(self, Self::Cmd | Self::PowerShell)
    }

    /// The directory the default prompt at the start of `line` shows: `C:\dir` of `C:\dir>` (cmd),
    /// of `PS C:\dir>` (PowerShell). `None` for bash, whose prompt shortens it (`~`, `/mnt/c`).
    pub fn prompt_dir(self, line: &str) -> Option<&str> {
        let end = match self {
            Self::Cmd | Self::PowerShell => self.prompt_length(line)?,
            Self::Msys | Self::Wsl => return None,
        };
        let prompt = line.get(..line.char_indices().nth(end).map_or(line.len(), |(i, _)| i))?;
        let dir = prompt.trim_end().strip_suffix('>')?;
        let dir = dir.strip_prefix("PS ").unwrap_or(dir);
        // A network folder in PowerShell: `PS Microsoft.PowerShell.Core\FileSystem::\\server\share>`.
        Some(dir.strip_prefix(POWERSHELL_FILE_SYSTEM).unwrap_or(dir))
    }

    /// Whether `line` (the line with the shell's cursor) is this shell's default prompt, i.e. the shell
    /// waits for a command rather than a program waiting for an answer (`(Y/N)?`, `>>>`, `Password:`).
    pub fn is_prompt(self, line: &str) -> bool {
        let line = line.trim_end();
        match self {
            Self::Cmd => line.ends_with('>') && starts_with_windows_path(line),
            Self::PowerShell => line.starts_with("PS ") && line.ends_with('>'),
            Self::Msys | Self::Wsl => line.ends_with('$') || line.ends_with('#'),
        }
    }

    /// The length in characters of this shell's default prompt at the start of `line`, whatever is
    /// typed after it: `C:\dir>` (cmd), `PS C:\dir> ` (PowerShell), `user@host:~$ ` (bash).
    pub fn prompt_length(self, line: &str) -> Option<usize> {
        let end = match self {
            Self::Cmd => starts_with_windows_path(line).then(|| line.find('>'))??,
            Self::PowerShell => line.starts_with("PS ").then(|| line.find('>'))??,
            Self::Msys | Self::Wsl => line.find(['$', '#'])?,
        };
        let mut length = line[..=end].chars().count();
        // PowerShell and bash put a space after the prompt sign; cmd does not.
        if self != Self::Cmd && line[end + 1..].starts_with(' ') {
            length += 1;
        }
        Some(length)
    }

    /// Keys that empty the shell's input line wherever its cursor is: Esc in cmd and PowerShell
    /// (PSReadLine's RevertLine), Ctrl+E Ctrl+U in bash.
    pub fn clear_input_keys(self) -> &'static [u8] {
        match self {
            Self::Cmd | Self::PowerShell => b"\x1b",
            Self::Msys | Self::Wsl => b"\x05\x15",
        }
    }
}

/// `C:\...` or `\\server\...`: how cmd's default prompt `$P$G` begins.
fn starts_with_windows_path(line: &str) -> bool {
    let bytes = line.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    drive || line.starts_with("\\\\")
}

/// PowerShell literal string: single quotes, embedded quotes doubled.
fn single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// POSIX shell literal string: single quotes, embedded quotes as '\''.
fn posix_single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wsl_changes_to_the_translated_windows_path() {
        let line = Dialect::Wsl.change_dir(Path::new("C:\\it's"));
        assert_eq!(line, "cd \"$(wslpath 'C:\\it'\\''s')\"");
    }

    #[test]
    fn recognises_default_prompts_only() {
        assert!(Dialect::Cmd.is_prompt("C:\\Users\\me>"));
        assert!(Dialect::Cmd.is_prompt("\\\\server\\share>  "));
        assert!(!Dialect::Cmd.is_prompt("C:\\x\\*, Are you sure (Y/N)? "));
        assert!(!Dialect::Cmd.is_prompt(">>> "));
        assert!(!Dialect::Cmd.is_prompt("Press any key to continue . . . "));
        assert!(Dialect::PowerShell.is_prompt("PS C:\\work> "));
        assert!(!Dialect::PowerShell.is_prompt("Confirm [Y] Yes [N] No: "));
        assert!(Dialect::Wsl.is_prompt("me@host:/mnt/c$ "));
        assert!(!Dialect::Wsl.is_prompt("Password: "));
    }

    #[test]
    fn prompt_length_ignores_what_is_typed_after_it() {
        assert_eq!(Dialect::Cmd.prompt_length("C:\\dir>echo a>b"), Some(7));
        assert_eq!(Dialect::Cmd.prompt_length("C:\\>"), Some(4));
        assert_eq!(Dialect::Cmd.prompt_length("Reply from 1.2.3.4"), None);
        assert_eq!(
            Dialect::PowerShell.prompt_length("PS C:\\dir> ls -r"),
            Some(11)
        );
        assert_eq!(Dialect::Wsl.prompt_length("me@pc:~$ ls"), Some(9));
    }

    #[test]
    fn reads_the_directory_from_the_prompt() {
        assert_eq!(Dialect::Cmd.prompt_dir("E:\\dev>ping x"), Some("E:\\dev"));
        assert_eq!(Dialect::Cmd.prompt_dir("E:\\>"), Some("E:\\"));
        assert_eq!(
            Dialect::PowerShell.prompt_dir("PS E:\\dev> ls"),
            Some("E:\\dev")
        );
        assert_eq!(
            Dialect::PowerShell.prompt_dir("PS Microsoft.PowerShell.Core\\FileSystem::\\\\pc\\s> "),
            Some("\\\\pc\\s")
        );
        assert_eq!(
            Dialect::PowerShell.prompt_dir("PS HKLM:\\> "),
            Some("HKLM:\\")
        );
        assert_eq!(Dialect::Cmd.prompt_dir("Press any key"), None);
        assert_eq!(Dialect::Wsl.prompt_dir("me@pc:/mnt/e$ "), None);
        assert!(is_same_dir("e:\\Dev", Path::new("E:\\dev")));
        assert!(is_same_dir("E:\\", Path::new("E:\\")));
        assert!(!is_same_dir("E:\\dev", Path::new("E:\\dev2")));
    }

    #[test]
    fn changes_directory_in_a_form_people_do_not_type() {
        assert_eq!(
            Dialect::Cmd.change_dir(Path::new("E:\\my dir")),
            "cd/d \"E:\\my dir\""
        );
        assert_eq!(
            Dialect::PowerShell.change_dir(Path::new("E:\\it's")),
            "Set-Location -LiteralPath 'E:\\it''s'; [Environment]::CurrentDirectory = $PWD.ProviderPath #vc"
        );
        // `[` `]` are wildcards for a plain `cd`, not for -LiteralPath.
        assert_eq!(
            Dialect::PowerShell.change_dir(Path::new("E:\\[x]")),
            "Set-Location -LiteralPath 'E:\\[x]'; [Environment]::CurrentDirectory = $PWD.ProviderPath #vc"
        );
    }

    #[test]
    fn msys_bash_changes_to_the_translated_windows_path() {
        let line = Dialect::Msys.change_dir(Path::new("C:\\it's"));
        assert_eq!(line, "cd \"$(cygpath -u 'C:\\it'\\''s')\"");
        assert!(Dialect::Msys.is_prompt("$ "));
        assert_eq!(Dialect::Msys.prompt_dir("$ "), None);
        assert!(!Dialect::Msys.prompt_shows_dir());
    }

    #[test]
    fn a_dialect_goes_through_the_settings_file_and_back() {
        for dialect in Dialect::ALL {
            assert_eq!(Dialect::from_id(dialect.id()), dialect);
        }
        assert_eq!(Dialect::from_id("fish"), Dialect::Cmd);
    }
}
