use super::*;

const NOW: (i64, i64, i64) = (2026, 11, 18);

fn one(line: &str) -> Listed {
    listed(line, NOW).unwrap_or_else(|| panic!("not read: {line}"))
}

#[test]
fn a_date_is_seconds_since_1970() {
    assert_eq!(seconds_of((1970, 1, 1), (0, 0, 0)), 0);
    assert_eq!(seconds_of((2000, 3, 1), (12, 0, 0)), 951_912_000);
}

#[test]
fn full_time_lines_read_with_their_zone() {
    let line = "-rw-r--r-- 1 user users 1234 2024-03-10 12:30:45.123456789 +0200 report.txt";
    let read = one(line);
    assert_eq!(read.entry.name, "report.txt");
    assert_eq!(read.entry.size, 1234);
    assert_eq!(read.entry.owner.as_deref(), Some("user"));
    assert_eq!(read.entry.group.as_deref(), Some("users"));
    let utc = seconds_of((2024, 3, 10), (10, 30, 45));
    assert_eq!(read.entry.modified, Some(u32::try_from(utc).unwrap()));
    assert_eq!(read.entry.permissions, Some(0o100_644));
    assert!(!read.entry.is_dir);
}

#[test]
fn classic_dates_take_this_year_or_the_last() {
    let recent = one("drwxr-xr-x  2 root root 4096 Nov  1 09:15 docs");
    assert!(recent.entry.is_dir);
    let expected = seconds_of((2026, 11, 1), (9, 15, 0));
    assert_eq!(
        recent.entry.modified,
        Some(u32::try_from(expected).unwrap())
    );
    let ahead = one("-rw------- 1 u g 7 Dec 25 10:00 later.txt");
    let last_year = seconds_of((2025, 12, 25), (10, 0, 0));
    assert_eq!(
        ahead.entry.modified,
        Some(u32::try_from(last_year).unwrap())
    );
    let old = one("-rw-r--r-- 1 u g 7 Jan  5  2019 old.txt");
    let midnight = seconds_of((2019, 1, 5), (0, 0, 0));
    assert_eq!(old.entry.modified, Some(u32::try_from(midnight).unwrap()));
    let day_first = one("-rw-r--r-- 1 u g 7 5 Jan 2019 eu.txt");
    assert_eq!(day_first.entry.modified, old.entry.modified);
    // How exact each is (WinSCP's `TModificationFmt`).
    assert_eq!(recent.entry.precision, Precision::Minute);
    assert_eq!(old.entry.precision, Precision::Day);
    let full = one("-rw-r--r-- 1 u g 7 2024-03-10 12:30:45 +0000 f.txt");
    assert_eq!(full.entry.precision, Precision::Second);
}

#[test]
fn links_groups_with_spaces_specials_and_devices() {
    let link = one("lrwxrwxrwx 1 u g 11 Nov  1 09:15 www -> /var/www/html");
    assert!(link.entry.is_link);
    assert_eq!(link.entry.name, "www");
    assert_eq!(link.link_to.as_deref(), Some("/var/www/html"));
    let spaced = one("-rw-r--r--+ 1 u Domain Users 10 Nov  1 09:15 a b.txt");
    assert_eq!(spaced.entry.group.as_deref(), Some("Domain Users"));
    assert_eq!(spaced.entry.name, "a b.txt");
    let suid = one("-rwsr-xr-t 1 u g 1 Nov  1 09:15 tool");
    assert_eq!(
        suid.entry.permissions,
        Some(0o100_000 | 0o4000 | 0o1000 | 0o755)
    );
    let device = one("crw-rw-rw- 1 root root 1, 3 Nov  1 09:15 null");
    assert_eq!(device.entry.size, 3);
    let busybox = one("-rw-r--r-- root root 5 Nov  1 09:15 bb.txt");
    assert_eq!(busybox.entry.owner.as_deref(), Some("root"));
}

#[test]
fn a_listing_leaves_out_total_and_the_dots() {
    let text = "total 8\ndrwxr-xr-x 2 u g 4096 Nov  1 09:15 .\ndrwxr-xr-x 9 u g 4096 Nov  1 09:15 ..\n-rw-r--r-- 1 u g 3 Nov  1 09:15 a.txt\nnot a line\n";
    let read = listing(text, NOW);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].entry.name, "a.txt");
}
