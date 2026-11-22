//! An SFX module's icon and version information replaced as FAR's arclite does it (rsrc.cpp
//! `replace_icon`, `load_icon_file`, `load_icon_rsrc`, `replace_ver_info` with its `VersionEncoder`):
//! the module's icons deleted, the icon file's put first in place of its first one, the others kept,
//! all renumbered; its version information written anew from the options.

use std::io;
use std::path::Path;

use nc_plugin_sdk::system::{
    RT_GROUP_ICON, RT_ICON, RT_VERSION, ResourceChange, ResourceName, resource, resources,
    update_resources,
};

use crate::sfx::VersionInfo;

/// The system's `MAKELANGID(LANG_NEUTRAL, SUBLANG_NEUTRAL)`.
const NEUTRAL: u16 = 0;

/// One image of an icon (arclite's `IconImage`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IconImage {
    pub width: u8,
    pub height: u8,
    pub colors: u8,
    pub planes: u16,
    pub bits: u16,
    pub bitmap: Vec<u8>,
}

fn word(data: &[u8], at: usize) -> io::Result<u16> {
    (data.get(at..at + 2))
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
}

fn dword(data: &[u8], at: usize) -> io::Result<u32> {
    Ok(u32::from(word(data, at)?) | (u32::from(word(data, at + 2)?) << 16))
}

fn byte(data: &[u8], at: usize) -> io::Result<u8> {
    (data.get(at).copied()).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
}

/// The images of an icon file (arclite's `load_icon_file`: its header, a 16-byte entry each, an image
/// past the file's end refused).
pub fn icon_file(data: &[u8]) -> io::Result<Vec<IconImage>> {
    let count = word(data, 4)?;
    let mut images = Vec::new();
    for index in 0..usize::from(count) {
        let at = 6 + index * 16;
        let size = usize::try_from(dword(data, at + 8)?).unwrap_or(usize::MAX);
        let offset = usize::try_from(dword(data, at + 12)?).unwrap_or(usize::MAX);
        let bitmap = (offset.checked_add(size))
            .and_then(|end| data.get(offset..end))
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        images.push(IconImage {
            width: byte(data, at)?,
            height: byte(data, at + 1)?,
            colors: byte(data, at + 2)?,
            planes: word(data, at + 4)?,
            bits: word(data, at + 6)?,
            bitmap: bitmap.to_vec(),
        });
    }
    Ok(images)
}

/// An icon group's entries: each image's fields and its `RT_ICON` id (arclite's `IconGroupEntry`, 14
/// bytes after a 6-byte header); the bitmaps left empty.
pub fn group_entries(data: &[u8]) -> io::Result<Vec<(IconImage, u16)>> {
    let count = word(data, 4)?;
    (0..usize::from(count))
        .map(|index| {
            let at = 6 + index * 14;
            let image = IconImage {
                width: byte(data, at)?,
                height: byte(data, at + 1)?,
                colors: byte(data, at + 2)?,
                planes: word(data, at + 4)?,
                bits: word(data, at + 6)?,
                bitmap: Vec::new(),
            };
            Ok((image, word(data, at + 12)?))
        })
        .collect()
}

/// An icon group of `images` with their ids (arclite's writing of `IconGroupHeader` and entries).
pub fn group_bytes(images: &[(IconImage, u16)]) -> Vec<u8> {
    let mut data = vec![0, 0, 1, 0];
    data.extend_from_slice(
        &u16::try_from(images.len())
            .unwrap_or(u16::MAX)
            .to_le_bytes(),
    );
    for (image, id) in images {
        data.extend_from_slice(&[image.width, image.height, image.colors, 0]);
        data.extend_from_slice(&image.planes.to_le_bytes());
        data.extend_from_slice(&image.bits.to_le_bytes());
        let size = u32::try_from(image.bitmap.len()).unwrap_or(u32::MAX);
        data.extend_from_slice(&size.to_le_bytes());
        data.extend_from_slice(&id.to_le_bytes());
    }
    data
}

/// An icon of the module (arclite's `IconRsrc`): its name, language and images with theirs.
struct Icon {
    name: ResourceName,
    language: u16,
    images: Vec<(IconImage, u16)>,
}

/// The module's icons as they are (arclite's `load_icon_rsrc` of each group and language).
fn icons_of(pe: &Path) -> io::Result<Vec<Icon>> {
    let mut icons = Vec::new();
    for (name, language) in resources(pe, RT_GROUP_ICON)? {
        let group = resource(pe, RT_GROUP_ICON, &name, language)?;
        let mut images = Vec::new();
        for (mut image, id) in group_entries(&group)? {
            image.bitmap = resource(pe, RT_ICON, &ResourceName::Id(id), language)?;
            images.push((image, id));
        }
        icons.push(Icon {
            name,
            language,
            images,
        });
    }
    Ok(icons)
}

