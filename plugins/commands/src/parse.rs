//! A prefix's command read as FAR's farcmds reads it (`OpenFromCommandLine`, `GetEditParam`,
//! `GetStreamAndMode`, `GetInt`): `edit:[line,column] object`, `run:file<command`, an object after `<`
//! with its stream and mode, else the object as it is. A command that is wrong in its form is none: the
//! plugin shows its syntax (FAR shows the command's help).

use crate::text;

/// The prefixes (FAR's `CommandPrefix`).
pub const PREFIXES: [&str; 9] = [
    "view", "edit", "goto", "clip", "whereis", "link", "run", "load", "unload",
];

/// Which streams a command's output takes (FAR's `CatchModeType`, the configuration's order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Catch {
    /// `*`: both as one.
    #[default]
    Both,
    /// `1`: the standard output only.
    Output,
    /// `2`: the standard error only.
    Errors,
    /// `?`: both, each its own.
    Apart,
}

impl Catch {
    pub fn of(index: i64) -> Self {
        match index {
            1 => Self::Output,
            2 => Self::Errors,
            3 => Self::Apart,
            _ => Self::Both,
        }
    }

    pub fn index(self) -> i64 {
        match self {
            Self::Both => 0,
            Self::Output => 1,
            Self::Errors => 2,
            Self::Apart => 3,
        }
    }
}

/// What the console's own output does while a command runs (FAR's `enShowCmdOutput`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// `>`: hidden, a window tells the output's size.
    #[default]
    Hide,
    /// `<`: kept for Ctrl+O.
    Keep,
    /// `+`: kept, the output taken shown with it.
    Both,
}

impl Screen {
    pub fn of(index: i64) -> Self {
        match index {
            1 => Self::Keep,
            2 => Self::Both,
            _ => Self::Hide,
        }
    }

    pub fn index(self) -> i64 {
        match self {
            Self::Hide => 0,
            Self::Keep => 1,
            Self::Both => 2,
        }
    }
}

/// A command as read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    /// The prefix, small.
    pub prefix: String,
    /// The object: a name, a path, `link:`'s arguments, or the command after `<`.
    pub object: String,
    /// After `<`: a command's output (`view:`, `edit:`, `clip:`, `run:`) or a file holding the object.
    pub from: bool,
    pub catch: Catch,
    pub screen: Screen,
    /// `edit:`'s line and column (none: as the editor opens it).
    pub line: Option<i64>,
    pub column: Option<i64>,
    /// `run:`'s file for the output.
    pub run_file: Option<String>,
}

/// A number between `[` and `]` (FAR's `GetInt`): variables expanded, its leading digits; empty: 0.
fn number(text: &str) -> i64 {
    let text = if text.contains('%') {
        text::expand(text)
    } else {
        text.to_string()
    };
    let text = text.trim_start();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    sign * digits[..end].parse::<i64>().unwrap_or(0)
}

/// `edit:`'s `[line,column]` (FAR's `GetEditParam`): the rest and the numbers; none when the brackets are
/// wrong (a comma after `]` too, as FAR's).
fn edit_place<'a>(text: &'a str, separator: &str) -> Option<(&'a str, Option<i64>, Option<i64>)> {
    let Some(inner) = text.strip_prefix('[') else {
        return Some((text.trim_start(), None, None));
    };
    let close = inner.find(']');
    let separated = (!separator.is_empty())
        .then(|| inner.find(separator))
        .flatten();
    let Some(close) = close.filter(|close| separated.is_none_or(|at| *close < at)) else {
        return Some((text.trim_start(), None, None));
    };
    let rest = inner[close + 1..].trim_start();
    match inner.find(',') {
        Some(comma) if comma < close => {
            let (line, column) = (number(&inner[..comma]), number(&inner[comma + 1..close]));
            (line > -2 && column > -2).then_some((rest, Some(line), Some(column)))
        }
        Some(_) => None,
        None => {
            let line = number(&inner[..close]);
            (line > -2).then_some((rest, Some(line), None))
        }
    }
}

/// `<`'s stream and mode before the object (FAR's `GetStreamAndMode`).
fn stream_and_mode(text: &str, command: &mut Command) -> String {
    let mut rest = text;
    if let Some(first) = rest.chars().next() {
        let catch = match first {
            '*' => Some(Catch::Both),
            '1' => Some(Catch::Output),
            '2' => Some(Catch::Errors),
            '?' => Some(Catch::Apart),
            _ => None,
        };
        if let Some(catch) = catch {
            command.catch = catch;
            rest = &rest[1..];
        }
    }
    if let Some(first) = rest.chars().next() {
        let screen = match first {
            '>' => Some(Screen::Hide),
            '<' => Some(Screen::Keep),
            '+' => Some(Screen::Both),
            _ => None,
        };
        if let Some(screen) = screen {
            command.screen = screen;
            rest = &rest[1..];
        }
    }
    rest.trim_start().to_string()
}

/// `prefix:text` read, the configuration's stream and mode the defaults; none: shown its syntax.
pub fn parse(
    prefix: &str,
    text: &str,
    separator: &str,
    defaults: (Catch, Screen),
) -> Option<Command> {
    let prefix = prefix.to_ascii_lowercase();
    let text = text.trim_end();
    if prefix.len() + 1 + text.len() <= 3 {
        return None;
    }
    let mut command = Command {
        prefix,
        catch: defaults.0,
        screen: defaults.1,
        ..Command::default()
    };
    let mut rest = text;
    if command.prefix == "edit" {
        let (after, line, column) = edit_place(rest, separator)?;
        (command.line, command.column, rest) = (line, column, after);
    } else if command.prefix == "run" {
        // `run:file<command`; without `<` nothing is done (FAR's run as it is).
        let at = rest.find('<')?;
        command.run_file = Some(rest[..at].trim().to_string());
        rest = &rest[at..];
    }
    let rest = rest.trim_start();
    command.object = match rest.strip_prefix('<') {
        Some(after) => {
            command.from = true;
            stream_and_mode(after, &mut command)
        }
        None => rest.to_string(),
    };
    (!command.object.is_empty()).then_some(command)
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
