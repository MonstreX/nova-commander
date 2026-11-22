//! An archive's items as arclite reads them through 7-Zip's library (archive.cpp `make_index`,
//! `get_attr`, `get_size`, the `get_*time` functions, `get_default_name`): the path (the archive's name
//! less its extension when the format gives none, as `tgz` stands for a tar), whether it is a folder, its
//! sizes (none for a folder), its attributes (a Unix mode's own bits dropped), its times (the archive
//! file's when it tells none), whether it is encrypted.

use std::time::SystemTime;

use nc_plugin_sdk::sevenzip::{Archive, Format, Prop, prop};

use crate::item::{DIRECTORY, Item, from_ticks, split};
use crate::opened::FileInfo;

/// Every item of `archive`, in its order; `default_name` for one without a path.
pub fn items(archive: &Archive, default_name: &str, file: &FileInfo) -> Vec<Item> {
    (0..archive.count())
        .map(|index| item(archive, index, default_name, file))
        .collect()
}

fn item(archive: &Archive, index: u32, default_name: &str, file: &FileInfo) -> Item {
    let is_dir = archive.property(index, prop::IS_DIR) == Prop::Bool(true);
    let path = match archive.property(index, prop::PATH) {
        Prop::Text(path) => path,
        _ => default_name.to_string(),
    };
    let size = |id| {
        if is_dir {
            0
        } else {
            archive.property(index, id).number().unwrap_or(0)
        }
    };
    let time = |id, otherwise: Option<SystemTime>| match archive.property(index, id) {
        Prop::Time(ticks) => from_ticks(ticks),
        _ => otherwise,
    };
    Item {
        path: split(&path),
        is_dir,
        size: size(prop::SIZE),
        packed: size(prop::PACKED_SIZE),
        modified: time(prop::MODIFIED, file.modified),
        created: time(prop::CREATED, file.created),
        accessed: time(prop::ACCESSED, file.accessed),
        attributes: attributes(&archive.property(index, prop::ATTRIBUTES), is_dir),
        encrypted: archive.property(index, prop::ENCRYPTED) == Prop::Bool(true),
    }
}

/// arclite's `get_attr`: the Windows half of `get_posix_and_nt_attributes`, a folder always one.
fn attributes(raw: &Prop, is_dir: bool) -> u32 {
    let raw = match raw {
        Prop::U32(raw) => *raw,
        _ => 0,
    };
    let nt = if raw & 0xF000_0000 != 0 {
        raw & 0x3FFF
    } else {
        raw
    };
    if is_dir { nt | DIRECTORY } else { nt }
}

/// arclite's `get_default_name`: the archive's name less its extension, that extension's inner one put
/// on when the format names one (`a.tgz` gives `a.tar`) and the rest has not it already.
pub fn default_name(archive_name: &str, format: Option<&Format>) -> String {
    let (stem, extension) = split_extension(archive_name);
    let Some(format) = format else {
        return stem.to_string();
    };
    let inner = (format.extensions.iter().zip(&format.add_extensions))
        .find(|(ext, _)| extension.len() > 1 && ext.eq_ignore_ascii_case(&extension[1..]))
        .map(|(_, add)| add.as_str())
        .filter(|add| *add != "*");
    let Some(inner) = inner else {
        return stem.to_string();
    };
    let (base, own) = split_extension(stem);
    if own.eq_ignore_ascii_case(inner) {
        return stem.to_string();
    }
    format!("{base}{inner}")
}

/// A name and its extension with the dot (empty when it has none: FAR's `extract_file_ext`).
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if !name[dot..].contains(['\\', '/']) => name.split_at(dot),
        _ => (name, ""),
    }
}

#[cfg(test)]
#[path = "library_items_tests.rs"]
mod tests;
