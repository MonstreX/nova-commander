use super::*;

use std::time::{Duration, UNIX_EPOCH};

fn state(plugin: Option<&str>, reading: bool) -> PanelState {
    PanelState {
        plugin: plugin.map(str::to_string),
        reading,
        ..PanelState::default()
    }
}

#[test]
fn only_a_disks_panel_of_files_is_regular() {
    assert!(regular(Some(&state(None, false))));
    assert!(!regular(Some(&state(Some("netway"), false))));
    assert!(!regular(None));
}

/// NetBox's: deeper into the same subfolders, up as many levels, elsewhere not followed.
#[test]
fn the_local_folder_follows_the_servers() {
    assert_eq!(
        local_path("/home", "/home/docs/deep", r"C:\work").as_deref(),
        Some(r"C:\work\docs\deep")
    );
    assert_eq!(
        local_path("/home/docs/deep", "/home", r"C:\work\docs\deep\").as_deref(),
        Some(r"C:\work")
    );
    assert_eq!(
        local_path("/", "/srv", r"C:\work").as_deref(),
        Some(r"C:\work\srv")
    );
    assert_eq!(
        local_path("/srv", "/", r"C:\work").as_deref(),
        Some(r"C:\"),
        "up to the drive's root"
    );
    assert_eq!(local_path("/a", "/b", r"C:\work"), None, "elsewhere");
    assert_eq!(local_path("/a/b/c", "/", r"C:\"), None, "above the disk");
}

fn local(name: &str, size: u64, seconds: u64) -> PanelItem {
    PanelItem {
        name: name.to_string(),
        size,
        modified: Some(UNIX_EPOCH + Duration::from_secs(seconds)),
        ..PanelItem::default()
    }
}

fn remote(name: &str, size: u64, seconds: u32) -> RemoteEntry {
    RemoteEntry {
        name: name.to_string(),
        size,
        modified: Some(seconds),
        ..RemoteEntry::default()
    }
}

/// Only on one side, another time; another size only when asked; folders and case left out.
#[test]
fn the_files_that_differ_are_found() {
    let mut folder = local("dir", 0, 1);
    folder.folder = true;
    let locals = [
        local("same.txt", 5, 100),
        local("NEWER.txt", 5, 200),
        local("only-here.txt", 1, 1),
        local("bigger.txt", 9, 300),
        folder,
    ];
    let remotes = [
        remote("same.txt", 5, 100),
        remote("newer.txt", 5, 150),
        remote("only-there.txt", 1, 1),
        remote("bigger.txt", 3, 300),
    ];
    let (left, right) = differences(&locals, &remotes, false);
    assert_eq!(left, ["NEWER.txt", "only-here.txt"]);
    assert_eq!(right, ["newer.txt", "only-there.txt"]);
    let (left, right) = differences(&locals, &remotes, true);
    assert!(left.contains(&"bigger.txt".to_string()) && right.contains(&"bigger.txt".to_string()));
    assert_eq!(
        compared(2, 3),
        "Differences found: 2 local files, 3 remote files"
    );
}

/// A server's time to the minute (or the day): a disk's file of the same minute is no difference, one of
/// the next minute is; a name matched without case beyond ASCII.
#[test]
fn coarse_server_times_differ_only_past_their_unit() {
    use crate::sftp::Precision;
    let at_minute = 1_700_000_040;
    let coarse = |name: &str, precision| RemoteEntry {
        precision,
        ..remote(name, 5, at_minute)
    };
    let locals = [
        local("same-minute.txt", 5, u64::from(at_minute) + 59),
        local("next-minute.txt", 5, u64::from(at_minute) + 60),
        local("same-day.txt", 5, u64::from(at_minute) + 3_600),
        local("Äpfel.txt", 5, u64::from(at_minute)),
    ];
    let remotes = [
        coarse("same-minute.txt", Precision::Minute),
        coarse("next-minute.txt", Precision::Minute),
        RemoteEntry {
            modified: Some(1_699_920_000),
            ..coarse("same-day.txt", Precision::Day)
        },
        coarse("äpfel.txt", Precision::Second),
    ];
    let (left, right) = differences(&locals, &remotes, true);
    assert_eq!(left, ["next-minute.txt"]);
    assert_eq!(right, ["next-minute.txt"]);
    let exact = [remote("same-minute.txt", 5, at_minute)];
    let (left, _) = differences(&locals[..1], &exact, false);
    assert_eq!(left, ["same-minute.txt"], "to the second");
}
