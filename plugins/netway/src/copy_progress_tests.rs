use super::*;

/// NetBox's bar, WinSCP's bytes and spans, names cut from their first folders.
#[test]
fn the_window_parts_are_netboxs() {
    assert_eq!(bar(50).chars().count(), WIDTH);
    assert!(bar(100).ends_with(" 100%"));
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(200 * 1024), "200 KB");
    assert_eq!(span(3725), "1:02:05");
    assert_eq!(minimize("/a/b/c/file.txt", 40, true), "/a/b/c/file.txt");
    assert_eq!(
        minimize("/aaaa/bbbb/cccc/file.txt", 18, true),
        "/.../cccc/file.txt"
    );
    assert_eq!(percent(1, 0), 0);
}

/// The total known: the time left and the bytes' bar; not calculated: the start time and the files'.
#[test]
fn the_whole_goes_by_bytes_or_by_files() {
    let shown = Shown {
        moving: false,
        temporary: false,
        remote_source: true,
        operation: None,
    };
    let progress = crate::copy_work::Snapshot {
        total: 100,
        done: 50,
        files: 4,
        files_done: 1,
        total_known: true,
        started: "10:20:30".to_string(),
        ..Default::default()
    };
    let (_, lines) = window(shown, &progress, Duration::from_secs(1));
    assert!(lines[4].contains("Left:"), "{lines:?}");
    assert_eq!(lines.last(), Some(&bar(50)));
    let uncounted = crate::copy_work::Snapshot {
        total_known: false,
        ..progress
    };
    let (_, lines) = window(shown, &uncounted, Duration::from_secs(1));
    assert!(lines[4].contains("Start time:") && lines[4].ends_with("10:20:30"));
    assert_eq!(lines.last(), Some(&bar(25)));
    let deleting = Shown {
        operation: Some("Deleting"),
        ..shown
    };
    let (title, lines) = window(deleting, &uncounted, Duration::from_secs(1));
    assert_eq!((title.as_str(), lines.len()), ("Deleting", 2), "{lines:?}");
    assert_eq!(lines[1], bar(25));
}
