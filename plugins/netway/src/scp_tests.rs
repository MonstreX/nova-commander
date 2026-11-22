use super::*;

#[test]
fn days_turn_back_into_dates() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    let days = crate::scp_listing::seconds_of((2026, 11, 18), (0, 0, 0)) / 86_400;
    assert_eq!(civil_from_days(days), (2026, 11, 18));
}

#[test]
fn times_are_touched_in_utc() {
    let seconds = crate::scp_listing::seconds_of((2024, 3, 10), (10, 30, 45));
    assert_eq!(
        touch_time(u32::try_from(seconds).unwrap()),
        "202403101030.45"
    );
}

#[test]
fn attributes_become_winscps_commands() {
    let attributes = FileAttributes {
        permissions: Some(0o100_755),
        uid: Some(1000),
        group: Some("staff".to_string()),
        mtime: Some(0),
        ..FileAttributes::empty()
    };
    let commands = attribute_commands("/a b/c", &attributes);
    assert_eq!(
        commands,
        [
            "chmod 0755 \"/a b/c\"",
            "chown \"1000\" \"/a b/c\"",
            "chgrp \"staff\" \"/a b/c\"",
            "TZ=UTC touch -m -t 197001010000.00 \"/a b/c\"",
        ]
    );
}

#[test]
fn records_carry_rights_size_and_name_or_the_servers_failure() {
    assert_eq!(file_record(0o100_644, 12, "a.txt"), "C0644 12 a.txt\n");
    assert_eq!(record_size("C0644 12 a.txt"), Ok(12));
    assert_eq!(
        record_size("\u{1}scp: /x: No such file or directory"),
        Err("scp: /x: No such file or directory".to_string())
    );
    assert_eq!(record_size(""), Err(EMPTY_LINE.to_string()));
    assert!(record_size("Z1").is_err());
}

#[test]
fn an_entry_becomes_attributes() {
    let entry = RemoteEntry {
        size: 4,
        is_dir: true,
        modified: Some(7),
        permissions: Some(0o120_755),
        owner: Some("u".to_string()),
        ..RemoteEntry::default()
    };
    let read = attributes(&entry);
    assert_eq!(read.permissions, Some(0o040_755));
    assert_eq!(read.mtime, Some(7));
    assert_eq!(read.user.as_deref(), Some("u"));
    assert!(read.is_dir());
}
