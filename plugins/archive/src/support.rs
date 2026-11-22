//! What the program's tests and e2e stands use to make and read archives with no tool of their own: a
//! zip's CRC-32 (the standard one, as a zip's headers keep it), a zip of stored files, and an archive
//! file read as the panel reads it (its first way, every format tried through 7-Zip's library, no
//! password asked), a disk image of two partitions.

use std::path::Path;

use nc_plugin_sdk::sevenzip::{Library, Opening};

use crate::library_open::{Request, open};
use crate::opened::Opened;

/// The CRC-32 of `data` (IEEE 802.3, reflected, as zip keeps it).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low != 0 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

/// A zip's 32-bit field of `value` (a test's archive is small: past the field, its largest).
fn u32_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A zip's 16-bit field of `value`, as [`u32_of`].
fn u16_of(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// A zip of stored `files` (name, data), as any tool writes one.
pub fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = u32_of(out.len());
        let crc = crc32(data);
        let size = u32_of(data.len());
        let name_len = u16_of(name.len());
        let common = |out: &mut Vec<u8>| {
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // stored
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0x5221u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
        };
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        common(&mut out);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        common(&mut central);
        central.extend_from_slice(&[0; 6]); // comment, disk, internal attributes
        central.extend_from_slice(&0x20u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = u32_of(out.len());
    let count = u16_of(files.len());
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&u32_of(central.len()).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The archive file at `path` as the panel reads it; none when it is no archive or 7-Zip's library is
/// not found.
pub fn opened(path: &Path) -> Option<Opened> {
    struct NoAsk;
    impl Opening for NoAsk {}
    let library = Library::find(Path::new("."));
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let chain = open(&request, path, &mut NoAsk).ok()?.into_iter().next()?;
    let name = chain.name(&formats);
    Opened::from_library(path, chain, (name, ""), None).ok()
}

/// The items' paths (`/` between parts) of the archive at `path`, in its order; none when it is no
/// archive.
pub fn names_in(path: &Path) -> Vec<String> {
    opened(path).map_or_else(Vec::new, |opened| {
        (opened.items.iter())
            .map(|item| item.path.join("/"))
            .collect()
    })
}

const SECTOR: usize = 512;

/// A FAT12 volume of 64 sectors holding `HELLO.TXT` ("hello").
fn fat12() -> Vec<u8> {
    let mut volume = vec![0u8; 64 * SECTOR];
    let boot = &mut volume[..SECTOR];
    boot[..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
    boot[3..11].copy_from_slice(b"MSDOS5.0");
    boot[11..13].copy_from_slice(&512u16.to_le_bytes());
    boot[13] = 1; // sectors a cluster
    boot[14..16].copy_from_slice(&1u16.to_le_bytes()); // reserved
    boot[16] = 2; // FATs
    boot[17..19].copy_from_slice(&16u16.to_le_bytes()); // root entries
    boot[19..21].copy_from_slice(&64u16.to_le_bytes()); // sectors
    boot[21] = 0xF8;
    boot[22..24].copy_from_slice(&1u16.to_le_bytes()); // sectors a FAT
    boot[24..26].copy_from_slice(&32u16.to_le_bytes());
    boot[26..28].copy_from_slice(&2u16.to_le_bytes());
    boot[38] = 0x29;
    boot[39..43].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    boot[43..54].copy_from_slice(b"NO NAME    ");
    boot[54..62].copy_from_slice(b"FAT12   ");
    boot[510] = 0x55;
    boot[511] = 0xAA;
    for fat in [1, 2] {
        volume[fat * SECTOR..fat * SECTOR + 6].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0x0F, 0]);
    }
    let entry = &mut volume[3 * SECTOR..3 * SECTOR + 32];
    entry[..11].copy_from_slice(b"HELLO   TXT");
    entry[11] = 0x20;
    entry[26..28].copy_from_slice(&2u16.to_le_bytes());
    entry[28..32].copy_from_slice(&5u32.to_le_bytes());
    volume[4 * SECTOR..4 * SECTOR + 5].copy_from_slice(b"hello");
    volume
}

/// A disk image at `path`: one sector of MBR then two FAT12 volumes as its partitions (type 1; two, so 7-Zip names
/// no main one to open inside).
pub fn mbr_disk(path: &Path) -> std::io::Result<()> {
    let volume = fat12();
    let sectors = (volume.len() / SECTOR) as u32;
    let mut image = vec![0u8; SECTOR];
    for (at, start) in [(446, 1), (462, 1 + sectors)] {
        let entry = &mut image[at..at + 16];
        entry[4] = 0x01;
        entry[8..12].copy_from_slice(&start.to_le_bytes());
        entry[12..16].copy_from_slice(&sectors.to_le_bytes());
    }
    image[510] = 0x55;
    image[511] = 0xAA;
    image.extend_from_slice(&volume);
    image.extend_from_slice(&volume);
    std::fs::write(path, image)
}

#[cfg(test)]
#[path = "support_tests.rs"]
mod tests;
