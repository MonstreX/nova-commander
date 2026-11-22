//! NetBox's bookmarks and "Open directory" (`TWinSCPFileSystem::OpenDirectory`, `OpenDirectoryDialog`):
//! each session's bookmarked folders (by its session key, NetBox's `TBookmarkList` in its configuration)
//! kept in the plugin's data folder; the menu lists the session's last folders (its path history, the 40
//! last) and, after a separator, the bookmarks sorted, the cursor on the current folder's bookmark.

use std::path::Path;

/// The bookmarks' file in the plugin's data folder: `key<TAB>remote[<TAB>local<TAB>name]` lines (the
/// shared ones, Location profiles', under the empty key).
pub const FILE: &str = "Bookmarks";

/// A bookmark (WinSCP's `TBookmark`): its name, its remote folder and its local one (either may be empty).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mark {
    pub name: String,
    pub remote: String,
    pub local: String,
}

impl Mark {
    /// Open directory's bookmark of the remote `folder`, named after it.
    pub fn remote(folder: &str) -> Self {
        Self {
            name: folder.to_string(),
            remote: folder.to_string(),
            local: String::new(),
        }
    }
}

/// The history's lines shown (NetBox's `MaxHistory`).
pub const MAX_HISTORY: usize = 40;

/// NetBox's `NB_OPEN_DIRECTORY_BROWSE_CAPTION`, `NB_OPEN_DIRECTORY_ADD_BOOKMARK_ACTION`,
/// `NB_OPEN_DIRECTORY_HELP`.
pub const OPEN_TITLE: &str = "Open directory";
pub const ADD_TITLE: &str = "Manage bookmarks";
pub const HELP: &str = "Del - remove, Ctrl-Enter - command line, Ctrl-Ins - clipboard";

/// A field as kept in the file: its tabs, line ends and `%` as `%09`, `%0A`, `%0D`, `%25` (a name or
/// a folder may hold them; a raw one would move the fields or split the line).
fn escaped(field: &str) -> String {
    let mut kept = String::with_capacity(field.len());
    for char in field.chars() {
        match char {
            '%' => kept.push_str("%25"),
            '\t' => kept.push_str("%09"),
            '\n' => kept.push_str("%0A"),
            '\r' => kept.push_str("%0D"),
            other => kept.push(other),
        }
    }
    kept
}

/// A field read back (a `%` before anything else stays as it is).
fn unescaped(field: &str) -> String {
    let mut read = String::with_capacity(field.len());
    let mut rest = field;
    while let Some(at) = rest.find('%') {
        read.push_str(&rest[..at]);
        let code = rest.get(at + 1..at + 3);
        let char = match code {
            Some("25") => Some('%'),
            Some("09") => Some('\t'),
            Some("0A") => Some('\n'),
            Some("0D") => Some('\r'),
            _ => None,
        };
        match char {
            Some(char) => {
                read.push(char);
                rest = &rest[at + 3..];
            }
            None => {
                read.push('%');
                rest = &rest[at + 1..];
            }
        }
    }
    read.push_str(rest);
    read
}

/// The bookmarks of `key` (a session's, or the shared ones under "") in `file`, as kept.
pub fn load_marks(file: &Path, key: &str) -> Vec<Mark> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    (text.lines())
        .filter_map(|line| {
            let mut fields = line.split('\t').map(unescaped);
            let at = fields.next()?;
            let remote = fields.next()?;
            let local = fields.next().unwrap_or_default();
            let name = fields.next().filter(|name| !name.is_empty());
            let name = name.unwrap_or_else(|| remote.clone());
            (at == key).then_some(Mark {
                name,
                remote,
                local,
            })
        })
        .collect()
}

/// The remote folders bookmarked for `key` (Open directory's).
pub fn load(file: &Path, key: &str) -> Vec<String> {
    (load_marks(file, key).into_iter())
        .map(|mark| mark.remote)
        .filter(|remote| !remote.is_empty())
        .collect()
}

/// The bookmarks of `key` written over its old ones (the other keys' kept).
pub fn save_marks(file: &Path, key: &str, marks: &[Mark]) -> std::io::Result<()> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let at = escaped(key);
    let mut lines: Vec<String> = (text.lines())
        .filter(|line| line.split('\t').next() != Some(at.as_str()))
        .map(str::to_string)
        .collect();
    lines.extend((marks.iter()).map(|mark| {
        let fields = [&mark.remote, &mark.local, &mark.name].map(|field| escaped(field));
        format!("{at}\t{}", fields.join("\t"))
    }));
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    std::fs::write(file, text)
}

/// A line of the menu: the folder it shows and opens (empty: NetBox's empty line, which opens nothing;
/// none: the separator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub folder: Option<String>,
    /// A bookmark (Del removes it), else the history's.
    pub bookmark: bool,
}

/// The menu's lines (NetBox's): the history's last folders, then the bookmarks sorted without twins after a
/// separator - or, without bookmarks, an empty line; and the line the cursor starts on (the current
/// folder's bookmark, else the first bookmark, else the empty line).
pub fn lines(history: &[String], bookmarks: &[String], current: &str) -> (Vec<Line>, usize) {
    // NetBox's: past 40, the last 39.
    let first = match history.len() > MAX_HISTORY {
        true => history.len() - MAX_HISTORY + 1,
        false => 0,
    };
    let mut lines: Vec<Line> = (history[first..].iter())
        .map(|folder| Line {
            folder: Some(folder.clone()),
            bookmark: false,
        })
        .collect();
    let mut sorted: Vec<&String> = (bookmarks.iter())
        .filter(|folder| !folder.is_empty())
        .collect();
    sorted.sort();
    sorted.dedup();
    if sorted.is_empty() {
        lines.push(Line {
            folder: Some(String::new()),
            bookmark: false,
        });
        let empty = lines.len() - 1;
        return (lines, empty);
    }
    if !lines.is_empty() {
        lines.push(Line {
            folder: None,
            bookmark: false,
        });
    }
    let offset = lines.len();
    let focused = (sorted.iter()).position(|folder| *folder == current);
    lines.extend(sorted.into_iter().map(|folder| Line {
        folder: Some(folder.clone()),
        bookmark: true,
    }));
    (lines, offset + focused.unwrap_or(0))
}

#[cfg(test)]
#[path = "bookmarks_tests.rs"]
mod tests;
