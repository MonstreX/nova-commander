//! The file times' units and a name's parts.

use super::*;

#[test]
fn ticks_are_moments() {
    assert_eq!(from_ticks(0), None);
    assert_eq!(from_ticks(TICKS_TO_1970), Some(UNIX_EPOCH));
    assert_eq!(
        from_ticks(TICKS_TO_1970 + 10_000_000),
        Some(UNIX_EPOCH + Duration::from_secs(1))
    );
    assert!(from_ticks(1).is_some_and(|moment| moment < UNIX_EPOCH));
    assert_eq!(to_ticks(UNIX_EPOCH), i64::try_from(TICKS_TO_1970).ok());
    assert_eq!(to_ticks(from_ticks(12_345_678).unwrap()), Some(12_345_678));
    // Before 1601 no ticks; Windows' std (since Rust 1.100) has no such moment at all.
    let before_1601 = UNIX_EPOCH.checked_sub(Duration::from_secs(400 * 366 * 86_400));
    assert!(before_1601.is_none_or(|moment| to_ticks(moment).is_none()));
    assert_eq!(split("/a//b/"), ["a", "b"]);
    assert_eq!(split("a\\b\u{f05c}c"), ["a", "b", "c"]);
}