/// arclite's `replace_icon`: `pe`'s icons deleted; the icon file at `ico` put first in the first one's
/// place (all its languages gone), in its language; every icon and image renumbered from 1 (a named icon
/// keeps its name) and written again.
pub fn replace_icon(pe: &Path, ico: &Path) -> io::Result<()> {
    let mut icons = icons_of(pe)?;
    let mut changes = Vec::new();
    for icon in &icons {
        for (_, id) in &icon.images {
            changes.push(change(RT_ICON, ResourceName::Id(*id), icon.language, None));
        }
        changes.push(change(
            RT_GROUP_ICON,
            icon.name.clone(),
            icon.language,
            None,
        ));
    }
    let language = icons.first().map_or(NEUTRAL, |icon| icon.language);
    let images = icon_file(&std::fs::read(ico)?)?;
    if let Some(first) = icons.first().map(|icon| icon.name.clone()) {
        let others = icons.iter().position(|icon| icon.name != first);
        icons.drain(..others.unwrap_or(icons.len()));
    }
    icons.insert(
        0,
        Icon {
            name: ResourceName::Id(0),
            language,
            images: images.into_iter().map(|image| (image, 0)).collect(),
        },
    );
    let (mut icon_id, mut image_id) = (1u16, 1u16);
    for icon in &mut icons {
        if matches!(icon.name, ResourceName::Id(_)) {
            icon.name = ResourceName::Id(icon_id);
            icon_id = icon_id.wrapping_add(1);
        }
        for (_, id) in &mut icon.images {
            *id = image_id;
            image_id = image_id.wrapping_add(1);
        }
    }
    for icon in icons {
        let group = group_bytes(&icon.images);
        for (image, id) in icon.images {
            changes.push(change(
                RT_ICON,
                ResourceName::Id(id),
                icon.language,
                Some(image.bitmap),
            ));
        }
        changes.push(change(RT_GROUP_ICON, icon.name, icon.language, Some(group)));
    }
    update_resources(pe, &changes)
}

fn change(kind: u16, name: ResourceName, language: u16, data: Option<Vec<u8>>) -> ResourceChange {
    ResourceChange {
        kind,
        name,
        language,
        data,
    }
}

/// arclite's `str_to_int` of a version's part: its leading digits (none: 0).
fn number(part: &str) -> u32 {
    let digits: String = part
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse::<u64>()
        .map_or(0, |value| (value & 0xFFFF) as u32)
}

/// The version's high and low double words from its first four parts (`1.2.3.4`).
pub fn version_numbers(version: &str) -> (u32, u32) {
    let mut parts = version.split('.').map(number);
    let mut next = || parts.next().unwrap_or(0);
    let high = (next() << 16) | next();
    let low = (next() << 16) | next();
    (high, low)
}

/// arclite's `VersionEncoder`: words, double words, NUL-terminated UTF-16 texts, padding to 4 bytes
/// and a block's length written at its start.
#[derive(Default)]
struct Encoder(Vec<u8>);

impl Encoder {
    fn word(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn dword(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn text(&mut self, text: &str) {
        for unit in text.encode_utf16().chain([0]) {
            self.word(unit);
        }
    }

    fn pad(&mut self) {
        while !self.0.len().is_multiple_of(4) {
            self.0.push(0);
        }
    }

    /// A block's header: its length (set by `close`), value length, type and key; padded.
    fn open(&mut self, value_length: u16, kind: u16, key: &str) -> usize {
        let at = self.0.len();
        self.word(0);
        self.word(value_length);
        self.word(kind);
        self.text(key);
        self.pad();
        at
    }

    fn close(&mut self, at: usize) {
        let length = u16::try_from(self.0.len() - at).unwrap_or(u16::MAX);
        self.0[at..at + 2].copy_from_slice(&length.to_le_bytes());
    }
}

/// The system's `VS_FIXEDFILEINFO` size, `VOS_NT_WINDOWS32`, `VFT_APP`.
const FIXED_SIZE: u16 = 52;
const VOS_NT_WINDOWS32: u32 = 0x0004_0004;
const VFT_APP: u32 = 1;

/// arclite's version information of `info` in `language` (its `replace_ver_info`'s encoding).
pub fn version_bytes(info: &VersionInfo, language: u16) -> Vec<u8> {
    let (high, low) = version_numbers(&info.version);
    let mut e = Encoder::default();
    let root = e.open(FIXED_SIZE, 0, "VS_VERSION_INFO");
    for value in [
        0xFEEF_04BD,
        0x0001_0000,
        high,
        low,
        high,
        low,
        0x3F,
        0,
        VOS_NT_WINDOWS32,
        VFT_APP,
        0,
        0,
        0,
    ] {
        e.dword(value);
    }
    e.pad();
    let file_info = e.open(0, 1, "StringFileInfo");
    let table = e.open(0, 1, &format!("{language:04x}04b0"));
    for (key, value) in [
        ("Comments", &info.comments),
        ("CompanyName", &info.company_name),
        ("FileDescription", &info.file_description),
        ("FileVersion", &info.version),
        ("LegalCopyright", &info.legal_copyright),
        ("ProductName", &info.product_name),
        ("ProductVersion", &info.version),
    ] {
        let length = u16::try_from(value.encode_utf16().count() + 1).unwrap_or(u16::MAX);
        let string = e.open(length, 1, key);
        e.text(value);
        e.pad();
        e.close(string);
    }
    e.close(table);
    e.close(file_info);
    let var_info = e.open(0, 1, "VarFileInfo");
    let var = e.open(4, 0, "Translation");
    e.word(language);
    e.word(0x04B0);
    e.close(var);
    e.close(var_info);
    e.close(root);
    e.0
}

/// arclite's `replace_ver_info`: every version information of `pe` deleted, the new one written in the
/// first one's name and language (else 1, neutral).
pub fn replace_version(pe: &Path, info: &VersionInfo) -> io::Result<()> {
    let found = resources(pe, RT_VERSION)?;
    let (name, language) = (found.first().cloned()).unwrap_or((ResourceName::Id(1), NEUTRAL));
    let mut changes: Vec<ResourceChange> = (found.into_iter())
        .map(|(name, language)| change(RT_VERSION, name, language, None))
        .collect();
    let data = version_bytes(info, language);
    changes.push(change(RT_VERSION, name, language, Some(data)));
    update_resources(pe, &changes)
}

#[cfg(test)]
#[path = "sfx_rsrc_tests.rs"]
mod tests;
