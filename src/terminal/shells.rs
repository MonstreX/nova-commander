//! The shells the guest can be (ADR-0006): the system's (`platform::process::shells`, Git Bash where
//! Git for Windows is), and the user's own, each a program with its arguments and the dialect it speaks.

use std::env;
use std::path::{Path, PathBuf};

use super::shell::{Dialect, POWERSHELL_HISTORY_FILTER};
use crate::platform;
use crate::platform::process::ShellKind;

/// What the user's own shells' ids begin with, so that no name of theirs is taken for one of the
/// system's.
const USER_ID: &str = "user:";

/// A shell to start: its program, its arguments, how it speaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    id: String,
    title: String,
    program: PathBuf,
    args: Vec<String>,
    dialect: Dialect,
    /// The user's arguments as typed; empty for the system's shells.
    arguments: String,
}

impl Shell {
    /// The system's `kind`, if it is installed (the default one always is).
    pub fn system(kind: ShellKind) -> Option<Self> {
        let program = match kind {
            ShellKind::GitBash => platform::process::git_bash()?,
            _ if kind == platform::process::DEFAULT_SHELL => PathBuf::from(kind.program()),
            _ => find_in_path(kind.program())?,
        };
        let (dialect, args): (Dialect, &[&str]) = match kind {
            ShellKind::Cmd => (Dialect::Cmd, &[]),
            ShellKind::Powershell | ShellKind::Pwsh => (
                Dialect::PowerShell,
                &["-NoLogo", "-NoExit", "-Command", POWERSHELL_HISTORY_FILTER],
            ),
            ShellKind::Wsl => (Dialect::Wsl, &[]),
            // A login shell reads Git's profile: its prompt, its `PATH`.
            ShellKind::GitBash => (Dialect::Msys, &["--login", "-i"]),
        };
        Some(Self {
            id: kind.id().to_string(),
            title: kind.title().to_string(),
            program,
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            dialect,
            arguments: String::new(),
        })
    }

    /// The system's default shell.
    pub fn default_shell() -> Self {
        Self::system(platform::process::DEFAULT_SHELL).unwrap_or_else(|| Self {
            id: platform::process::DEFAULT_SHELL.id().to_string(),
            title: platform::process::DEFAULT_SHELL.title().to_string(),
            program: PathBuf::from(platform::process::DEFAULT_SHELL.program()),
            args: Vec::new(),
            dialect: Dialect::Cmd,
            arguments: String::new(),
        })
    }

    /// One of the user's: `arguments` as a command line writes them (quotes group words).
    pub fn user(name: &str, program: &str, arguments: &str, dialect: Dialect) -> Self {
        Self {
            id: format!("{USER_ID}{name}"),
            title: name.to_string(),
            program: PathBuf::from(program),
            args: split_arguments(arguments),
            dialect,
            arguments: arguments.to_string(),
        }
    }

    /// Its name in the settings file.
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn arguments(&self) -> &str {
        &self.arguments
    }

    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    pub fn change_dir(&self, dir: &Path) -> String {
        self.dialect.change_dir(dir)
    }

    pub fn prompt_shows_dir(&self) -> bool {
        self.dialect.prompt_shows_dir()
    }

    pub fn prompt_dir<'a>(&self, line: &'a str) -> Option<&'a str> {
        self.dialect.prompt_dir(line)
    }

    pub fn is_prompt(&self, line: &str) -> bool {
        self.dialect.is_prompt(line)
    }

    pub fn prompt_length(&self, line: &str) -> Option<usize> {
        self.dialect.prompt_length(line)
    }

    pub fn clear_input_keys(&self) -> &'static [u8] {
        self.dialect.clear_input_keys()
    }
}

/// The shells to choose from, in menu order: the system's that are installed (its default always),
/// then the user's.
pub fn available_shells(user: &[Shell]) -> Vec<Shell> {
    (system_shells().iter().cloned())
        .chain(user.iter().cloned())
        .collect()
}

/// The system's installed shells, looked for once a run: the lookup walks every `PATH` folder (review
/// R21: a folder on a share that does not answer held each menu), and the program's `PATH` stays as it
/// started.
fn system_shells() -> &'static [Shell] {
    static FOUND: std::sync::OnceLock<Vec<Shell>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        (platform::process::shells().iter())
            .filter_map(|kind| Shell::system(*kind))
            .collect()
    })
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .any(|dir| dir.join(program).is_file())
        .then(|| PathBuf::from(program))
}

/// Words apart at spaces; `"` groups them, `""` inside quotes is one `"`.
pub fn split_arguments(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                word.push('"');
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ' ' | '\t' if !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(ch);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shell_is_always_offered_first() {
        let first = available_shells(&[]).into_iter().next();
        assert_eq!(first, Some(Shell::default_shell()));
    }

    #[test]
    fn the_users_shells_come_after_the_systems_and_keep_their_own_ids() {
        let mine = Shell::user("cmd", r"C:\tools\bash.exe", "--login -i", Dialect::Msys);
        let shells = available_shells(std::slice::from_ref(&mine));
        assert_eq!(shells.last(), Some(&mine));
        assert_eq!(mine.id(), "user:cmd", "not the system's cmd");
        assert_eq!(mine.args(), ["--login", "-i"]);
    }

    #[test]
    fn arguments_split_as_a_command_line_writes_them() {
        assert_eq!(split_arguments(""), Vec::<String>::new());
        assert_eq!(split_arguments("  -a   -b "), ["-a", "-b"]);
        assert_eq!(
            split_arguments(r#"-c "echo hi" "say ""x""" ""#),
            ["-c", "echo hi", r#"say "x""#, ""]
        );
    }

    #[test]
    fn git_bash_is_a_login_bash_where_git_is() {
        if let Some(bash) = Shell::system(ShellKind::GitBash) {
            assert_eq!(bash.dialect(), Dialect::Msys);
            assert_eq!(bash.args(), ["--login", "-i"]);
            assert!(bash.program().is_file());
        }
    }
}
