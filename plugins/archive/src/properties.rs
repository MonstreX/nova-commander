//! Ctrl+A on an archive's panel: arclite's "Attributes" (`show_attr`, attr.cpp `get_attr_list`, ui.cpp
//! `AttrDialog`) - every property the item's format has, in arclite's names and formats
//! ([`crate::props::item_properties`]); on ".." at the root the archive's own (the info panel's lines).
//! Its "..." leaves Ctrl+A to the program (arclite's `false`: FAR's own attributes dialog).

use nc_plugin_sdk::{Dialog, DialogItem};

use crate::layout::Layout;
use crate::opened::Opened;

/// arclite's `g_WinAttribChars`: a letter for each of the first 21 attribute bits, `.` none.
const ATTRIBUTE_CHARS: &str = "RHS8DAdNTsLCOIEV.X.PU";

/// arclite's `kPosixTypes`.
const POSIX_TYPES: &str = "0pc3d5b7-9lBsDEF";

/// arclite's `uint_to_hex_str`: upper case, at least `digits`.
fn hex(value: u64, digits: usize) -> String {
    format!("{value:0digits$X}")
}

/// arclite's `format_posix_attrib_prop`: the type, `rwx` three times (set-id and sticky bits), the high
/// bits in hex after a space.
pub fn posix_text(mode: u32) -> String {
    let kind = POSIX_TYPES.as_bytes()[usize::try_from((mode >> 12) & 0xF).unwrap_or(0)];
    let mut text = vec![char::from(kind)];
    for shift in [6, 3, 0] {
        for (bit, letter) in [(2, 'r'), (1, 'w'), (0, 'x')] {
            text.push(if mode & (1 << (shift + bit)) != 0 {
                letter
            } else {
                '-'
            });
        }
    }
    let special = |text: &mut Vec<char>, flag: u32, at: usize, exec: u32, letter: char| {
        if mode & flag != 0 {
            text[at] = if mode & exec != 0 {
                letter
            } else {
                letter.to_ascii_uppercase()
            };
        }
    };
    special(&mut text, 0x800, 3, 1 << 6, 's');
    special(&mut text, 0x400, 6, 1 << 3, 's');
    special(&mut text, 0x200, 9, 1, 't');
    let mut out: String = text.into_iter().collect();
    let high = mode & !0xFFFF;
    if high != 0 {
        out.push(' ');
        out.push_str(&hex(u64::from(high), 8));
    }
    out
}

/// arclite's `format_attrib_prop` on 7-Zip's attributes (`raw`): Windows' letters, bits without one in
/// hex, the Unix mode of the high half after them.
pub fn attributes_text(raw: u32) -> String {
    let (posix, mut nt) = if raw & 0xF000_0000 != 0 {
        (raw >> 16, raw & 0x3FFF)
    } else {
        (0, raw)
    };
    let mut text = String::new();
    for (bit, letter) in ATTRIBUTE_CHARS.chars().enumerate() {
        let flag = 1u32 << bit;
        if nt & flag != 0 && letter != '.' {
            nt &= !flag;
            text.push(letter);
        }
    }
    if nt != 0 {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&hex(u64::from(nt), 8));
    }
    if posix != 0 {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&posix_text(posix));
    }
    text
}

/// arclite's `get_attr_list` of an archive 7-Zip's library reads: every item property its format has,
/// named and written as arclite does (its raw properties, NT security and reparse data, not read).
pub fn item_properties(opened: &Opened, index: usize) -> Vec<(String, String)> {
    let (Some((chain, _)), Ok(index)) = (opened.library(), u32::try_from(index)) else {
        return Vec::new();
    };
    crate::props::item_properties(chain.archive(), index)
}

/// What Ctrl+A shows in the folder `path` with `current` under the cursor (none: ".."), as arclite's
/// `show_attr`: the archive's own properties (the info panel's lines) on ".." at the root, the folder's
/// on ".." below it, the entry's otherwise (a file before a folder of its name, as the panel lists the
/// file); none for a folder only implied.
pub fn shown_for(opened: &Opened, (path, current): (&str, Option<&str>)) -> Vec<(String, String)> {
    let node = match current {
        None if path.trim_matches('/').is_empty() => {
            return (crate::panel::info_lines(opened).into_iter())
                .map(|line| (line.text, line.data))
                .collect();
        }
        None => opened.tree.find_dir(path),
        Some(name) => {
            let folder = opened.tree.find_dir(path);
            let children = folder.map_or(&[][..], |folder| &opened.tree.nodes[folder].children[..]);
            let named = |dir: bool| {
                (children.iter().copied()).find(|&node| {
                    opened.tree.nodes[node].name == name && opened.tree.nodes[node].is_dir == dir
                })
            };
            named(false).or_else(|| named(true))
        }
    };
    (node.and_then(|node| opened.tree.nodes[node].item))
        .map_or_else(Vec::new, |item| item_properties(opened, item))
}

/// arclite's `AttrDialog`: each name in a column as wide as the widest (half the width at most), its
/// value in a read-only line one wider than the longest value, within `width` (FAR's optimal message
/// width); OK and "..." under them.
pub fn dialog(properties: &[(String, String)], width: usize) -> Dialog {
    let names = properties
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    let values = properties
        .iter()
        .map(|(_, value)| value.chars().count())
        .max()
        .unwrap_or(0)
        + 1;
    let names = names.min(width / 2);
    let values = values.min(width.saturating_sub(names + 1));
    let as_i32 = |value: usize| i32::try_from(value).unwrap_or(i32::MAX);
    let mut layout = Layout::new("Attributes", as_i32(names + 1 + values));
    for (name, value) in properties {
        let shown: String = name.chars().take(names).collect();
        layout.label(&shown.replace('&', "&&"));
        layout.pad(as_i32(names + 1));
        layout.edit(DialogItem::edit(value).read_only(), as_i32(values));
        layout.new_line();
    }
    layout.separator();
    layout.new_line();
    layout.buttons(vec![
        DialogItem::button("OK").default_button(),
        DialogItem::button("..."),
    ]);
    layout.new_line();
    layout.dialog()
}

#[cfg(test)]
#[path = "properties_tests.rs"]
mod tests;
