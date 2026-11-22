use super::*;

#[test]
fn mlsd_facts_are_read() {
    let file = mlsd_entry(
        "type=file;size=1234;modify=20240310103045;UNIX.mode=0644;UNIX.ownername=user;UNIX.groupname=users; a b.txt",
    )
    .unwrap();
    assert_eq!(file.name, "a b.txt");
    assert_eq!(file.size, 1234);
    let utc = seconds_of((2024, 3, 10), (10, 30, 45));
    assert_eq!(file.modified, Some(u32::try_from(utc).unwrap()));
    assert_eq!(file.permissions, Some(0o100_644));
    assert_eq!(file.owner.as_deref(), Some("user"));
    assert_eq!(file.group.as_deref(), Some("users"));
    let folder = mlsd_entry("Type=dir;Modify=20240310103045.123;perm=el; docs").unwrap();
    assert!(folder.is_dir);
    assert_eq!(
        folder.permissions,
        Some(0o040_555),
        "no write right: read-only, a folder searchable"
    );
    let link = mlsd_entry("type=OS.unix=symlink;size=4; www").unwrap();
    assert!(link.is_link);
}

#[test]
fn the_folder_its_parent_and_nonsense_are_left_out() {
    let lines: Vec<String> = [
        "type=cdir;modify=20240310103045; /srv",
        "type=pdir;modify=20240310103045; ..",
        "garbage",
        "type=file;size=1; ok.txt",
    ]
    .iter()
    .map(|line| line.to_string())
    .collect();
    let read = mlsd(&lines);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].name, "ok.txt");
    assert_eq!(modify_of("2024"), None);
}

#[test]
fn list_lines_read_as_ls() {
    let lines = vec![
        "drwxr-xr-x 2 ftp ftp 4096 Nov  1 09:15 docs".to_string(),
        "-rw-r--r-- 1 ftp ftp 3 Nov  1 09:15 a.txt".to_string(),
    ];
    let read = list(&lines, (2026, 11, 18));
    assert_eq!(read.len(), 2);
    assert!(read[0].entry.is_dir);
    assert_eq!(read[1].entry.size, 3);
}
