//! Icon files and groups read and written, the version information encoded; both replaced in a copy of
//! the installed 7-Zip's `7z.sfx`.

use std::path::PathBuf;

use super::*;

/// An icon file of `images` (a 16-byte entry each, the bitmaps after the entries).
fn ico_of(images: &[IconImage]) -> Vec<u8> {
    let count = u16::try_from(images.len()).unwrap();
    let mut data = vec![0, 0, 1, 0];
    data.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for image in images {
        data.extend_from_slice(&[image.width, image.height, image.colors, 0]);
        data.extend_from_slice(&image.planes.to_le_bytes());
        data.extend_from_slice(&image.bits.to_le_bytes());
        data.extend_from_slice(&u32::try_from(image.bitmap.len()).unwrap().to_le_bytes());
        data.extend_from_slice(&u32::try_from(offset).unwrap().to_le_bytes());
        offset += image.bitmap.len();
    }
    for image in images {
        data.extend_from_slice(&image.bitmap);
    }
    data
}

fn image(width: u8, fill: u8) -> IconImage {
    IconImage {
        width,
        height: width,
        colors: 0,
        planes: 1,
        bits: 32,
        bitmap: vec![fill; usize::from(width) + 40],
    }
}

/// An icon file's images read back; one past the file's end refused; a header cut short refused.
#[test]
fn icon_files_are_read() {
    let images = [image(16, 1), image(32, 2)];
    let data = ico_of(&images);
    assert_eq!(icon_file(&data).unwrap(), images);
    let cut = &data[..data.len() - 1];
    assert!(icon_file(cut).is_err());
    assert!(icon_file(&data[..3]).is_err());
}

/// A group written with its images' ids and sizes and read back the same, bitmaps left out.
#[test]
fn icon_groups_are_written_and_read() {
    let images = vec![(image(16, 1), 7), (image(48, 2), 9)];
    let group = group_bytes(&images);
    assert_eq!(group.len(), 6 + 2 * 14);
    assert_eq!(&group[..6], &[0, 0, 1, 0, 2, 0]);
    assert_eq!(u32::from_le_bytes(group[14..18].try_into().unwrap()), 56);
    let read = group_entries(&group).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[1].1, 9);
    assert_eq!(read[1].0.width, 48);
    assert_eq!(read[0].0.bits, 32);
    assert!(read[0].0.bitmap.is_empty());
}

/// The version's four parts in two double words, each part's leading digits taken as arclite's
/// `str_to_int`, a part past 16 bits cut, missing parts 0.
#[test]
fn version_parts_make_two_double_words() {
    assert_eq!(version_numbers("1.2.3.4"), (0x0001_0002, 0x0003_0004));
    assert_eq!(version_numbers("10"), (0x000A_0000, 0));
    assert_eq!(version_numbers(""), (0, 0));
    assert_eq!(version_numbers("2x.70000.b.1.9"), (0x0002_1170, 1));
}

/// The encoded information: its length first, the fixed part's signature and versions, the strings'
/// table of the language, the translation at the end.
#[test]
fn version_information_is_encoded_as_arclite_does() {
    let info = VersionInfo {
        version: "1.2.3.4".to_string(),
        product_name: "Product".to_string(),
        ..VersionInfo::default()
    };
    let data = version_bytes(&info, 0x0419);
    assert_eq!(
        usize::from(u16::from_le_bytes([data[0], data[1]])),
        data.len()
    );
    assert_eq!(u16::from_le_bytes([data[2], data[3]]), 52);
    let wide = |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let has = |text: &str| data.windows(wide(text).len()).any(|w| w == wide(text));
    assert!(has("VS_VERSION_INFO"));
    assert!(has("041904b0"));
    assert!(has("ProductName") && has("Product") && has("ProductVersion"));
    let signature = 0xFEEF_04BDu32.to_le_bytes();
    let at = data.windows(4).position(|w| w == signature).unwrap();
    assert_eq!(at % 4, 0);
    assert_eq!(&data[at + 8..at + 12], &0x0001_0002u32.to_le_bytes());
    assert_eq!(&data[at + 12..at + 16], &0x0003_0004u32.to_le_bytes());
    assert_eq!(&data[data.len() - 4..], &[0x19, 0x04, 0xB0, 0x04]);
}

