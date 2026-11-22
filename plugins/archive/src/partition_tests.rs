//! A disk image of two FAT12 partitions behind an MBR ([`crate::support::mbr_disk`]); its partitions
//! named and opened.

use super::*;

/// arclite's boot sector and superblock checks.
#[test]
fn file_systems_are_guessed_from_their_start() {
    let mut start = vec![0u8; 2048];
    assert_eq!(guess_fs_ext(&start), None);
    assert_eq!(guess_fs_ext(&start[..100]), None, "too short");
    start[510] = 0x55;
    start[511] = 0xAA;
    start[3..11].copy_from_slice(b"NTFS    ");
    assert_eq!(guess_fs_ext(&start), Some(".ntfs"));
    start[3..11].copy_from_slice(b"EXFAT   ");
    assert_eq!(guess_fs_ext(&start), Some(".xfat"));
    start[3..11].copy_from_slice(b"MSDOS5.0");
    start[0x52..0x5A].copy_from_slice(b"FAT32   ");
    assert_eq!(guess_fs_ext(&start), Some(".fat"));
    let mut ext = vec![0u8; 2048];
    ext[0x438] = 0x53;
    ext[0x439] = 0xEF;
    assert_eq!(guess_fs_ext(&ext), Some(".ext2"));
    ext[0x44C] = 1;
    ext[0x45C] = 0x04;
    assert_eq!(guess_fs_ext(&ext), Some(".ext3"));
    ext[0x44C] = 2;
    assert_eq!(guess_fs_ext(&ext), Some(".ext"));
    ext[5] = 1;
    assert_eq!(guess_fs_ext(&ext), None, "the first KB not zeros");
}

fn item(name: &str, size: u64, packed: u64) -> Item {
    Item {
        path: vec![name.to_string()],
        size,
        packed,
        ..Item::default()
    }
}

/// A partition renamed by its index, the kind and the file system guessed, else its name's ending; one
/// with no extension, no offset, a folder, packed otherwise or renamed already kept.
#[test]
fn roots_are_named_as_arclite_names_them() {
    let fat = || Some(".fat");
    let none = || None;
    let part = item("0.fat", 10, 10);
    assert_eq!(
        root_name((0, &part, Some(512)), 'm', fat).as_deref(),
        Some("0.mbr.fat")
    );
    let apfs = item("1.apfs", 10, 10);
    assert_eq!(
        root_name((1, &apfs, Some(512)), 'g', none).as_deref(),
        Some("1.gpt.apfs")
    );
    let ext4 = item("2.ext4", 10, 10);
    assert_eq!(
        root_name((2, &ext4, Some(0)), 'm', none).as_deref(),
        Some("2.mbr.ext")
    );
    assert_eq!(root_name((0, &item("0", 10, 10), Some(0)), 'm', fat), None);
    assert_eq!(root_name((0, &part, None), 'm', fat), None);
    assert_eq!(
        root_name((0, &item("0.fat", 10, 9), Some(0)), 'm', fat),
        None
    );
    assert_eq!(
        root_name((3, &item("3.img", 10, 10), Some(0)), 'm', none),
        None
    );
    assert_eq!(
        root_name((0, &item("0.fat", 0, 0), Some(0)), 'm', fat),
        None,
        "empty"
    );
    assert!(is_partition(&part) && !is_partition(&item("0.fat", 0, 0)));
    let done = item("0.mbr.fat", 10, 10);
    assert_eq!(root_name((0, &done, Some(0)), 'm', fat), None);
    let folder = Item {
        is_dir: true,
        ..part.clone()
    };
    assert_eq!(root_name((0, &folder, Some(0)), 'm', fat), None);
}

/// FAT16 and FAT12 by their names at 0x36; a boot sector's names only with its 0x55 0xAA at its end; an
/// ext of revision 1 without a journal is ext2.
#[test]
fn boot_sectors_need_their_signature() {
    let mut start = vec![0u8; 2048];
    start[510] = 0x55;
    start[511] = 0xAA;
    start[0x36..0x3E].copy_from_slice(b"FAT16   ");
    assert_eq!(guess_fs_ext(&start), Some(".fat"));
    start[0x36..0x3E].copy_from_slice(b"FAT12   ");
    assert_eq!(guess_fs_ext(&start), Some(".fat"));
    start[511] = 0;
    assert_eq!(guess_fs_ext(&start), None, "0x55 alone");
    start[510] = 0;
    start[511] = 0xAA;
    assert_eq!(guess_fs_ext(&start), None, "0xAA alone");
    let mut ext = vec![0u8; 2048];
    ext[0x438] = 0x53;
    ext[0x439] = 0xEF;
    ext[0x44C] = 1;
    assert_eq!(guess_fs_ext(&ext), Some(".ext2"), "no journal");
    ext[0x439] = 0;
    assert_eq!(guess_fs_ext(&ext), None, "half the magic");
    ext[0x438] = 0;
    ext[0x439] = 0xEF;
    assert_eq!(guess_fs_ext(&ext), None, "the other half");
    let mut short = vec![0u8; 2047];
    short[510] = 0x55;
    short[511] = 0xAA;
    short[3..11].copy_from_slice(b"NTFS    ");
    assert_eq!(guess_fs_ext(&short), None, "less than 2 KB");
}

