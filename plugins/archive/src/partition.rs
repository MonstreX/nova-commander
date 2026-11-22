//! A disk image's partitions as FAR's arclite shows them (plugin.cpp `part_mode`, `guess_fs_ext`,
//! `correct_part_root_name`, `set_partition`): an archive of 7-Zip's MBR or GPT format names each
//! partition at its root `N.mbr.<fs>` / `N.gpt.<fs>` by the file system its boot sector tells (else by
//! its name's ending); Enter or Ctrl+PgDn on one opens it by that file system's format from its stream,
//! its archive inside the disk's (`MBR→NTFS`).

use std::io;

use nc_plugin_sdk::sevenzip::{Archive, Format, Library, Opening, prop};

use crate::detect::Entry;
use crate::item::Item;
use crate::library_open::Chain;

/// 7-Zip's class id of its format of id byte `id` (arclite's `DEFINE_ARC_ID`).
fn seven_zip_id(id: u8) -> [u8; 16] {
    [
        0x69, 0x0F, 0x17, 0x23, 0xC1, 0x40, 0x8A, 0x27, 0x10, 0x00, 0x00, 0x01, 0x10, id, 0, 0,
    ]
}

/// tc4shell's exFAT format's class id (arclite's `c_guid_xfat`).
const EXFAT_ID: [u8; 16] = [
    0x1e, 0x7e, 0xa5, 0x3e, 0xcb, 0xdd, 0xf0, 0x45, 0x9a, 0xe2, 0x2e, 0xf7, 0xf3, 0x98, 0x42, 0x53,
];

/// The partitions' kind of a disk's format: `m` for MBR, `g` for GPT (arclite's `part_mode`).
pub fn part_mode(format: &Format) -> Option<char> {
    if format.class_id == seven_zip_id(0xDB) {
        Some('m')
    } else if format.class_id == seven_zip_id(0xCB) {
        Some('g')
    } else {
        None
    }
}

/// arclite's `guess_fs_ext`: the file system of a partition's first 2 KB (its boot sector's names, or
/// an ext superblock after a first KB of zeros, its revision and journal telling ext2 from ext3).
pub fn guess_fs_ext(start: &[u8]) -> Option<&'static str> {
    if start.len() < 2048 {
        return None;
    }
    let at = |offset: usize, text: &[u8]| &start[offset..offset + text.len()] == text;
    if start[510] == 0x55 && start[511] == 0xAA {
        if at(0x03, b"EXFAT   ") {
            return Some(".xfat");
        }
        if at(0x03, b"NTFS    ") {
            return Some(".ntfs");
        }
        if at(0x52, b"FAT32   ") || at(0x36, b"FAT16   ") || at(0x36, b"FAT12   ") {
            return Some(".fat");
        }
    }
    if start[0x438] == 0x53 && start[0x439] == 0xEF && start[..1024].iter().all(|byte| *byte == 0) {
        return Some(match start[0x44C] {
            0 => ".ext2",
            1 if start[0x45C] & 0x04 == 0 => ".ext2",
            1 => ".ext3",
            _ => ".ext",
        });
    }
    None
}

/// The ending arclite's `correct_part_root_name` falls back on: an APFS, HFS or ext4 name's.
fn ext_of_name(name: &str) -> Option<&'static str> {
    if name.ends_with(".apfs") {
        Some(".apfs")
    } else if name.ends_with(".hfs") || name.ends_with(".hfsx") {
        Some(".hfs")
    } else if name.ends_with(".ext4") {
        Some(".ext")
    } else {
        None
    }
}

/// arclite's checks of a partition (`part_index_to_item`, `set_partition`): a file at the root, of a size,
/// its packed size the same.
pub fn is_partition(item: &Item) -> bool {
    !item.is_dir && item.path.len() == 1 && item.size != 0 && item.size == item.packed
}

/// arclite's `correct_part_root_name`: a partition's root name (a partition, its offset told, its name
/// with an extension, not renamed yet) as `N.mbr.<fs>` by the file system `guessed` reads from its
/// start, else by its name's ending; none when it stays as it is.
pub fn root_name(
    (index, item, offset): (u32, &Item, Option<u64>),
    mode: char,
    guessed: impl FnOnce() -> Option<&'static str>,
) -> Option<String> {
    let name = item.path.first()?;
    if !is_partition(item) || offset.is_none() {
        return None;
    }
    let dot = name.find('.')?;
    let second = name[dot + 1..].find('.').map(|at| dot + 1 + at);
    if second == Some(dot + 4) && name[dot..].chars().nth(1) == Some(mode) {
        return None;
    }
    let ext = guessed().or_else(|| ext_of_name(name))?;
    let kind = if mode == 'm' { ".mbr" } else { ".gpt" };
    Some(format!("{index}{kind}{ext}"))
}

/// The partitions at the root of `archive` (of the partitions' kind `mode`) renamed in `items`.
pub fn rename_roots(archive: &Archive, items: &mut [Item], mode: char) {
    for (index, item) in items.iter_mut().enumerate() {
        let Ok(index) = u32::try_from(index) else {
            break;
        };
        let offset = archive.property(index, prop::OFFSET).number();
        let guessed = || {
            let stream = archive.stream(index)?;
            let mut start = vec![0u8; 2048];
            let read = stream.read(&mut start).ok()?;
            guess_fs_ext(&start[..read])
        };
        if let Some(name) = root_name((index, item, offset), mode, guessed) {
            item.path = vec![name];
        }
    }
}

/// The format a partition's name ends in (arclite's `set_partition`: FAT, exFAT, NTFS, APFS, HFS, ext).
pub fn format_of(name: &str, formats: &[Format]) -> Option<usize> {
    let table = [
        (".fat", seven_zip_id(0xDA)),
        (".xfat", EXFAT_ID),
        (".ntfs", seven_zip_id(0xD9)),
        (".apfs", seven_zip_id(0xC3)),
        (".hfs", seven_zip_id(0xE3)),
        (".ext2", seven_zip_id(0xC7)),
        (".ext3", seven_zip_id(0xC7)),
        (".ext4", seven_zip_id(0xC7)),
        (".ext", seven_zip_id(0xC7)),
    ];
    let (_, id) = table.iter().find(|(ext, _)| name.ends_with(ext))?;
    formats.iter().position(|format| format.class_id == id)
}

/// arclite's `set_partition` of item `index` (named `name`) of `chain`'s archive: its file system's
/// format opened from its stream, chained inside; none when it is no partition or does not open.
pub fn open(
    library: &Library,
    chain: &Chain,
    (index, name): (u32, &str),
    asker: &mut dyn Opening,
) -> io::Result<Option<Chain>> {
    let archive = chain.archive();
    let Some(offset) = archive.property(index, prop::OFFSET).number() else {
        return Ok(None);
    };
    let Some(format) = format_of(name, &library.formats()) else {
        return Ok(None);
    };
    let Some(stream) = archive.stream(index) else {
        return Ok(None);
    };
    let Some(inner) = library.open(format, &stream, None, true, asker)? else {
        return Ok(None);
    };
    let mut outer = chain.outer.clone();
    outer.push(std::rc::Rc::clone(&chain.archive));
    let mut parts = chain.parts.clone();
    parts.push(Entry { format, offset });
    Ok(Some(Chain {
        parts,
        archive: std::rc::Rc::new(inner),
        outer,
    }))
}

#[cfg(test)]
#[path = "partition_tests.rs"]
mod tests;
