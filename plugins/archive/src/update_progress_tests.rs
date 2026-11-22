use std::path::Path;

use super::*;

/// arclite's lines: the archive, the file with its bytes and bar, all bytes at the speed with the time
/// left, read against written, the whole bar.
#[test]
fn the_window_is_arclites() {
    let at = UpdateAt {
        file: Path::new(r"C:\src\b.txt"),
        file_done: 256,
        file_total: 1024,
        done: 1024,
        total: 4096,
        written: 512,
    };
    let (title, lines) = text((false, r"C:\a.zip"), &at, Duration::from_secs(2));
    assert_eq!(title, "{25%} Updating archive...");
    assert_eq!(lines.len(), 9);
    assert_eq!(lines[0], r"C:\a.zip");
    assert_eq!((lines[1].as_str(), lines[5].as_str()), ("\u{1}", "\u{1}"));
    assert_eq!(lines[2], r"C:\src\b.txt");
    assert_eq!(lines[3], "    256 / 1 KB");
    assert_eq!(lines[6], "   1 KB / 4 KB @   512 B/s -00:00:06");
    assert_eq!(lines[7], "   1 KB \u{2192}     512 = 50%");
    assert_eq!(lines[4].chars().filter(|&c| c == '\u{2588}').count(), 15);
    assert_eq!(lines[8].chars().filter(|&c| c == '\u{2588}').count(), 15);
    let (created, _) = text((true, "a.zip"), &at, Duration::ZERO);
    assert_eq!(created, "{25%} Creating archive...");
}

/// Nothing read yet: no speed, no time left; an hour and more as hours.
#[test]
fn nothing_read_and_long_times() {
    let at = UpdateAt {
        file: Path::new("f"),
        file_done: 0,
        file_total: 0,
        done: 0,
        total: 10,
        written: 0,
    };
    let (_, lines) = text((false, "a"), &at, Duration::ZERO);
    assert!(lines[6].ends_with("@     0 B/s -00:00:00"), "{}", lines[6]);
    assert_eq!(time(3 * 3600 + 61), "03:01:01");
}

/// A file grown since the walk (more read than counted): no time left, the bars full, no failure.
#[test]
fn more_read_than_counted_fills_the_bars() {
    let at = UpdateAt {
        file: Path::new("grown"),
        file_done: 300,
        file_total: 100,
        done: 300,
        total: 100,
        written: 600,
    };
    let (title, lines) = text((false, "a"), &at, Duration::from_secs(3));
    assert_eq!(
        title, "{100%} Updating archive...",
        "arclite's percent at most 100"
    );
    assert!(lines[6].ends_with("@   100 B/s -00:00:00"), "{}", lines[6]);
    for bar in [&lines[4], &lines[8]] {
        assert_eq!(bar.chars().filter(|&c| c == '\u{2588}').count(), 60);
        assert_eq!(bar.chars().count(), 60);
    }
    assert!(lines[7].ends_with("= 100%"), "{}", lines[7]);
}

/// The time left: the whole time as the bytes so far took theirs, less the time worked.
#[test]
fn the_time_left_is_by_the_bytes_so_far() {
    let at = UpdateAt {
        file: Path::new("f"),
        file_done: 0,
        file_total: 0,
        done: 100,
        total: 1000,
        written: 0,
    };
    let (_, lines) = text((false, "a"), &at, Duration::from_secs(60));
    assert!(lines[6].ends_with("-00:09:00"), "{}", lines[6]);
    let half = UpdateAt {
        done: 50,
        total: 100,
        ..at
    };
    let (title, lines) = text((false, "a"), &half, Duration::from_millis(500));
    assert_eq!(title, "{50%} Updating archive...");
    assert!(
        lines[6].contains("@   100 B/s"),
        "by the milliseconds: {}",
        lines[6]
    );
}

fn deleting(done: u64, total: u64) -> UpdateAt<'static> {
    UpdateAt {
        file: Path::new("unused"),
        file_done: 7,
        file_total: 9,
        done,
        total,
        written: 3,
    }
}

fn filled(bar: &str) -> usize {
    bar.chars().filter(|&c| c == '\u{2588}').count()
}

/// arclite's `ArchiveFileDeleterProgress`: two lines - all the bytes written at the speed, the bar - and
/// the update's title; no archive, no file, no time left.
#[test]
fn the_deleting_window_is_arclites() {
    let (title, lines) = delete_text(&deleting(1024, 4096), Duration::from_secs(2));
    assert_eq!(title, "{25%} Updating archive...");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0], "   1 KB / 4 KB @   512 B/s");
    assert_eq!(filled(&lines[1]), 15);
    assert_eq!(lines[1].chars().count(), 60);
    let (_, lines) = delete_text(&deleting(2048, 4096), Duration::from_millis(500));
    assert_eq!(
        lines[0], "   2 KB / 4 KB @    4 KB/s",
        "by the milliseconds"
    );
}

/// Its percent rounded as arclite's `al_round` (half up), not cut as the update's: the title and the bar
/// alike; at most 100, none of nothing.
#[test]
fn the_deleting_percent_is_rounded() {
    let title = |done: u64, total: u64| delete_text(&deleting(done, total), Duration::ZERO).0;
    assert_eq!(title(2, 3), "{67%} Updating archive...", "66.7 up");
    assert_eq!(title(1, 3), "{33%} Updating archive...", "33.3 down");
    assert_eq!(title(1, 8), "{13%} Updating archive...", "12.5 half up");
    assert_eq!(title(1, 200), "{1%} Updating archive...", "0.5 half up");
    assert_eq!(title(1, 201), "{0%} Updating archive...");
    assert_eq!(title(0, 0), "{0%} Updating archive...", "nothing to write");
    assert_eq!(title(300, 100), "{100%} Updating archive...", "at most 100");
    assert_eq!(title(u64::MAX, u64::MAX), "{100%} Updating archive...");
    let (_, lines) = delete_text(&deleting(2, 3), Duration::ZERO);
    assert_eq!(filled(&lines[1]), 40, "67% of 60, not 66%'s 39");
    assert_eq!(lines[0], "      2 / 3 @     0 B/s", "no time: no speed");
    let (_, lines) = delete_text(&deleting(300, 100), Duration::from_secs(3));
    assert_eq!(filled(&lines[1]), 60);
    assert_eq!(lines[0], "    300 / 100 @   100 B/s");
}
