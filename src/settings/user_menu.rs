//! The user menu's files (F2), in the text form of FAR's menu files (`usermenu.cpp`): a line `hotkey: label` starts an
//! item, the lines under it that begin with a space are its commands, `{` ... `}` after it are its submenu;
//! the hotkey `--` makes a separator. The main menu is `settings\UserMenu.ini` beside the settings file, a
//! local one `UserMenu.ini` in a folder (the owner: the program's own name, no FAR's).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::vfs::local::ops;

/// The name of a menu's file.
pub const MENU_FILE: &str = "UserMenu.ini";
/// What a command line is indented by when saved (FAR's `"F24: "` padding).
const PADDING: usize = 5;

/// One item of a menu.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Item {
    /// A letter, a digit, `F1`..`F24`, `--` for a separator, or nothing.
    pub hotkey: String,
    pub label: String,
    pub commands: Vec<String>,
    /// The items of a submenu; `None` for a command.
    pub submenu: Option<Vec<Item>>,
}

impl Item {
    pub fn is_separator(&self) -> bool {
        self.hotkey == "--"
    }
}

/// The main menu's file beside the settings file.
pub fn main_menu_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name(MENU_FILE)
}

/// FAR's `ParseMenu`.
pub fn parse(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut at = 0;
    parse_level(&lines, &mut at)
}

fn parse_level(lines: &[&str], at: &mut usize) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    while *at < lines.len() {
        let line = lines[*at].trim_end();
        *at += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with('{') {
            if let Some(item) = items.last_mut().filter(|item| item.submenu.is_none()) {
                item.submenu = Some(parse_level(lines, at));
                item.commands.clear();
            }
            continue;
        }
        // `}` can be a hotkey as well.
        if line.starts_with('}') && !line[1..].starts_with(':') {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if let Some(item) = items.last_mut() {
                item.commands.push(line.trim_start().to_string());
            }
            continue;
        }
        let Some(mut colon) = line.find(':') else {
            continue;
        };
        // The hotkey `:` itself.
        if line[colon + 1..].starts_with(':') {
            colon += 1;
        }
        items.push(Item {
            hotkey: line[..colon].to_string(),
            label: line[colon + 1..].trim_start().to_string(),
            ..Item::default()
        });
    }
    items
}

/// FAR's `SerializeMenu`, with the system's line breaks.
pub fn serialize(items: &[Item]) -> String {
    let eol = crate::platform::LINE_BREAK;
    let mut text = String::new();
    for item in items {
        let hotkey = format!("{}:", item.hotkey);
        text.push_str(&format!("{hotkey:<PADDING$}{}{eol}", item.label));
        match &item.submenu {
            Some(submenu) => {
                text.push_str(&format!("{{{eol}"));
                text.push_str(&serialize(submenu));
                text.push_str(&format!("}}{eol}"));
            }
            None => {
                for command in &item.commands {
                    text.push_str(&format!("{:PADDING$}{command}{eol}", ""));
                }
            }
        }
    }
    text
}

/// Text in the console's OEM code page; the caller has it from the system.
pub type Oem = fn(&[u8]) -> String;

/// The text of a menu's file: UTF-8 (with or without its mark), UTF-16 with its mark, else the console's
/// OEM code page, as FAR's `GetFileCodepage` guesses.
pub fn decode(bytes: &[u8], oem: Oem) -> String {
    if let Some(text) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        return String::from_utf8_lossy(text).into_owned();
    }
    if let Some(units) = bytes.strip_prefix(b"\xff\xfe") {
        let units: Vec<u16> = units
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => oem(bytes),
    }
}

/// The menu in `path`; `None` when there is no such file.
pub fn load(path: &Path, oem: Oem) -> io::Result<Option<Vec<Item>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(parse(&decode(&bytes, oem)))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Writes `items` to `path` in UTF-8; an empty menu removes its file (FAR's `SaveMenu`).
pub fn save(path: &Path, items: &[Item]) -> io::Result<()> {
    if items.is_empty() {
        return match fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    ops::write_atomic(path, serialize(items).as_bytes())
}

#[cfg(test)]
#[path = "user_menu_tests.rs"]
mod tests;
