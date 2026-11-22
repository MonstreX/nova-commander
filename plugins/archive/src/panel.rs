//! An archive's panel as arclite shows it (`Plugin::list`, `Plugin::info`, `Archive::load_arc_attr`): a
//! folder's items with their sizes, packed sizes, times and attributes (a folder only implied, or an
//! item without a time, has the archive file's), the title `Archive:<format>:<name>[:<folder>]`, the info
//! panel's lines.

use nc_plugin_sdk::{FileEntry, InfoLine};

use crate::item::DIRECTORY;
use crate::opened::Opened;
use crate::options::TITLE;

/// `FILE_ATTRIBUTE_READONLY`, `_HIDDEN`, `_SYSTEM`.
const READ_ONLY: u32 = 0x1;
const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;

/// The entries of the folder `path`; none when there is no such folder.
pub fn entries(opened: &Opened, path: &str) -> Option<Vec<FileEntry>> {
    let folder = opened.tree.find_dir(path)?;
    let children = &opened.tree.nodes[folder].children;
    Some(children.iter().map(|&node| entry(opened, node)).collect())
}

/// The node `node` as the panel's entry.
pub fn entry(opened: &Opened, node: usize) -> FileEntry {
    let node = &opened.tree.nodes[node];
    let item = node.item.map(|index| &opened.items[index]);
    let file = &opened.file;
    let attributes =
        item.map_or(DIRECTORY, |item| item.attributes) | if node.is_dir { DIRECTORY } else { 0 };
    let (size, packed) = item.map_or((0, 0), |item| (item.size, item.packed));
    FileEntry {
        name: node.name.clone(),
        is_dir: node.is_dir,
        size,
        allocated: Some(packed),
        modified: item.and_then(|item| item.modified).or(file.modified),
        created: item.and_then(|item| item.created).or(file.created),
        accessed: item.and_then(|item| item.accessed).or(file.accessed),
        hidden: attributes & HIDDEN != 0,
        system: attributes & SYSTEM != 0,
        read_only: attributes & READ_ONLY != 0,
        attributes,
        ..FileEntry::default()
    }
}

/// arclite's `PanelTitle`: its name, the format, the archive's name, the folder in it as FAR writes it.
pub fn title(opened: &Opened, path: &str) -> String {
    let mut title = format!("{TITLE}:{}:{}", opened.format, opened.name());
    // arclite's partition: its disk's path and `:N` (`set_partition`'s `arc_path`).
    if let Some(partition) = opened.partition {
        title.push_str(&format!(":{partition}"));
    }
    let folder = path.trim_matches('/');
    if !folder.is_empty() {
        title.push_str(":\\");
        title.push_str(&folder.replace('/', "\\"));
    }
    title
}

/// arclite's `load_arc_attr`: the lines read as the archive opened ([`crate::props::info_lines`]).
pub fn info_lines(opened: &Opened) -> Vec<InfoLine> {
    opened
        .library()
        .map_or_else(Vec::new, |(_, lines)| lines.to_vec())
}

/// arclite's `format_size_prop`: the short size (`format_data_size`, by 1024 past 1000, three digits) and,
/// when it differs, ` = ` the bytes.
pub fn size_text(value: u64) -> String {
    let short = short_size(value);
    let long = value.to_string();
    if short == long {
        short
    } else {
        format!("{short} = {long}")
    }
}

/// arclite's `format_data_size` with its suffixes (none for bytes, KB, MB, GB, TB).
fn short_size(value: u64) -> String {
    format_data_size(value, &["", "KB", "MB", "GB", "TB"])
}

/// arclite's `format_data_size`: by 1024 past 1000, three digits, `suffixes` for bytes to terabytes (an
/// empty one: none).
pub fn format_data_size(value: u64, suffixes: &[&str; 5]) -> String {
    let mut step = 0;
    let mut div: u64 = 1;
    while value / div >= 1000 && step < 4 {
        step += 1;
        div *= 1024;
    }
    let mut whole = value / div;
    let mul = match whole {
        0..10 => 100,
        10..100 => 10,
        _ => 1,
    };
    let rest = u128::from(value % div);
    let (mul_wide, div_wide) = (u128::from(mul), u128::from(div));
    let digit = rest * mul_wide * 10 / div_wide % 10;
    let mut fraction = u64::try_from(rest * mul_wide / div_wide).unwrap_or(0);
    if digit >= 5 {
        if fraction + 1 == mul {
            fraction = 0;
            if whole == 999 && step < 4 {
                whole = 0;
                fraction = 98;
                step += 1;
            } else {
                whole += 1;
            }
        } else {
            fraction += 1;
        }
    }
    let mut text = whole.to_string();
    if fraction != 0 {
        text.push('.');
        if whole < 10 && fraction < 10 {
            text.push('0');
        }
        text.push_str(&fraction.to_string());
    }
    if !suffixes[step].is_empty() {
        text.push(' ');
        text.push_str(suffixes[step]);
    }
    text
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
