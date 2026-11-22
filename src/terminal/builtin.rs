//! Command lines the program carries out itself instead of starting a process, as FAR's
//! `CommandLine::ProcessOSCommands` (ADR-0006): changing the folder or drive, and `set NAME=value`.

/// What a built-in command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Builtin {
    /// `cd <dir>`, `cd /d <dir>`, `chdir <dir>`, `cd..`, `cd\`, a bare drive `D:`.
    ChangeDir(String),
    /// `set NAME=value` / `set "NAME=value"`; an empty value removes the variable.
    SetVariable { name: String, value: String },
    /// `pushd [dir]`: remembers the folder, then goes to `dir`.
    PushDir(Option<String>),
    /// `popd`: back to the folder remembered last.
    PopDir,
    /// `clrd` (FAR's own): forgets the remembered folders.
    ClearDirs,
    /// `title text`: the console's title.
    SetTitle(String),
    /// `exit [code]`: the program ends.
    Exit(i32),
}

/// Recognises a built-in command line; everything else (including `cd` alone, `set` alone, `set /a`,
/// `set /p` and `/?`) goes to the command interpreter.
pub fn parse(line: &str) -> Option<Builtin> {
    let line = line.trim();
    if crate::platform::files::paths::bare_drive_root(line).is_some() {
        return Some(Builtin::ChangeDir(line.to_string()));
    }
    parse_change_dir(line)
        .or_else(|| parse_set(line))
        .or_else(|| parse_far_command(line))
}

/// The rest of FAR's `ProcessOSCommands`. `exit` with something that is no number ends with 0, as FAR.
fn parse_far_command(line: &str) -> Option<Builtin> {
    let (command, arguments) = line.split_once(' ').unwrap_or((line, ""));
    let arguments = arguments.trim();
    if has_switch(arguments, '?') {
        return None;
    }
    match command.to_ascii_uppercase().as_str() {
        "PUSHD" => Some(Builtin::PushDir(
            (!arguments.is_empty()).then(|| arguments.to_string()),
        )),
        "POPD" => Some(Builtin::PopDir),
        "CLRD" => Some(Builtin::ClearDirs),
        "TITLE" => Some(Builtin::SetTitle(arguments.to_string())),
        "EXIT" => Some(Builtin::Exit(arguments.parse().unwrap_or(0))),
        _ => None,
    }
}

/// `chcp <number>`: the code page the runner sets itself (FAR's `ProcessOSCommands`); `chcp` alone and
/// anything else is the interpreter's.
pub fn code_page(line: &str) -> Option<u32> {
    let (command, arguments) = line.trim().split_once(' ')?;
    if !command.eq_ignore_ascii_case("chcp") {
        return None;
    }
    arguments.trim().parse().ok()
}

fn parse_change_dir(line: &str) -> Option<Builtin> {
    let rest = ["chdir", "cd"]
        .iter()
        .find_map(|verb| strip_prefix_ignore_case(line, verb))?;
    // FAR's "cursed spaceless DOS legacy syntax": `cd\`, `cd..`, `cd/d`.
    if !(rest.is_empty() || rest.starts_with([' ', '.', '\\', '/'])) {
        return None;
    }
    let rest = rest.trim_start();
    if has_switch(rest, '?') {
        return None;
    }
    let rest = if crate::platform::process::CD_TAKES_DRIVE_SWITCH && has_switch(rest, 'd') {
        rest[2..].trim()
    } else {
        rest.trim()
    };
    (!rest.is_empty()).then(|| Builtin::ChangeDir(rest.to_string()))
}

/// FAR: `SET [variable=[value]]`; showing variables, `/a` and `/p` are left to the interpreter.
fn parse_set(line: &str) -> Option<Builtin> {
    let (command, arguments) = line.split_once(' ')?;
    if !command.eq_ignore_ascii_case("set") {
        return None;
    }
    let arguments = arguments.trim();
    if ['?', 'a', 'A', 'p', 'P']
        .iter()
        .any(|key| has_switch(arguments, *key))
    {
        return None;
    }
    let (name, value) = unquote(arguments).split_once('=')?;
    (!name.is_empty()).then(|| Builtin::SetVariable {
        name: name.to_string(),
        value: value.to_string(),
    })
}

