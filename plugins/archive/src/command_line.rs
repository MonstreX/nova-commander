//! `arc:`'s commands other than opening, read as arclite's cmdline.cpp reads them: `c` and `u` (a new
//! archive, the files added to one), `x` (archives extracted), `e` (items of an archive extracted), `d`
//! (items deleted), `t` (archives tested), each with its parameters (`-name` or `/name`, `:value`), the
//! names after them; a command it cannot read is its `E_BAD_FORMAT` (the usage shown, arclite's help
//! topic "Prefix").

use crate::extract_options::{ExtractOptions, Overwrite, Tri};
use crate::opening::{OpenCommand, is_param, tokens, unquote};

/// The commands' syntax as arclite's cmdline.cpp gives it, shown for a command not understood (its
/// lines cut to fit a window of 80 columns).
pub const USAGE: [&str; 14] = [
    "arc:[-d] [-t:<arc_type>] [-p:<password>] <archive>",
    "arc:c [-pr:<profile>] [-t:<arc_type>] [-l:<level>] [-m:<method>]",
    "  [-s[:(y|n)]] [-p:<password>] [-eh[:(y|n)]] [-mf[:(y|n)]] [-ie[:(y|n)]]",
    "  [-adv:<advanced>] <archive> (<file> ... | @<filelist>)",
    "arc:u [-l:<level>] [-m:<method>] [-s[:(y|n)]] [-p:<password>]",
    "  [-eh[:(y|n)]] [-mf[:(y|n)]] [-ie[:(y|n)]] [-o[:(o|s)]]",
    "  [-adv:<advanced>] <archive> (<file> ... | @<filelist>)",
    "  <level> = 0|1|3|5|7|9, <method> = lzma|lzma2|ppmd|deflate|deflate64",
    "arc:x [-ie[:(y|n)]] [-o[:(o|s|r|a)]] [-mf[:(y|n)]] [-p:<password>]",
    "  [-sd[:(a|y|n)]] [-da[:(y|n)]] <archive1> <archive2> ... <path>",
    "arc:e [-ie[:(y|n)]] [-o[:(o|s|r|a)]] [-mf[:(y|n)]] [-p:<password>]",
    "  [-out:<outdir>] <archive> <extract_item> ...",
    "arc:d [-ie[:(y|n)]] [-p:<password>] <archive> <delete_item> ...",
    "arc:t <archive1> <archive2> ...",
];

/// arclite's `E_BAD_FORMAT`: the command is not understood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadFormat;

/// A command read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open(OpenCommand),
    Update(UpdateCommand),
    /// `x`: archives extracted into a folder.
    Extract(ExtractCommand),
    /// `e` (`delete` false) or `d`: items of one archive extracted or deleted.
    Items(ItemsCommand),
    /// `t`: archives tested.
    Test(Vec<String>),
}

/// `c` / `u` (arclite's `UpdateCommand`): what is not given is `None` (the profile's, the archive's or
/// arclite's default then).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateCommand {
    /// A new archive: `c`, or `u` of a file not there.
    pub create: bool,
    /// `-pr:` (only for `c`).
    pub profile: Option<String>,
    /// `-t:`: the format's name (only for `c`).
    pub format: Option<String>,
    pub level: Option<u32>,
    pub method: Option<String>,
    pub solid: Option<bool>,
    /// `-p:`: encrypted with it.
    pub password: Option<String>,
    /// `-eh`: the header's encryption (7z's; a zip has none to encrypt).
    pub encrypt_header: Option<Tri>,
    /// `-sfx[:module]` (only for `c`).
    pub sfx: Option<String>,
    /// `-v:size` (only for `c`).
    pub volumes: Option<String>,
    pub move_files: Option<bool>,
    pub ignore_errors: Option<bool>,
    /// `-o` (only for `u`): Overwrite or Skip; none: Ask.
    pub overwrite: Option<Overwrite>,
    pub advanced: Option<String>,
    pub archive: String,
    pub files: Vec<String>,
    /// `@<file>`: files listing the files, one a line.
    pub listfiles: Vec<String>,
}

/// `x` (arclite's `ExtractCommand`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractCommand {
    pub options: ExtractOptions,
    pub archives: Vec<String>,
}

/// `e` / `d` (arclite's `ExtractItemsCommand`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemsCommand {
    pub delete: bool,
    pub options: ExtractOptions,
    pub archive: String,
    /// Each an item's path in the archive (`\` between its parts) or masks.
    pub items: Vec<String>,
}