/// The name's ending when nothing is guessed: HFS and HFSX as `.hfs`; what is guessed goes before the
/// name; a name renamed for the other kind (or not as arclite renames) is renamed again.
#[test]
fn names_fall_back_on_their_ending() {
    let none = || None;
    let fat = || Some(".fat");
    let root = |name: &str, mode: char, guessed: &dyn Fn() -> Option<&'static str>| {
        root_name((4, &item(name, 10, 10), Some(0)), mode, guessed)
    };
    assert_eq!(root("4.hfs", 'g', &none).as_deref(), Some("4.gpt.hfs"));
    assert_eq!(root("4.hfsx", 'g', &none).as_deref(), Some("4.gpt.hfs"));
    assert_eq!(root("4.apfs", 'm', &fat).as_deref(), Some("4.mbr.fat"));
    assert_eq!(root("4.gpt.fat", 'g', &fat), None, "renamed already");
    assert_eq!(root("4.gpt.fat", 'm', &fat).as_deref(), Some("4.mbr.fat"));
    assert_eq!(root("4.mbrx.fat", 'm', &fat).as_deref(), Some("4.mbr.fat"));
    assert_eq!(root("4.m.fat", 'm', &fat).as_deref(), Some("4.mbr.fat"));
    let deep = Item {
        path: vec!["d".to_string(), "4.fat".to_string()],
        size: 10,
        packed: 10,
        ..Item::default()
    };
    assert!(!is_partition(&deep), "not at the root");
}

/// Each file system's name ending opens by its format among the installed library's; the disks' kinds
/// by their formats.
#[test]
fn partitions_open_by_their_file_systems_format() {
    let library = crate::test_archives::library();
    let formats = library.formats();
    let name_of = |name: &str| format_of(name, &formats).map(|at| formats[at].name.clone());
    assert_eq!(name_of("0.mbr.fat").as_deref(), Some("FAT"));
    assert_eq!(name_of("0.mbr.ntfs").as_deref(), Some("NTFS"));
    assert_eq!(name_of("0.gpt.apfs").as_deref(), Some("APFS"));
    assert_eq!(name_of("0.gpt.hfs").as_deref(), Some("HFS"));
    for ext in ["ext2", "ext3", "ext4", "ext"] {
        assert_eq!(name_of(&format!("1.gpt.{ext}")).as_deref(), Some("Ext"));
    }
    assert_eq!(name_of("0.mbr.img"), None);
    let mode_of = |name: &str| {
        let format = formats.iter().find(|format| format.name == name).unwrap();
        part_mode(format)
    };
    assert_eq!(mode_of("MBR"), Some('m'));
    assert_eq!(mode_of("GPT"), Some('g'));
    assert_eq!(mode_of("7z"), None);
}

/// A disk image of 7-Zip's MBR: its partition named `0.mbr.fat` by its boot sector, opened as FAT from
/// its stream inside the disk's archive (`MBR→FAT`), its file read; a name of no file system opens none.
#[test]
fn a_partition_is_named_and_opened() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("disk.mbr");
    crate::support::mbr_disk(&path).unwrap();
    let opened = crate::test_archives::opened(&path);
    assert_eq!(opened.format, "MBR");
    let names: Vec<String> = opened
        .items
        .iter()
        .map(|item| item.path.join("/"))
        .collect();
    assert_eq!(names, ["0.mbr.fat", "1.mbr.fat"]);
    let library = Library::find(temp.path());
    let formats = library.formats();
    let (chain, _) = opened.library().unwrap();
    let mode = chain
        .format()
        .and_then(|format| part_mode(&formats[format]));
    assert_eq!(mode, Some('m'));
    let mut asker = crate::test_archives::NoAsk(None);
    let inner = open(&library, chain, (0, "0.mbr.fat"), &mut asker)
        .unwrap()
        .unwrap();
    assert_eq!(inner.name(&formats), "MBR→FAT");
    assert_eq!(inner.parts[1].offset, 512);
    let archive = inner.archive();
    assert_eq!(archive.count(), 1);
    assert_eq!(archive.property(0, prop::PATH).text(), Some("HELLO.TXT"));
    assert!(
        open(&library, chain, (0, "0.mbr.img"), &mut asker)
            .unwrap()
            .is_none()
    );
}