/// cmd's (and FAR's) `set "NAME=value" ...`: from after the first quote to the last one.
fn unquote(text: &str) -> &str {
    let Some(inner) = text.strip_prefix('"') else {
        return text;
    };
    let inner = inner.trim_start();
    inner.rfind('"').map_or(inner, |end| &inner[..end])
}

/// FAR's `FindKey`: the text starts with `/key` followed by nothing or a space.
fn has_switch(text: &str, key: char) -> bool {
    let mut chars = text.chars();
    chars.next() == Some('/')
        && chars.next().is_some_and(|c| c.eq_ignore_ascii_case(&key))
        && chars.next().is_none_or(|c| c == ' ')
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        text.get(prefix.len()..)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(line: &str) -> Option<String> {
        match parse(line)? {
            Builtin::ChangeDir(target) => Some(target),
            _ => None,
        }
    }

    fn set(line: &str) -> Option<(String, String)> {
        match parse(line)? {
            Builtin::SetVariable { name, value } => Some((name, value)),
            _ => None,
        }
    }

    #[test]
    fn parses_fars_own_commands() {
        assert_eq!(
            parse("pushd \"a b\""),
            Some(Builtin::PushDir(Some("\"a b\"".to_string())))
        );
        assert_eq!(parse("PUSHD"), Some(Builtin::PushDir(None)));
        assert_eq!(parse("popd"), Some(Builtin::PopDir));
        assert_eq!(parse("Clrd"), Some(Builtin::ClearDirs));
        assert_eq!(
            parse("title  my title "),
            Some(Builtin::SetTitle("my title".to_string()))
        );
        assert_eq!(parse("exit"), Some(Builtin::Exit(0)));
        assert_eq!(parse("exit 3"), Some(Builtin::Exit(3)));
        assert_eq!(parse("exit /b 3"), Some(Builtin::Exit(0)));
        for help in ["pushd /?", "popd /?", "title /?", "exit /?"] {
            assert_eq!(parse(help), None, "{help}");
        }
        assert_eq!(parse("exiting"), None);
    }

    #[test]
    fn only_a_number_is_a_code_page_of_our_own() {
        assert_eq!(code_page("chcp 65001"), Some(65001));
        assert_eq!(code_page(" CHCP   866 "), Some(866));
        for other in ["chcp", "chcp x", "chcp 866 & dir", "chcpx 866"] {
            assert_eq!(code_page(other), None, "{other}");
        }
    }

    #[test]
    fn parses_change_directory_commands() {
        assert_eq!(dir("cd src").as_deref(), Some("src"));
        assert_eq!(dir("CD /D D:\\data").as_deref(), Some("D:\\data"));
        assert_eq!(dir("chdir \"My Files\"").as_deref(), Some("\"My Files\""));
        assert_eq!(dir("cd..").as_deref(), Some(".."));
        assert_eq!(dir("cd\\").as_deref(), Some("\\"));
        assert_eq!(dir("cd/d e:\\x").as_deref(), Some("e:\\x"));
        assert_eq!(dir("e:").as_deref(), Some("e:"));
        // `cd /d` alone shows the folder, as `cd` alone: the interpreter's business.
        for bare in ["cd /d", "CD /D  ", "cd/d"] {
            assert_eq!(parse(bare), None, "{bare}");
        }
        assert_eq!(dir("cd /data").as_deref(), Some("/data"));
    }

    #[test]
    fn leaves_other_commands_to_the_interpreter() {
        for line in ["cd", "cd /?", "cdrecord x", "dir", "cargo build", "e: x"] {
            assert_eq!(parse(line), None, "{line}");
        }
    }

    #[test]
    fn parses_set_as_far_does() {
        let pair = |name: &str, value: &str| Some((name.to_string(), value.to_string()));
        assert_eq!(set("set A=1"), pair("A", "1"));
        assert_eq!(set("SET  Path=%PATH%;C:\\x "), pair("Path", "%PATH%;C:\\x"));
        assert_eq!(set("set A="), pair("A", ""));
        assert_eq!(set("set \"A=b c\" junk"), pair("A", "b c"));
        assert_eq!(set("set a=b|c"), pair("a", "b|c"));
    }

    #[test]
    fn leaves_showing_and_switches_of_set_to_the_interpreter() {
        for line in [
            "set",
            "set PATH",
            "set =x",
            "set /a X=1+2",
            "set /P X=?",
            "set /?",
            "setx A=1",
        ] {
            assert_eq!(parse(line), None, "{line}");
        }
    }
}
