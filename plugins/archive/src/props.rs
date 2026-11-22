//! 7-Zip's properties as arclite names and writes them (attr.cpp: `c_prop_info`, the `format_*_prop`
//! functions, `load_arc_attr`): an archive's own on the info panel, an item's in Ctrl+A's window.

use nc_plugin_sdk::InfoLine;
use nc_plugin_sdk::sevenzip::{Archive, Prop, prop};

use crate::item::Item;
use crate::panel::size_text;
use crate::properties::{attributes_text, posix_text};

/// arclite's names of 7-Zip's properties from `kpidPath` (3) on, in their ids' order.
const NAMES: [&str; 102] = [
    "Path",
    "Name",
    "Extension",
    "Folder",
    "Size",
    "Packed Size",
    "Attributes",
    "Created",
    "Accessed",
    "Modified",
    "Solid",
    "Commented",
    "Encrypted",
    "Split Before",
    "Split After",
    "Dictionary",
    "CRC",
    "Type",
    "Anti",
    "Method",
    "Host OS",
    "File System",
    "User",
    "Group",
    "Block",
    "Comment",
    "Position",
    "Path Prefix",
    "Folders",
    "Files",
    "Version",
    "Volume",
    "Multivolume",
    "Offset",
    "Links",
    "Blocks",
    "Volumes",
    "Time Type",
    "64-bit",
    "Big-endian",
    "CPU",
    "Physical Size",
    "Headers Size",
    "Checksum",
    "Characteristics",
    "Virtual Address",
    "ID",
    "Short Name",
    "Creator Application",
    "Sector Size",
    "Mode",
    "Symbolic Link",
    "Error",
    "Total Size",
    "Free Space",
    "Cluster Size",
    "Label",
    "Local Name",
    "Provider",
    "NT Security",
    "Alternate Stream",
    "Aux",
    "Deleted",
    "Is Tree",
    "SHA-1",
    "SHA-256",
    "Error Type",
    "Errors",
    "Errors",
    "Warnings",
    "Warning",
    "Streams",
    "Alternate Streams",
    "Alternate Streams Size",
    "Virtual Size",
    "Unpack Size",
    "Total Physical Size",
    "Volume Index",
    "SubType",
    "Short Comment",
    "Code Page",
    "Is not archive type",
    "Physical Size can't be detected",
    "Zeros Tail Is Allowed",
    "Tail Size",
    "Embedded Stub Size",
    "Link",
    "Hard Link",
    "iNode",
    "Stream ID",
    "Read-only",
    "Out Name",
    "Copy Link",
    "Arc FileName",
    "Is Hash",
    "Metadata Changed",
    "User ID",
    "Group ID",
    "Device Major",
    "Device Minor",
    "Dev Major",
    "Dev Minor",
];

/// The ids arclite writes as sizes (`format_size_prop`).
const SIZES: [u32; 15] = [7, 8, 18, 44, 45, 52, 56, 57, 58, 76, 77, 78, 79, 87, 88];

/// How arclite writes a property's value when it has a function of its own for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Size,
    Time,
    Crc,
    Attributes,
    Mode,
    /// By the value's type (`format_str_prop`, `format_uint_prop`, ...).
    Plain,
}

fn shape(id: u32) -> Shape {
    match id {
        prop::ATTRIBUTES => Shape::Attributes,
        prop::CREATED | prop::ACCESSED | prop::MODIFIED => Shape::Time,
        prop::CRC => Shape::Crc,
        prop::POSIX_ATTRIBUTES => Shape::Mode,
        id if SIZES.contains(&id) => Shape::Size,
        _ => Shape::Plain,
    }
}

/// arclite's name of the property `id` (`find_prop_info`), else the format's own name for it, else its
/// number.
pub fn name(id: u32, own: Option<&str>) -> String {
    let known = (id.checked_sub(prop::PATH)).and_then(|at| NAMES.get(usize::try_from(at).ok()?));
    match (known, own) {
        (Some(name), _) => (*name).to_string(),
        (None, Some(own)) => own.to_string(),
        (None, None) => id.to_string(),
    }
}

