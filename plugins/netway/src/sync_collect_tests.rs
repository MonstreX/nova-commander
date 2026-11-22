use super::*;

fn file(name: &str, size: u64, modified: u64) -> Node {
    Node {
        name: name.to_string(),
        size,
        modified: Some(modified),
        ..Node::default()
    }
}

fn folder(name: &str, children: Vec<Node>) -> Node {
    Node {
        name: name.to_string(),
        is_dir: true,
        children,
        ..Node::default()
    }
}

/// A server's time to the minute (`ls -l`'s "hh:mm") or the day: the disk's time cut to it first, so a
/// file of the same minute is no difference (WinSCP's `ReduceDateTimePrecision`); a later minute is.
#[test]
fn a_coarse_server_time_is_compared_at_its_precision() {
    use crate::sftp::Precision;
    let at_minute = 1_700_000_040; // a whole minute
    let local = vec![
        file("a.txt", 1, at_minute + 27),
        file("b.txt", 1, at_minute + 75),
    ];
    let coarse = |name: &str, precision| Node {
        precision,
        ..file(name, 1, at_minute)
    };
    let remote = vec![
        coarse("a.txt", Precision::Minute),
        coarse("b.txt", Precision::Minute),
    ];
    let items = collect(&local, &remote, &Settings::default(), "");
    assert_eq!(
        actions(&items),
        [("b.txt".to_string(), Action::UploadUpdate, true)]
    );
    let midnight = 1_699_920_000; // a whole day
    let local = vec![file("c.txt", 1, midnight + 3_600)];
    let remote = vec![Node {
        precision: Precision::Day,
        ..file("c.txt", 1, midnight)
    }];
    assert!(collect(&local, &remote, &Settings::default(), "").is_empty());
    let exact = vec![file("a.txt", 1, at_minute)];
    let local = vec![file("a.txt", 1, at_minute + 27)];
    assert_eq!(
        collect(&local, &exact, &Settings::default(), "").len(),
        1,
        "to the second"
    );
}

/// Names matched without case for any letter, not only ASCII ones.
#[test]
fn names_match_without_case_beyond_ascii() {
    assert!(same_name("Äpfel.TXT", "äpfel.txt"));
    assert!(same_name("README", "readme"));
    // Both ways the same, whichever side has the letter beyond ASCII (the Kelvin sign lowers to "k").
    assert!(same_name("k.txt", "\u{212A}.txt"));
    assert!(same_name("\u{212A}.txt", "k.txt"));
    assert!(!same_name("Äpfel", "Apfel"));
    let local = vec![file("Äpfel.txt", 1, 100)];
    let remote = vec![file("äpfel.txt", 1, 100)];
    let deleting = Settings {
        mode: Mode::Remote,
        delete: true,
        ..Settings::default()
    };
    assert!(
        collect(&local, &remote, &deleting, "").is_empty(),
        "one file"
    );
}

fn actions(items: &[Item]) -> Vec<(String, Action, bool)> {
    (items.iter())
        .map(|item| (item.path.clone(), item.action, item.checked))
        .collect()
}

fn trees() -> (Vec<Node>, Vec<Node>) {
    let local = vec![
        file("same.txt", 1, 100),
        file("newer-here.txt", 1, 300),
        file("older-here.txt", 1, 100),
        file("only-here.txt", 1, 1),
        folder(
            "sub",
            vec![file("inner.txt", 1, 500), file("Mixed.TXT", 2, 9)],
        ),
        folder("local-dir", vec![]),
    ];
    let remote = vec![
        file("same.txt", 1, 100),
        file("newer-here.txt", 1, 200),
        file("older-here.txt", 1, 200),
        file("only-there.txt", 1, 1),
        folder(
            "sub",
            vec![file("inner.txt", 1, 400), file("mixed.txt", 2, 9)],
        ),
    ];
    (local, remote)
}

/// WinSCP's "Synchronize files" to the server: the newer local files and the new ones up; the server's
/// newer file left (no download in this mode); a remote-only file's deletion listed unchecked unless
/// deleting (WinSCP's `Checked = spDelete`), not listed at all for timestamps.
#[test]
fn files_go_to_the_remote_side() {
    let (local, remote) = trees();
    let items = collect(&local, &remote, &Settings::default(), "");
    assert_eq!(
        actions(&items),
        [
            ("local-dir".to_string(), Action::UploadNew, true),
            ("newer-here.txt".to_string(), Action::UploadUpdate, true),
            ("only-here.txt".to_string(), Action::UploadNew, true),
            ("only-there.txt".to_string(), Action::DeleteRemote, false),
            ("sub/inner.txt".to_string(), Action::UploadUpdate, true),
        ]
    );
    let stamping = Settings {
        timestamps: true,
        ..Settings::default()
    };
    let items = collect(&local, &remote, &stamping, "");
    assert!(
        !items.iter().any(|item| item.action == Action::DeleteRemote),
        "timestamps delete nothing"
    );
    let deleting = Settings {
        delete: true,
        ..Settings::default()
    };
    let items = collect(&local, &remote, &deleting, "");
    assert!(items.contains(&Item {
        path: "only-there.txt".to_string(),
        is_dir: false,
        action: Action::DeleteRemote,
        checked: true,
        local: None,
        remote: Some((1, Some(1))),
    }));
}

