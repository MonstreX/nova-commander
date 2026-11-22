//! File sizes as FAR Manager shows them (port of `FileSizeToStr`, `far/strmix.cpp`): plain numbers that
//! switch to K/M/G... when they do not fit, optional digit grouping and the "float" style used in totals.

use crate::platform::locale::NumberFormat;

const UNIT_SYMBOLS: [char; 7] = ['B', 'K', 'M', 'G', 'T', 'P', 'E'];
const BITS_PER_UNIT: u32 = 10;

/// How a size is formatted (FAR's `COLFLAGS_*` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SizeStyle {
    /// Thousands separators (`COLFLAGS_GROUPDIGITS`).
    pub group_digits: bool,
    /// "1.21 M" instead of whole numbers (`COLFLAGS_FLOATSIZE`).
    pub float: bool,
    /// No space before the unit letter (`COLFLAGS_ECONOMIC`).
    pub compact: bool,
    /// Always show the unit letter, "0 B" (`COLFLAGS_SHOW_MULTIPLIER`).
    pub show_unit: bool,
    /// Units of 1000 written in small letters, "1 k" (`COLFLAGS_THOUSAND`).
    pub thousand: bool,
}

/// Formats `size` into `width` characters (0 = as long as needed), right-aligned unless `left_align`.
/// Text that still does not fit is cut and marked with `…`.
pub fn format_size(
    size: u64,
    width: usize,
    left_align: bool,
    style: SizeStyle,
    numbers: NumberFormat,
) -> String {
    format_size_from_unit(size, width, left_align, style, numbers, 0)
}

/// [`format_size`] that shows at least `min_unit` (1 = K, 2 = M, ...), as FAR's
/// `COLFLAGS_MULTIPLIER_K`/`_M`/... do; the float style ignores it.
pub fn format_size_from_unit(
    size: u64,
    width: usize,
    left_align: bool,
    style: SizeStyle,
    numbers: NumberFormat,
    min_unit: usize,
) -> String {
    let fit = Fit {
        width,
        left_align,
        style,
    };
    if style.float {
        let (text, unit) = float_size(size, width, style, numbers.decimal_separator);
        return fit.with_unit(text, unit);
    }
    let to_text = |value: u64| {
        if style.group_digits {
            group_digits(value, numbers.thousands_separator)
        } else {
            value.to_string()
        }
    };
    let mut text = to_text(size);
    let suffix = if style.show_unit || (width > 0 && char_count(&text) > width) {
        if style.compact { 1 } else { 2 }
    } else {
        0
    };
    let max_number_width = width.saturating_sub(suffix);
    let mut unit = 0;
    while unit + 1 < UNIT_SYMBOLS.len()
        && (unit < min_unit || (width > 0 && char_count(&text) > max_number_width))
    {
        let value = rounded_in_unit(size, unit + 1, style.thousand);
        // FAR's minimum unit divides even down to 0 ("0 M"); fitting the width stops at 0.
        if value == 0 && unit >= min_unit {
            break;
        }
        unit += 1;
        text = to_text(value);
    }
    fit.with_unit(text, unit)
}

struct Fit {
    width: usize,
    left_align: bool,
    style: SizeStyle,
}

impl Fit {
    fn with_unit(&self, number: String, unit: usize) -> String {
        if unit == 0 && !self.style.show_unit {
            return self.apply(number);
        }
        let space = if self.style.compact { "" } else { " " };
        let symbol = if self.style.thousand {
            UNIT_SYMBOLS[unit].to_ascii_lowercase()
        } else {
            UNIT_SYMBOLS[unit]
        };
        self.apply(format!("{number}{space}{symbol}"))
    }

    fn apply(&self, text: String) -> String {
        let width = self.width;
        let length = char_count(&text);
        if width == 0 {
            return text;
        }
        if length <= width {
            return if self.left_align {
                format!("{text:<width$}")
            } else {
                format!("{text:>width$}")
            };
        }
        let keep = width - 1;
        if self.left_align {
            let head: String = text.chars().take(keep).collect();
            format!("{head}…")
        } else {
            let tail: String = text.chars().skip(length - keep).collect();
            format!("…{tail}")
        }
    }
}

/// Size in `unit` (1 = K, 2 = M, ...), rounded half up as FAR does.
fn rounded_in_unit(size: u64, unit: usize, thousand: bool) -> u64 {
    let Some(denominator) = unit_value(unit, thousand) else {
        return 0;
    };
    let integral = size / denominator;
    let remainder = size % denominator;
    integral + u64::from(remainder >= denominator - remainder)
}