/// arclite's text of the value of the property `id`: its own function's when it has one, else by the
/// value's type (a text's line breaks as spaces, "Yes" / "No", the number, the time); empty when none.
pub fn value_text(id: u32, value: &Prop) -> String {
    match (shape(id), value) {
        (Shape::Size, value) => value.number().map_or_else(String::new, size_text),
        (Shape::Time, Prop::Time(ticks)) | (Shape::Plain, Prop::Time(ticks)) => time_text(*ticks),
        (Shape::Crc, Prop::U32(crc)) => format!("{crc:08X}"),
        (Shape::Crc, Prop::U64(crc)) => format!("{crc:016X}"),
        (Shape::Attributes, Prop::U32(raw)) => attributes_text(*raw),
        (Shape::Mode, Prop::U32(mode)) => posix_text(*mode),
        (Shape::Plain, Prop::Text(text)) => text.replace(['\r', '\n'], " "),
        (Shape::Plain, Prop::Bool(on)) => (if *on { "Yes" } else { "No" }).to_string(),
        (Shape::Plain, Prop::U32(value)) => value.to_string(),
        (Shape::Plain, Prop::U64(value)) => value.to_string(),
        (Shape::Plain, Prop::I64(value)) => value.to_string(),
        _ => String::new(),
    }
}

/// A `FILETIME` as arclite's `format_filetime_prop` writes it: the user's short date and time, local.
fn time_text(ticks: u64) -> String {
    let local = i64::try_from(ticks)
        .ok()
        .and_then(nc_plugin_sdk::system::local_time);
    local.map_or_else(String::new, |local| {
        format!("{} {}", local.date, local.time)
    })
}

/// arclite's `get_attr_list`: every item property the format has, in its order, named and written as
/// arclite does (an empty value too).
pub fn item_properties(archive: &Archive, index: u32) -> Vec<(String, String)> {
    (archive.properties().iter())
        .map(|info| {
            let value = archive.property(index, info.id);
            (
                name(info.id, info.name.as_deref()),
                value_text(info.id, &value),
            )
        })
        .collect()
}

/// arclite's `load_arc_attr`: the path, the archive's properties with a value, the compression ratio and
/// the sizes (when every file tells its own), the files and the folders counted (`items` the archive's,
/// not the folders only implied).
pub fn info_lines(path: &str, archive: &Archive, items: &[Item], file_size: u64) -> Vec<InfoLine> {
    let line = |text: String, data: String| InfoLine {
        text,
        data,
        ..InfoLine::default()
    };
    let mut lines = vec![line(name(prop::PATH, None), path.to_string())];
    for info in archive.archive_properties() {
        let value = value_text(info.id, &archive.archive_property(info.id));
        if !value.is_empty() {
            lines.push(line(name(info.id, info.name.as_deref()), value));
        }
    }
    let files: Vec<u32> = (0..archive.count())
        .filter(|&index| !items.get(index as usize).is_some_and(|item| item.is_dir))
        .collect();
    let sum = |id: u32| -> Option<u64> {
        (files.iter()).try_fold(0u64, |total, &index| {
            Some(total.saturating_add(archive.property(index, id).number()?))
        })
    };
    if let Some(total) = sum(prop::SIZE) {
        lines.push(line(
            "Compression ratio".to_string(),
            format!("{}%", ratio(file_size, total)),
        ));
        lines.push(line("Data size".to_string(), size_text(total)));
    }
    if let Some(packed) = sum(prop::PACKED_SIZE) {
        lines.push(line("Packed size".to_string(), size_text(packed)));
    }
    lines.push(line("File count".to_string(), files.len().to_string()));
    let folders = (archive.count() as usize).saturating_sub(files.len());
    lines.push(line("Directory count".to_string(), folders.to_string()));
    lines
}

/// The archive's size to its data's in percent, rounded, at most 100 (an empty one 100).
fn ratio(archive: u64, data: u64) -> u64 {
    if data == 0 {
        return 100;
    }
    let percent = (u128::from(archive) * 200 + u128::from(data)) / (u128::from(data) * 2);
    u64::try_from(percent).unwrap_or(u64::MAX).min(100)
}

#[cfg(test)]
#[path = "props_tests.rs"]
mod tests;
