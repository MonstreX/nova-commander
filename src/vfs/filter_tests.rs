//! FAR's rules of the filters, entry by entry.

use super::*;

fn file(name: &str, size: u64) -> Entry {
    Entry {
        name: name.to_string(),
        size,
        ..Entry::default()
    }
}

fn folder(name: &str) -> Entry {
    Entry {
        name: name.to_string(),
        is_dir: true,
        ..Entry::default()
    }
}

fn masked(mark: Mark, mask: &str) -> Marked {
    Marked {
        mark,
        conditions: Conditions {
            mask: FileMask::parse(mask),
            ..Conditions::default()
        },
    }
}

fn file_type(mark: Mark, mask: &str) -> Marked {
    Marked {
        mark,
        conditions: type_conditions(FileMask::parse(mask).unwrap()),
    }
}

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
}

#[test]
fn nothing_marked_shows_everything() {
    let filter = AreaFilter::default();
    assert!(!filter.is_on());
    assert!(filter.takes(&file("a.txt", 1), now()));
}

#[test]
fn an_include_shows_only_what_it_takes_and_the_folders() {
    let filter = AreaFilter {
        types: vec![file_type(Mark::Include, "*.txt")],
        ..AreaFilter::default()
    };
    assert!(filter.is_on());
    assert!(filter.takes(&file("a.txt", 1), now()));
    assert!(!filter.takes(&file("b.rs", 1), now()));
    assert!(
        filter.takes(&folder("src"), now()),
        "a file type's filter leaves the folders"
    );
}

#[test]
fn an_exclude_hides_what_it_takes_and_shows_the_rest() {
    let filter = AreaFilter {
        types: vec![file_type(Mark::Exclude, "*.bak")],
        ..AreaFilter::default()
    };
    assert!(!filter.takes(&file("a.bak", 1), now()));
    assert!(filter.takes(&file("a.txt", 1), now()));
}

#[test]
fn the_first_filter_decides_a_strong_one_over_the_weak_before_it() {
    let weak = AreaFilter {
        filters: vec![masked(Mark::Include, "*.*"), masked(Mark::Exclude, "*.bak")],
        ..AreaFilter::default()
    };
    assert!(weak.takes(&file("a.bak", 1), now()), "the first takes it");
    let strong = AreaFilter {
        filters: vec![
            masked(Mark::Include, "*.*"),
            masked(Mark::StrongExclude, "*.bak"),
        ],
        ..AreaFilter::default()
    };
    assert!(
        !strong.takes(&file("a.bak", 1), now()),
        "a strong one decides after all"
    );
    assert!(strong.takes(&file("a.txt", 1), now()));
}

#[test]
fn a_user_include_takes_folders_too_unless_it_leaves_them_out() {
    let all = AreaFilter {
        filters: vec![masked(Mark::Include, "a*")],
        ..AreaFilter::default()
    };
    assert!(
        !all.takes(&folder("src"), now()),
        "an include for files and folders"
    );
    assert!(all.takes(&folder("app"), now()));
    let files = AreaFilter {
        filters: vec![file_type(Mark::Include, "a*")],
        ..AreaFilter::default()
    };
    assert!(files.takes(&folder("src"), now()));
    let select = AreaFilter {
        select: true,
        ..files
    };
    assert!(
        !select.takes(&folder("src"), now()),
        "Select shows no folder by default"
    );
}

#[test]
fn the_folders_filter_takes_folders_only() {
    let filter = AreaFilter {
        folders: Mark::Exclude,
        ..AreaFilter::default()
    };
    assert!(!filter.takes(&folder("src"), now()));
    assert!(filter.takes(&file("a.txt", 1), now()));
}

#[test]
fn sizes_dates_and_attributes() {
    let sized = Conditions {
        size: Some((parse_size("2"), parse_size("1K"))),
        ..Conditions::default()
    };
    assert!(!sized.take(&file("a", 1), now()));
    assert!(sized.take(&file("a", 1024), now()));
    assert!(!sized.take(&file("a", 1025), now()));

    let day = Duration::from_secs(86_400);
    let recent = Conditions {
        date: Some((
            DateKind::Modified,
            Dates::Relative {
                after: day,
                before: Duration::ZERO,
            },
        )),
        ..Conditions::default()
    };
    let mut old = file("old", 1);
    old.modified = now().checked_sub(day * 2);
    let mut new = file("new", 1);
    new.modified = now().checked_sub(day / 2);
    assert!(!recent.take(&old, now()));
    assert!(recent.take(&new, now()));

    let no_folders = Conditions {
        attributes: Some((0, DIRECTORY)),
        ..Conditions::default()
    };
    assert!(no_folders.take(&file("a", 1), now()));
    assert!(!no_folders.take(&folder("a"), now()));
}

#[test]
fn sizes_are_written_as_far_writes_them() {
    assert_eq!(parse_size("10"), Some(10));
    assert_eq!(parse_size("10B"), Some(10));
    assert_eq!(parse_size("2K"), Some(2048));
    assert_eq!(parse_size("2k"), Some(2000));
    assert_eq!(parse_size("1M"), Some(1024 * 1024));
    assert_eq!(parse_size("1g"), Some(1_000_000_000));
    assert_eq!(parse_size("K"), None);
    assert_eq!(parse_size("1KB"), None);
    assert_eq!(parse_size("1 K"), None);
    assert_eq!(parse_size("1Q"), None);
}