/// Both ways each side's newer file goes to the other; "Existing files only" leaves the new ones
/// unchecked.
#[test]
fn both_ways_take_the_newer() {
    let (local, remote) = trees();
    let both = Settings {
        mode: Mode::Both,
        existing_only: true,
        ..Settings::default()
    };
    let items = collect(&local, &remote, &both, "");
    let found = actions(&items);
    assert!(found.contains(&("older-here.txt".to_string(), Action::DownloadUpdate, true)));
    assert!(found.contains(&("newer-here.txt".to_string(), Action::UploadUpdate, true)));
    assert!(found.contains(&("only-there.txt".to_string(), Action::DownloadNew, false)));
    assert!(found.contains(&("only-here.txt".to_string(), Action::UploadNew, false)));
}

/// "Mirror files" to the disk: any other time comes down, the older remote file too; deleting removes the
/// local-only files.
#[test]
fn mirroring_takes_any_other_time() {
    let (local, remote) = trees();
    let mirror = Settings {
        mode: Mode::Local,
        mirror: true,
        delete: true,
        ..Settings::default()
    };
    let found = actions(&collect(&local, &remote, &mirror, ""));
    assert!(found.contains(&("newer-here.txt".to_string(), Action::DownloadUpdate, true)));
    assert!(found.contains(&("older-here.txt".to_string(), Action::DownloadUpdate, true)));
    assert!(found.contains(&("only-there.txt".to_string(), Action::DownloadNew, true)));
    assert!(found.contains(&("only-here.txt".to_string(), Action::DeleteLocal, true)));
    assert!(found.contains(&("local-dir".to_string(), Action::DeleteLocal, true)));
}

/// To the disk: a local-only item's deletion listed unchecked unless deleting, never for timestamps; the
/// server's newer file comes down checked, "Existing files only" or not.
#[test]
fn local_deletions_are_checked_only_when_deleting() {
    let (local, remote) = trees();
    let to_disk = Settings {
        mode: Mode::Local,
        ..Settings::default()
    };
    let found = actions(&collect(&local, &remote, &to_disk, ""));
    assert!(found.contains(&("only-here.txt".to_string(), Action::DeleteLocal, false)));
    assert!(found.contains(&("local-dir".to_string(), Action::DeleteLocal, false)));
    let deleting = Settings {
        delete: true,
        ..to_disk
    };
    let found = actions(&collect(&local, &remote, &deleting, ""));
    assert!(found.contains(&("only-here.txt".to_string(), Action::DeleteLocal, true)));
    let existing = Settings {
        existing_only: true,
        ..deleting
    };
    let found = actions(&collect(&local, &remote, &existing, ""));
    assert!(found.contains(&("older-here.txt".to_string(), Action::DownloadUpdate, true)));
    assert!(found.contains(&("only-there.txt".to_string(), Action::DownloadNew, false)));
    assert!(found.contains(&("only-here.txt".to_string(), Action::DeleteLocal, true)));
    let stamping = Settings {
        timestamps: true,
        ..deleting
    };
    let found = actions(&collect(&local, &remote, &stamping, ""));
    assert!(
        !found
            .iter()
            .any(|(_, action, _)| *action == Action::DeleteLocal),
        "timestamps delete nothing: {found:?}"
    );
}

/// A server's time to the minute against a disk's in the next minute: the disk's is newer, its seconds
/// cut first (the minute's last second still the same minute); to the day likewise.
#[test]
fn a_coarse_time_cuts_only_within_its_unit() {
    use crate::sftp::Precision;
    assert_eq!(Precision::Second.reduced(1_700_000_099), 1_700_000_099);
    assert_eq!(Precision::Minute.reduced(1_700_000_099), 1_700_000_040);
    assert_eq!(Precision::Minute.reduced(1_700_000_100), 1_700_000_100);
    assert_eq!(
        Precision::Day.reduced(1_699_920_000 + 86_399),
        1_699_920_000
    );
    assert_eq!(
        Precision::Day.reduced(1_699_920_000 + 86_400),
        1_700_006_400
    );
    let at_minute = 1_700_000_040;
    let coarse = Node {
        precision: Precision::Minute,
        ..file("a.txt", 1, at_minute)
    };
    let same = vec![file("a.txt", 1, at_minute + 59)];
    let remote = std::slice::from_ref(&coarse);
    assert!(collect(&same, remote, &Settings::default(), "").is_empty());
    let next = vec![file("a.txt", 1, at_minute + 60)];
    assert_eq!(
        actions(&collect(&next, remote, &Settings::default(), "")),
        [("a.txt".to_string(), Action::UploadUpdate, true)]
    );
    let earlier = vec![file("a.txt", 1, at_minute - 1)];
    let both = Settings {
        mode: Mode::Both,
        ..Settings::default()
    };
    assert_eq!(
        actions(&collect(&earlier, remote, &both, "")),
        [("a.txt".to_string(), Action::DownloadUpdate, true)],
        "a second before the minute is the minute before"
    );
}

/// "Synchronize timestamps": existing files of another time only (new ones never), "Same size only" when
/// by size; "File size" alone finds a size change.
#[test]
fn timestamps_and_sizes() {
    let local = vec![
        file("a.txt", 1, 300),
        file("b.txt", 5, 300),
        file("new.txt", 1, 1),
    ];
    let remote = vec![file("a.txt", 1, 100), file("b.txt", 9, 100)];
    let stamps = Settings {
        timestamps: true,
        by_size: true,
        ..Settings::default()
    };
    let found = actions(&collect(&local, &remote, &stamps, ""));
    assert_eq!(found, [("a.txt".to_string(), Action::UploadUpdate, true)]);
    let sizes = Settings {
        by_time: false,
        by_size: true,
        ..Settings::default()
    };
    let found = actions(&collect(&local, &remote, &sizes, ""));
    assert!(found.contains(&("b.txt".to_string(), Action::UploadUpdate, true)));
    assert!(!found.iter().any(|(path, ..)| path == "a.txt"));
}