/// What one of `unit` is: 1024 or 1000 to its power.
fn unit_value(unit: usize, thousand: bool) -> Option<u64> {
    let power = u32::try_from(unit).ok()?;
    if thousand {
        return 1000u64.checked_pow(power);
    }
    1u64.checked_shl(power.checked_mul(BITS_PER_UNIT)?)
}

/// The largest unit `size` has a whole one of (FAR's `log2(size) / 10`, `log10(size) / 3`).
fn largest_unit(size: u64, thousand: bool) -> usize {
    if thousand {
        let mut unit = 0;
        while unit_value(unit + 1, true).is_some_and(|value| size >= value) {
            unit += 1;
        }
        return unit;
    }
    if size == 0 {
        0
    } else {
        ((u64::BITS - 1 - size.leading_zeros()) / BITS_PER_UNIT) as usize
    }
}

/// FAR's float style: three significant digits ("1.50 K", "10.1 K", "100 K").
fn float_size(size: u64, width: usize, style: SizeStyle, decimal: char) -> (String, usize) {
    let unit = largest_unit(size, style.thousand);
    let Some(denominator) = unit_value(unit, style.thousand).filter(|_| unit > 0) else {
        return (size.to_string(), 0);
    };
    let raw_integral = size / denominator;
    let raw_fraction = (size % denominator) as f64 / denominator as f64;
    let mut digits: u32 = if raw_integral < 10 {
        2
    } else if raw_integral < 100 {
        1
    } else {
        0
    };
    let (integral, fraction) = loop {
        let multiplier = 10u64.pow(digits);
        let rounded = (raw_fraction * multiplier as f64).round() as u64;
        let (integral, fraction) = if rounded == multiplier {
            (raw_integral + 1, 0)
        } else {
            (raw_integral, rounded)
        };
        let integral_length = if integral < 10 {
            1
        } else if integral < 100 {
            2
        } else {
            3
        };
        digits = digits.min(3 - integral_length);
        let required =
            integral_length + u32::from(digits != 0) + digits + u32::from(!style.compact) + 1;
        let width = u32::try_from(width).unwrap_or(u32::MAX);
        if width != 0 && width < required && digits != 0 {
            let overflow = required - width;
            if overflow <= digits + 1 {
                let with_separator = digits + 1 - overflow;
                digits = with_separator.saturating_sub(1);
                if digits != 0 {
                    continue;
                }
            }
        }
        break (integral, fraction);
    };
    let text = if digits == 0 {
        ((raw_integral as f64 + raw_fraction).round() as u64).to_string()
    } else {
        let digits = digits as usize;
        format!("{integral}{decimal}{fraction:0>digits$}")
    };
    (text, unit)
}

pub use crate::platform::locale::group_digits;