/// The string `key` of encoded version information: its value (after the key, padded to 4 bytes) and
/// the value's length its block's header tells (in units, its NUL counted).
fn string_of(data: &[u8], key: &str) -> (String, u16) {
    let mut wide: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
    wide.extend([0, 0]);
    let at = (0..data.len() - wide.len())
        .find(|at| data[*at..].starts_with(&wide) && (*at - 6) % 4 == 0)
        .unwrap_or_else(|| panic!("no {key}"));
    let value_length = u16::from_le_bytes([data[at - 4], data[at - 3]]);
    let start = (at + wide.len()).next_multiple_of(4);
    let units: Vec<u16> = (data[start..].chunks(2))
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    (String::from_utf16(&units).unwrap(), value_length)
}

/// Each string under its own key, the version as both file's and product's; the fixed part's versions
/// twice, its flags mask, the system and the type of a program.
#[test]
fn each_version_string_is_under_its_key() {
    let info = VersionInfo {
        version: "1.2.3.4".to_string(),
        comments: "Some comments".to_string(),
        company_name: "Company".to_string(),
        file_description: "Description".to_string(),
        legal_copyright: "(c) me".to_string(),
        product_name: "Product".to_string(),
    };
    let data = version_bytes(&info, 0x0409);
    for (key, value) in [
        ("Comments", "Some comments"),
        ("CompanyName", "Company"),
        ("FileDescription", "Description"),
        ("FileVersion", "1.2.3.4"),
        ("LegalCopyright", "(c) me"),
        ("ProductName", "Product"),
        ("ProductVersion", "1.2.3.4"),
    ] {
        let length = u16::try_from(value.len() + 1).unwrap();
        assert_eq!(string_of(&data, key), (value.to_string(), length), "{key}");
    }
    let signature = 0xFEEF_04BDu32.to_le_bytes();
    let at = data.windows(4).position(|w| w == signature).unwrap();
    let dword =
        |offset: usize| u32::from_le_bytes(data[at + offset..at + offset + 4].try_into().unwrap());
    assert_eq!(dword(4), 0x0001_0000, "the structure's version");
    assert_eq!(
        (dword(16), dword(20)),
        (0x0001_0002, 0x0003_0004),
        "the product's"
    );
    assert_eq!(
        (dword(24), dword(28)),
        (0x3F, 0),
        "the flags' mask, no flags"
    );
    assert_eq!(
        (dword(32), dword(36)),
        (0x0004_0004, 1),
        "Windows NT, a program"
    );
    let (_, translation) = string_of(&data, "Translation");
    assert_eq!(translation, 4, "its value's bytes");
}

/// The blocks' lengths: the strings' table and each string's block end where the next begins.
#[test]
fn every_block_tells_its_length() {
    let data = version_bytes(&VersionInfo::default(), 0);
    let wide = |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let block = |key: &str| {
        let key = wide(key);
        let at = data.windows(key.len()).position(|w| w == key).unwrap() - 6;
        (
            at,
            usize::from(u16::from_le_bytes([data[at], data[at + 1]])),
        )
    };
    let (file_info, file_info_len) = block("StringFileInfo");
    let (var_info, var_info_len) = block("VarFileInfo");
    assert_eq!(
        file_info + file_info_len,
        var_info,
        "the strings end where VarFileInfo begins"
    );
    assert_eq!(
        var_info + var_info_len,
        data.len(),
        "VarFileInfo ends the whole"
    );
    let (table, table_len) = block("000004b0");
    assert_eq!(
        table + table_len,
        var_info,
        "the table ends with StringFileInfo"
    );
    let (comments, comments_len) = block("Comments");
    let (company, _) = block("CompanyName");
    assert_eq!(
        comments + comments_len,
        company,
        "a string ends where the next begins"
    );
    assert_eq!(
        string_of(&data, "Comments"),
        (String::new(), 1),
        "an empty one: its NUL"
    );
}

