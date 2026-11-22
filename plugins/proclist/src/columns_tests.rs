use super::*;

const K: u64 = 1024;

/// FAR's own cases (`far/strmix.cpp`, `TEST_CASE("FileSizeToStrInvariant")`): the float style, binary and
/// decimal.
#[test]
fn the_float_style_is_fars() {
    let binary = [
        (999, "999"),
        (1023, "1023"),
        (1024, "1.00 K"),
        (1536, "1.50 K"),
        (2042, "1.99 K"),
        (2043, "2.00 K"),
        (10 * K, "10.0 K"),
        (10 * K + 143, "10.1 K"),
        (100 * K, "100 K"),
        (1_000_000, "977 K"),
        (K * K, "1.00 M"),
        (u64::MAX, "16.0 E"),
    ];
    for (value, text) in binary {
        assert_eq!(size(value, 0, false, '.'), text, "{value}");
    }
    let decimal = [
        (1000, "1.00 k"),
        (1023, "1.02 k"),
        (1536, "1.54 k"),
        (100 * K, "102 k"),
        (1_000_000, "1.00 m"),
    ];
    for (value, text) in decimal {
        assert_eq!(size(value, 0, true, '.'), text, "{value}");
    }
    assert_eq!(size(1536, 0, false, ','), "1,50 K", "the user's separator");
}

#[test]
fn a_number_too_wide_for_its_column_takes_a_unit() {
    assert_eq!(size(1024, 5, false, '.'), "1024");
    assert_eq!(size(1024, 4, false, '.'), "1024");
    assert_eq!(size(1024, 3, false, '.'), "1 K");
    assert_eq!(size(10 * K * K, 7, false, '.'), "10240 K");
    assert_eq!(size(123_456_789, 6, false, '.'), "118 M");
    assert_eq!(size(123_456_789, 6, true, '.'), "123 m");
    assert_eq!(size(0, 3, false, '.'), "0");
}

#[test]
fn a_duration_is_days_hours_minutes_seconds_and_ticks() {
    assert_eq!(duration(0), "00:00:00.0000000");
    assert_eq!(duration(12_345_678), "00:00:01.2345678");
    let day = 24 * 3600 * 10_000_000;
    assert_eq!(
        duration(2 * day + 3_610 * 10_000_000 + 5),
        "2 01:00:10.0000005"
    );
}

#[test]
fn a_text_is_right_aligned_in_its_column() {
    assert_eq!(right("42", 5), "   42");
    assert_eq!(right("123456", 3), "123456");
}