/// One parameter: its name in lower case, its value without quotes.
fn param(token: &str) -> (String, String) {
    match token[1..].split_once(':') {
        Some((name, value)) => (name.to_lowercase(), unquote(value)),
        None => (token[1..].to_lowercase(), String::new()),
    }
}

/// arclite's `parse_bool_value`: none or `y` yes, `n` no.
fn yes_no(value: &str) -> Result<bool, BadFormat> {
    match value.to_lowercase().as_str() {
        "" | "y" => Ok(true),
        "n" => Ok(false),
        _ => Err(BadFormat),
    }
}

/// arclite's `parse_tri_state_value`: none or `y` yes, `n` no, `a` neither.
fn tri(value: &str) -> Result<Tri, BadFormat> {
    match value.to_lowercase().as_str() {
        "" | "y" => Ok(Tri::True),
        "n" => Ok(Tri::False),
        "a" => Ok(Tri::Undef),
        _ => Err(BadFormat),
    }
}

/// arclite's `parse_listfile`: one name a line, empty lines passed over.
pub fn list_file(text: &str) -> Vec<String> {
    (text.split(['\r', '\n']))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// `arc:`'s text: its command, or the archive to open; `BadFormat` for one not understood. `exists`
/// tells whether an archive named so (as typed) is there: `u` of one that is not makes it (arclite).
pub fn parse(text: &str, exists: &dyn Fn(&str) -> bool) -> Result<Command, BadFormat> {
    let mut args = tokens(text);
    let first = args.first().map(|token| token.to_lowercase());
    let kind = first
        .as_deref()
        .filter(|first| matches!(*first, "c" | "u" | "x" | "e" | "d" | "t"));
    let Some(kind) = kind.map(str::to_string) else {
        return crate::opening::parse(text)
            .map(Command::Open)
            .map_err(|_| BadFormat);
    };
    args.remove(0);
    match kind.as_str() {
        "c" | "u" => {
            let named = args.iter().find(|arg| !is_param(arg));
            let create = kind == "c" || named.is_some_and(|archive| !exists(&unquote(archive)));
            update(create, &args).map(Command::Update)
        }
        "x" => extract(&args).map(Command::Extract),
        "e" | "d" => items(kind == "d", &args).map(Command::Items),
        _ => test(&args).map(Command::Test),
    }
}

/// arclite's `c_levels`.
const LEVELS: [u32; 6] = [0, 1, 3, 5, 7, 9];

/// arclite's `c_methods` and the 7-Zip codecs a zip may name (its `ArcAPI::codecs`).
const METHODS: [&str; 7] = [
    "lzma",
    "lzma2",
    "ppmd",
    "deflate",
    "deflate64",
    "bzip2",
    "copy",
];

/// arclite's `parse_update_command` (but the profile, read where the settings are).
fn update(create: bool, args: &[String]) -> Result<UpdateCommand, BadFormat> {
    let mut command = UpdateCommand {
        create,
        ..UpdateCommand::default()
    };
    let at = args
        .iter()
        .position(|arg| !is_param(arg))
        .ok_or(BadFormat)?;
    for arg in &args[..at] {
        update_param(&mut command, param(arg))?;
    }
    let (archive, rest) = args[at..].split_first().ok_or(BadFormat)?;
    if rest.is_empty() {
        return Err(BadFormat);
    }
    command.archive = unquote(archive);
    for arg in rest {
        if is_param(arg) {
            return Err(BadFormat);
        }
        match arg.strip_prefix('@') {
            Some(list) => command.listfiles.push(unquote(list)),
            None => command.files.push(unquote(arg)),
        }
    }
    Ok(command)
}

/// arclite's `str_to_int` (`_wtoi`): the digits a text starts with, 0 for none.
fn wtoi(text: &str) -> u32 {
    let digits: String = text
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0)
}