fn char_count(text: &str) -> usize {
    text.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVARIANT: NumberFormat = NumberFormat {
        thousands_separator: ',',
        decimal_separator: '.',
    };
    const K: u64 = 1024;
    const M: u64 = K * K;
    const G: u64 = M * K;
    const T: u64 = G * K;
    const P: u64 = T * K;
    const E: u64 = P * K;

    const PLAIN: SizeStyle = SizeStyle {
        group_digits: false,
        float: false,
        compact: false,
        show_unit: false,
        thousand: false,
    };
    const FLOAT: SizeStyle = SizeStyle {
        float: true,
        ..PLAIN
    };

    /// Width as FAR passes it: negative means left-aligned.
    fn far(size: u64, width: i32, style: SizeStyle) -> String {
        let columns = usize::try_from(width.unsigned_abs()).unwrap();
        format_size(size, columns, width < 0, style, INVARIANT)
    }

    // The cases below are FAR's own test table (`far/strmix.cpp`, TEST_CASE("FileSizeToStrInvariant"),
    // binary column) for the styles we use.

    #[test]
    fn plain_sizes() {
        assert_eq!(far(0, 0, PLAIN), "0");
        assert_eq!(far(1023, 0, PLAIN), "1023");
        assert_eq!(far(1025, 0, PLAIN), "1025");
        assert_eq!(far(1024, -5, PLAIN), "1024 ");
        assert_eq!(far(1024, -4, PLAIN), "1024");
        assert_eq!(far(1024, -3, PLAIN), "1 K");
        assert_eq!(far(1024, -2, PLAIN), "1…");
        assert_eq!(far(1024, -1, PLAIN), "…");
        let compact = SizeStyle {
            compact: true,
            ..PLAIN
        };
        assert_eq!(far(1024, -2, compact), "1K");
        let shown = SizeStyle {
            show_unit: true,
            ..PLAIN
        };
        assert_eq!(far(0, 0, shown), "0 B");
    }

    #[test]
    fn plain_maximum_in_every_width() {
        let max = u64::MAX;
        let expected = [
            (20, "18446744073709551615"),
            (19, "18014398509481984 K"),
            (18, "  17592186044416 M"),
            (16, "17592186044416 M"),
            (15, "  17179869184 G"),
            (12, "  16777216 T"),
            (10, "16777216 T"),
            (9, "  16384 P"),
            (7, "16384 P"),
            (6, "  16 E"),
            (4, "16 E"),
            (3, "… E"),
            (2, "…E"),
            (1, "…"),
        ];
        for (width, text) in expected {
            assert_eq!(far(max, width, PLAIN), text, "width {width}");
        }
    }

    #[test]
    fn float_sizes() {
        let cases = [
            (999, "999"),
            (1000, "1000"),
            (1023, "1023"),
            (1024, "1.00 K"),
            (1536, "1.50 K"),
            (2042, "1.99 K"),
            (2043, "2.00 K"),
            (10 * K, "10.0 K"),
            (100 * K, "100 K"),
            (M, "1.00 M"),
            (1_000_000, "977 K"),
            (G, "1.00 G"),
            (1_000_000_000, "954 M"),
            (T, "1.00 T"),
            (P, "1.00 P"),
            (E, "1.00 E"),
        ];
        for (size, text) in cases {
            assert_eq!(far(size, 0, FLOAT), text, "size {size}");
        }
        let shown = SizeStyle {
            show_unit: true,
            ..FLOAT
        };
        assert_eq!(far(0, 0, shown), "0 B");
    }

    #[test]
    fn float_sizes_in_narrow_columns() {
        let max = u64::MAX;
        let cases = [
            (max, 6, "16.0 E"),
            (max, 5, " 16 E"),
            (max, 3, "… E"),
            (1_000_000, 6, " 977 K"),
            (99_999, 6, "97.7 K"),
            (99_999, 5, " 98 K"),
            (99_999, 3, "… K"),
            (9_999, 6, "9.76 K"),
            (9_999, 5, "9.8 K"),
            (9_999, 4, "10 K"),
            (1024, 5, "1.0 K"),
            (1024, 4, " 1 K"),
            (1024, 2, "…K"),
            (1023, 6, "  1023"),
            (1023, 3, "…23"),
            (999, 2, "…9"),
        ];
        for (size, width, text) in cases {
            assert_eq!(far(size, width, FLOAT), text, "size {size} width {width}");
        }
        let compact = SizeStyle {
            compact: true,
            ..FLOAT
        };
        assert_eq!(far(99_999, 5, compact), "97.7K");
        assert_eq!(far(99_999, 4, compact), " 98K");
        assert_eq!(far(99_999, 2, compact), "…K");
    }

    #[test]
    fn grouped_digits_use_the_regional_separator() {
        let threes = crate::platform::locale::DigitGroups::default();
        assert_eq!(threes.apply(0, ','), "0");
        assert_eq!(threes.apply(999, ','), "999");
        assert_eq!(threes.apply(1_234_567, '\u{a0}'), "1\u{a0}234\u{a0}567");
        let grouped = SizeStyle {
            group_digits: true,
            ..PLAIN
        };
        assert_eq!(far(1_234_567, 10, grouped), " 1,234,567");
        assert_eq!(far(12_345_678_901, 10, grouped), "  11,774 M");
    }

    #[test]
    fn units_of_a_thousand_are_written_small() {
        let thousand = SizeStyle {
            thousand: true,
            ..SizeStyle::default()
        };
        assert_eq!(format_size(1024, 3, false, thousand, INVARIANT), "1 k");
        assert_eq!(
            format_size(1_500_000, 4, false, thousand, INVARIANT),
            " 2 m"
        );
        assert_eq!(format_size(999, 0, false, thousand, INVARIANT), "999");
        let float = SizeStyle {
            float: true,
            ..thousand
        };
        assert_eq!(format_size(1_500_000, 0, false, float, INVARIANT), "1.50 m");
        assert_eq!(format_size(999_999, 0, false, float, INVARIANT), "1000 k");
        let compact = SizeStyle {
            compact: true,
            ..thousand
        };
        assert_eq!(format_size(2_000, 2, false, compact, INVARIANT), "2k");
    }
}
