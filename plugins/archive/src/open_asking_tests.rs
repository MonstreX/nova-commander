use super::*;

/// arclite's opener window: the volume, the files of their total with a bar, a separator, the bytes of
/// theirs with a bar; the title's percent by the files when their total is known, else by the bytes.
#[test]
fn the_window_is_arclites() {
    let counts = Counts {
        total_files: 4,
        total_bytes: 2048,
        files: 1,
        bytes: 1024,
    };
    let (title, lines) = window("a.7z", counts);
    assert_eq!(title, "{25%} Reading archive...");
    assert_eq!(lines[0], "a.7z");
    assert_eq!(lines[1], "1 / 4");
    assert_eq!(lines[2], bar(25));
    assert_eq!(lines[3], "\u{1}");
    assert_eq!(
        lines[4],
        format!("{} / {}", data_size(1024, &SIZES), data_size(2048, &SIZES))
    );
    assert_eq!(lines[5], bar(50));
    let by_bytes = Counts {
        total_files: 0,
        ..counts
    };
    assert_eq!(window("a.7z", by_bytes).0, "{50%} Reading archive...");
}

/// A long volume name fitted to the window's width; nothing known yet is 0 of 0 at 0%.
#[test]
fn the_window_fits_its_name_and_starts_at_nothing() {
    let long = "v".repeat(WIDTH + 20);
    let (title, lines) = window(&long, Counts::default());
    assert_eq!(lines[0], fit(&long, WIDTH));
    assert_eq!(lines[0].chars().count(), WIDTH);
    assert_eq!(title, "{0%} Reading archive...");
    assert_eq!(lines[1], "0 / 0");
    assert_eq!((lines[2].clone(), lines[5].clone()), (bar(0), bar(0)));
    assert_eq!(lines.len(), 6);
}