/// One parameter of `c` / `u`.
fn update_param(
    command: &mut UpdateCommand,
    (name, value): (String, String),
) -> Result<(), BadFormat> {
    // arclite's `CHECK_FMT`.
    let check = |holds: bool| if holds { Ok(()) } else { Err(BadFormat) };
    match name.as_str() {
        "pr" => {
            check(command.create)?;
            command.profile = Some(value);
        }
        "t" => {
            // arclite's `find_by_name` finds no format of no name.
            check(command.create && !value.is_empty())?;
            command.format = Some(value);
        }
        "l" => {
            let level = wtoi(&value);
            check(LEVELS.contains(&level))?;
            command.level = Some(level);
        }
        "m" => {
            check(METHODS.contains(&value.to_lowercase().as_str()))?;
            command.method = Some(value);
        }
        "s" => command.solid = Some(yes_no(&value)?),
        "p" if !value.is_empty() => command.password = Some(value),
        "eh" => command.encrypt_header = Some(tri(&value)?),
        "sfx" => {
            check(command.create)?;
            command.sfx = Some(value);
        }
        "v" if !value.is_empty() => {
            check(command.create)?;
            command.volumes = Some(value);
        }
        "mf" => command.move_files = Some(yes_no(&value)?),
        "ie" => command.ignore_errors = Some(yes_no(&value)?),
        "o" => {
            check(!command.create)?;
            command.overwrite = Some(match value.to_lowercase().as_str() {
                "" | "o" => Overwrite::Overwrite,
                "s" => Overwrite::Skip,
                _ => return Err(BadFormat),
            });
        }
        "adv" if !value.is_empty() => command.advanced = Some(value),
        _ => return Err(BadFormat),
    }
    Ok(())
}

/// arclite's `ExtractOptions()` before the command's parameters.
fn extract_defaults() -> ExtractOptions {
    ExtractOptions {
        dst_dir: std::path::PathBuf::new(),
        ignore_errors: false,
        overwrite: Overwrite::Overwrite,
        move_files: Tri::Undef,
        password: String::new(),
        separate_dir: Tri::False,
        delete_archive: false,
        disable_delete_archive: false,
        open_dir: Tri::False,
    }
}

/// Which of `x`, `e`, `d` the parameters are read for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Of {
    Extract,
    Items,
    Delete,
}

/// arclite's `parse_extract_params`: the parameters (until `--`) into `options`, the names after.
fn extract_params(of: Of, args: &[String]) -> Result<(ExtractOptions, Vec<String>), BadFormat> {
    let mut options = extract_defaults();
    let mut names = Vec::new();
    let mut params = true;
    for arg in args {
        if params && arg == "--" {
            params = false;
        } else if params && is_param(arg) {
            extract_param(of, &mut options, param(arg))?;
        } else {
            names.push(unquote(arg));
        }
    }
    Ok((options, names))
}

/// One parameter of `x`, `e` or `d`.
fn extract_param(
    of: Of,
    options: &mut ExtractOptions,
    (name, value): (String, String),
) -> Result<(), BadFormat> {
    match (name.as_str(), of) {
        ("ie", _) => options.ignore_errors = yes_no(&value)?,
        ("o", Of::Extract | Of::Items) => {
            options.overwrite = match value.to_lowercase().as_str() {
                "" | "o" => Overwrite::Overwrite,
                "s" => Overwrite::Skip,
                "r" => Overwrite::Rename,
                "a" => Overwrite::Append,
                _ => return Err(BadFormat),
            };
        }
        ("mf", Of::Extract | Of::Items) => options.move_files = tri(&value)?,
        ("p", _) if !value.is_empty() => options.password = value,
        ("sd", Of::Extract) => options.separate_dir = tri(&value)?,
        ("da", Of::Extract) => options.delete_archive = yes_no(&value)?,
        ("out", Of::Items) if !value.is_empty() => options.dst_dir = value.into(),
        _ => return Err(BadFormat),
    }
    Ok(())
}

/// arclite's `parse_extract_command`: the archives, the folder last.
fn extract(args: &[String]) -> Result<ExtractCommand, BadFormat> {
    let (mut options, mut archives) = extract_params(Of::Extract, args)?;
    if archives.len() < 2 {
        return Err(BadFormat);
    }
    options.dst_dir = archives.pop().unwrap_or_default().into();
    Ok(ExtractCommand { options, archives })
}

/// arclite's `parse_extractitems_command`: the archive, then its items.
fn items(delete: bool, args: &[String]) -> Result<ItemsCommand, BadFormat> {
    let of = if delete { Of::Delete } else { Of::Items };
    let (options, mut names) = extract_params(of, args)?;
    if names.len() < 2 {
        return Err(BadFormat);
    }
    let archive = names.remove(0);
    Ok(ItemsCommand {
        delete,
        options,
        archive,
        items: names,
    })
}

/// arclite's `parse_test_command`: the archives, the last no parameter.
fn test(args: &[String]) -> Result<Vec<String>, BadFormat> {
    // arclite checks the last only (`CHECK_FMT(!is_param(args.back()))`): a parameter before it is
    // taken as an archive's name.
    if args.last().is_some_and(|last| is_param(last)) {
        return Err(BadFormat);
    }
    Ok(args.iter().map(|arg| unquote(arg)).collect())
}

#[cfg(test)]
#[path = "command_line_tests.rs"]
mod tests;
