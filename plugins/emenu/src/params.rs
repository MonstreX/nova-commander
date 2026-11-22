//! The files a menu is for (FAR's emenu `ParseParams`, `GetFilesFromParams`, `GetFilesFromPanel`,
//! `MenuForPanelOrCmdLine`): a command line's names split at spaces out of quotes (a quote ends a name too),
//! `%variables%` expanded; a name without a folder in the panel's folder; all of one folder, else FAR's
//! "different folders"; the menu's title: the one name (a folder's with `\`) or "2 file(s), 1 folder(s)".

use std::path::{Path, PathBuf};

/// `text` with its `%name%` variables expanded (Windows' `ExpandEnvironmentStrings`: an unknown one as it
/// is).
pub fn expand(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            break;
        };
        out.push_str(&rest[..start]);
        let name = &after[..end];
        match (!name.is_empty())
            .then(|| std::env::var(name).ok())
            .flatten()
        {
            Some(value) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The names in `text` (FAR's `ParseParams`): split at spaces and tabs out of quotes, a quote ending the
/// name before it and starting one.
pub fn names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut name = String::new();
    let mut quoted = false;
    for letter in text.chars() {
        let space = matches!(letter, ' ' | '\t') && !quoted;
        if space || letter == '"' {
            quoted ^= letter == '"';
            if !name.is_empty() {
                names.push(std::mem::take(&mut name));
            }
            continue;
        }
        name.push(letter);
    }
    if !name.is_empty() {
        names.push(name);
    }
    names
}

/// One of the menu's files or folders: whole, whether it is a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub folder: bool,
}

/// `name` whole: as it is with a folder or a drive, else in `current` (the menu's: Windows).
#[cfg(any(windows, test))]
pub fn whole(name: &str, current: &Path) -> PathBuf {
    let path = Path::new(name);
    match path.is_absolute() || path.has_root() || name.get(1..2) == Some(":") {
        true => path.to_path_buf(),
        false => current.join(name),
    }
}

/// A drive alone ("C:", "C:\"): its root, as FAR gives it ("C:\").
pub fn drive_root(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let drive = bytes.first().is_some_and(u8::is_ascii_alphabetic) && bytes.get(1) == Some(&b':');
    (drive && matches!(&name[2..], "" | "\\" | "/")).then(|| format!("{}\\", &name[..2]))
}

/// The folder `target` is in, to compare (a drive's root: none, as This PC's items have no folder).
fn folder_of(target: &Target) -> String {
    let text = target.path.to_string_lossy();
    if drive_root(&text).is_some() {
        return String::new();
    }
    let parent = target
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    parent
        .to_string_lossy()
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Whether all of `targets` are in one folder (FAR: else "files come from different folders").
pub fn one_folder(targets: &[Target]) -> bool {
    let mut folders = targets.iter().map(folder_of);
    let first = folders.next().unwrap_or_default();
    folders.all(|folder| folder == first)
}

/// The shown name of `target` (FAR's `PointToName`; a drive's root whole).
pub fn shown_name(target: &Target) -> String {
    let text = target.path.to_string_lossy().into_owned();
    if let Some(root) = drive_root(&text) {
        return root;
    }
    let name = target
        .path
        .file_name()
        .map_or(text.clone(), |name| name.to_string_lossy().into_owned());
    match target.folder {
        true => format!("{}\\", name.trim_end_matches('\\')),
        false => name,
    }
}

/// The menu's title (FAR's `DoMenu`): the one item's name, else the counts of files and folders.
pub fn title(targets: &[Target]) -> String {
    if let [target] = targets {
        return shown_name(target);
    }
    let folders = targets.iter().filter(|target| target.folder).count();
    let files = targets.len() - folders;
    let mut parts = Vec::new();
    if files > 0 {
        parts.push(format!("{files} file(s)"));
    }
    if folders > 0 {
        parts.push(format!("{folders} folder(s)"));
    }
    parts.join(", ")
}

/// Two texts the same but for `&` and case (FAR's `CompareExcluding` with `&`; the menu's: Windows).
#[cfg(any(windows, test))]
pub fn same_but_hotkeys(one: &str, other: &str) -> bool {
    let plain = |text: &str| text.replace('&', "").to_lowercase();
    plain(one) == plain(other)
}

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
