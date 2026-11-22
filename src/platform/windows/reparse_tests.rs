use super::*;
use crate::platform::files::{NewLink, link_target, make_link};

fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// A reparse buffer: the tag, the data's length, a reserved word, the data.
fn buffer(tag: u32, data: &[u8]) -> Vec<u8> {
    let mut buffer = tag.to_le_bytes().to_vec();
    buffer.extend(u16::try_from(data.len()).unwrap().to_le_bytes());
    buffer.extend([0, 0]);
    buffer.extend(data);
    buffer
}

#[test]
fn an_app_alias_shows_the_program_it_starts() {
    let mut data = 3u32.to_le_bytes().to_vec();
    data.extend(wide(
        "Microsoft.WindowsTerminal_8wekyb3d8bbwe\0App\0C:\\Program Files\\WindowsApps\\wt.exe\0\0",
    ));
    let text = parse(&buffer(TAG_APPEXECLINK, &data)).unwrap();
    assert_eq!(text.target, r"C:\Program Files\WindowsApps\wt.exe");
    assert!(
        !text.editable,
        "FAR edits only symbolic links and junctions"
    );
    let short = [3u32.to_le_bytes().to_vec(), wide("one\0two\0")].concat();
    assert_eq!(parse(&buffer(TAG_APPEXECLINK, &short)), None);
}

#[test]
fn a_wsl_link_shows_its_target_in_utf8() {
    let data = [
        2u32.to_le_bytes().to_vec(),
        "../каталог/file".as_bytes().to_vec(),
    ]
    .concat();
    let text = parse(&buffer(TAG_LX_SYMLINK, &data)).unwrap();
    assert_eq!(text.target, "../каталог/file");
    assert!(!text.editable);
}

#[test]
fn an_nfs_link_shows_its_target_and_another_nfs_file_nothing() {
    let data = [NFS_LINK.to_le_bytes().to_vec(), wide("/export/x")].concat();
    assert_eq!(parse(&buffer(TAG_NFS, &data)).unwrap().target, "/export/x");
    let fifo = [0x4F46_4946u64.to_le_bytes().to_vec(), wide("x")].concat();
    assert_eq!(parse(&buffer(TAG_NFS, &fifo)), None);
}

#[test]
fn a_containers_link_shows_its_name() {
    let mut data = vec![0u8; 24];
    data.extend(4u16.to_le_bytes());
    data.extend(wide("ab"));
    assert_eq!(parse(&buffer(TAG_WCI[2], &data)).unwrap().target, "ab");
}

#[test]
fn a_symbolic_link_shows_its_print_name_or_else_its_substitute_name() {
    let built = symlink_buffer(r"C:\Work\x").unwrap();
    let text = parse(&built).unwrap();
    assert_eq!(text.target, r"C:\Work\x");
    assert!(text.editable);
    // No print name: the substitute name, `\??\` taken off.
    let substitute = wide(r"\??\D:\y");
    let mut data = Vec::new();
    for value in [0, substitute.len(), substitute.len(), 0] {
        data.extend(u16::try_from(value).unwrap().to_le_bytes());
    }
    data.extend(0u32.to_le_bytes());
    data.extend(&substitute);
    assert_eq!(parse(&buffer(TAG_SYMLINK, &data)).unwrap().target, r"D:\y");
}

#[test]
fn a_relative_symbolic_link_is_relative_and_an_absolute_one_keeps_its_kernel_name() {
    let relative = symlink_buffer(r"..\x").unwrap();
    assert_eq!(&relative[16..20], &RELATIVE.to_le_bytes());
    // As Windows lays it: the print name, then the substitute name, no NULs.
    assert_eq!(&relative[20..], &wide(r"..\x..\x")[..]);
    let absolute = symlink_buffer(r"C:\x").unwrap();
    assert_eq!(&absolute[16..20], &0u32.to_le_bytes());
    assert_eq!(&absolute[20..], &wide(r"C:\x\??\C:\x")[..]);
    let words: Vec<u16> = (8..16)
        .step_by(2)
        .map(|at| u16::from_le_bytes([absolute[at], absolute[at + 1]]))
        .collect();
    assert_eq!(
        words,
        [8, 16, 0, 8],
        "substitute at 8 (16 bytes), print at 0 (8 bytes)"
    );
    assert_eq!(symlink_buffer(&"x".repeat(9000)), None);
}

#[test]
fn unknown_and_broken_buffers_say_nothing() {
    assert_eq!(parse(&buffer(0x9000_001A, &[1, 2, 3])), None);
    assert_eq!(parse(&[1, 2]), None);
    assert_eq!(parse(&buffer(TAG_SYMLINK, &[0; 6])), None);
}

#[test]
fn a_junction_leads_elsewhere_and_stays_a_junction() {
    let temp = tempfile::tempdir().unwrap();
    let (first, second) = (temp.path().join("first"), temp.path().join("second"));
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let link = temp.path().join("link");
    make_link(NewLink::Junction, &first, &link).unwrap();
    assert!(reparse_text(&link).unwrap().editable);

    // A relative target is counted from the link's folder.
    modify_link(&link, "second").unwrap();

    assert_eq!(link_target(&link).unwrap(), second);
    let text = reparse_text(&link).unwrap();
    assert_eq!(
        (text.tag, text.target),
        (TAG_MOUNT_POINT, second.display().to_string())
    );
}

/// A symbolic link needs Developer Mode or administrator rights, as the other tests of links.
#[test]
fn a_symbolic_link_gets_the_new_target_as_typed() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::write(temp.path().join("b.txt"), "b").unwrap();
    let link = temp.path().join("link.txt");
    std::os::windows::fs::symlink_file("a.txt", &link).unwrap();

    match modify_link(&link, "b.txt") {
        Ok(()) => {
            assert_eq!(link_target(&link).unwrap(), Path::new("b.txt"));
            assert_eq!(std::fs::read_to_string(&link).unwrap(), "b");
        }
        // Without the privilege (Developer Mode lets links be made, not their buffers set): the
        // link is as it was, and the error says why (the helper's case, ADR-0007).
        Err(error) => {
            assert!(crate::platform::elevation::needs_rights(&error), "{error}");
            assert_eq!(link_target(&link).unwrap(), Path::new("a.txt"));
        }
    }
}

#[test]
fn a_plain_folder_or_another_reparse_point_is_not_changed() {
    let temp = tempfile::tempdir().unwrap();
    assert!(modify_link(temp.path(), "x").is_err());
    assert_eq!(reparse_text(temp.path()), None);
}

#[test]
fn a_mount_points_volume_keeps_its_kernel_name_without_a_print_name() {
    let substitute = wide(r"\??\Volume{5f2c0a1e-0000-0000-0000-100000000000}\");
    let mut data = Vec::new();
    for value in [0, substitute.len(), substitute.len() + 2, 0] {
        data.extend(u16::try_from(value).unwrap().to_le_bytes());
    }
    data.extend(&substitute);
    data.extend([0, 0, 0, 0]);
    let text = parse(&buffer(TAG_MOUNT_POINT, &data)).unwrap();
    assert_eq!(
        text.target, r"\??\Volume{5f2c0a1e-0000-0000-0000-100000000000}\",
        "FAR's `NormalizeSymlinkName` leaves a volume's name as it is"
    );
}