/// An icon group's every field read back, the bitmaps' sizes written; an entry cut short refused.
#[test]
fn an_icon_groups_fields_are_kept() {
    let image = IconImage {
        width: 48,
        height: 32,
        colors: 16,
        planes: 2,
        bits: 8,
        bitmap: vec![7; 300],
    };
    let group = group_bytes(&[(image.clone(), 513)]);
    assert_eq!(&group[6..10], &[48, 32, 16, 0]);
    assert_eq!(u32::from_le_bytes(group[14..18].try_into().unwrap()), 300);
    let read = group_entries(&group).unwrap();
    let expected = IconImage {
        bitmap: Vec::new(),
        ..image
    };
    assert_eq!(read, [(expected, 513)]);
    assert!(group_entries(&group[..group.len() - 1]).is_err());
    assert_eq!(group_entries(&group_bytes(&[])).unwrap(), []);
}

/// A copy of the installed 7-Zip's `7z.sfx`.
fn module_copy(folder: &Path) -> PathBuf {
    let modules = nc_plugin_sdk::sevenzip::Library::find(folder).sfx_modules();
    let module = (modules.iter())
        .find(|path| path.ends_with("7z.sfx"))
        .expect("7-Zip's 7z.sfx installed");
    let copy = folder.join("copy.exe");
    std::fs::copy(module, &copy).unwrap();
    copy
}

/// The module's version replaced: the system reads the new one; its icon replaced: the first group is
/// the icon file's images, numbered from 1, the module's other groups kept (Windows: the system reads the
/// version back).
#[cfg(windows)]
#[test]
fn a_modules_version_and_icon_are_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let pe = module_copy(temp.path());
    let groups_before = resources(&pe, RT_GROUP_ICON).unwrap();
    let info = VersionInfo {
        version: "5.6.7.8".to_string(),
        file_description: "My setup".to_string(),
        ..VersionInfo::default()
    };
    replace_version(&pe, &info).unwrap();
    let read = nc_plugin_sdk::system::file_version(&pe).unwrap();
    assert_eq!(read.version.as_deref(), Some("5.6.7.8"));
    assert_eq!(read.description.as_deref(), Some("My setup"));
    assert_eq!(
        nc_plugin_sdk::system::fixed_version(&pe),
        Some((0x0005_0006 << 32) | 0x0007_0008)
    );
    let ico = temp.path().join("a.ico");
    std::fs::write(&ico, ico_of(&[image(16, 3), image(24, 4)])).unwrap();
    replace_icon(&pe, &ico).unwrap();
    let groups = resources(&pe, RT_GROUP_ICON).unwrap();
    assert_eq!(groups.len(), groups_before.len().max(1), "{groups:?}");
    let (name, language) = &groups[0];
    assert_eq!(*name, ResourceName::Id(1));
    let entries = group_entries(&resource(&pe, RT_GROUP_ICON, name, *language).unwrap()).unwrap();
    let ids: Vec<u16> = entries.iter().map(|(_, id)| *id).collect();
    assert_eq!(ids, [1, 2]);
    assert_eq!(entries[1].0.width, 24);
    let bitmap = resource(&pe, RT_ICON, &ResourceName::Id(2), *language).unwrap();
    assert_eq!(bitmap, image(24, 4).bitmap);
}

/// An icon file that is not there: the system's failure, the module left as it was.
#[test]
fn an_icon_file_not_there_fails() {
    let temp = tempfile::tempdir().unwrap();
    let pe = module_copy(temp.path());
    let before = std::fs::read(&pe).unwrap();
    assert!(replace_icon(&pe, &temp.path().join("none.ico")).is_err());
    assert_eq!(std::fs::read(&pe).unwrap(), before);
}
