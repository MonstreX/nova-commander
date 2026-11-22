//! Locale: local time and the user's regional date and number formats.

pub use super::date_text::{DateStyle, date_time, styled_date_time};
pub use super::os::{
    ansi_code_page, date_format, decode_oem, decode_page, digit_groups, encode_page,
    from_local_time, fuzzy_fold, installed_code_pages, local_time, month_name, number_format,
    oem_code_page, take_locale_change, two_digit_year_max,
};

/// A code page the system has: its number, its name ("ANSI - Cyrillic") and its longest character
/// in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePageInfo {
    pub id: u32,
    pub name: String,
    pub max_char_size: u32,
}

/// A point in local time, as the user's clock shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

/// Order of the date parts in the user's short date format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    MonthDayYear,
    DayMonthYear,
    YearMonthDay,
}

/// The user's regional date and time settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateFormat {
    pub order: DateOrder,
    pub date_separator: char,
    pub time_separator: char,
}

impl Default for DateFormat {
    fn default() -> Self {
        Self {
            order: DateOrder::DayMonthYear,
            date_separator: '.',
            time_separator: ':',
        }
    }
}

/// The user's regional number separators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberFormat {
    pub thousands_separator: char,
    pub decimal_separator: char,
}

impl Default for NumberFormat {
    fn default() -> Self {
        Self {
            thousands_separator: ',',
            decimal_separator: '.',
        }
    }
}

/// How digits are grouped from the right, as the system's `LOCALE_SGROUPING` says (FAR's
/// `digits_grouping`): `3;0` is threes, `3;2;0` is three and then twos, `3` is one group of three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigitGroups {
    sizes: [u8; 9],
    count: usize,
    /// The last size goes on to the end.
    repeats: bool,
}

impl Default for DigitGroups {
    fn default() -> Self {
        Self {
            sizes: [3, 0, 0, 0, 0, 0, 0, 0, 0],
            count: 1,
            repeats: true,
        }
    }
}

impl DigitGroups {
    /// Reads the system's text: `0;0` is no groups at all; what makes no sense is the default.
    pub fn parse(text: &str) -> Self {
        let parts: Vec<&str> = text.split(';').collect();
        let repeats = parts.last() == Some(&"0");
        let mut groups = Self {
            sizes: [0; 9],
            count: 0,
            repeats,
        };
        for part in parts.iter().filter(|part| **part != "0") {
            match part.parse::<u8>() {
                Ok(size @ 1..=9) if groups.count < groups.sizes.len() => {
                    groups.sizes[groups.count] = size;
                    groups.count += 1;
                }
                _ => return Self::default(),
            }
        }
        groups
    }

    /// The size of group `index`, counted from the right; `None` when the rest is not grouped.
    fn size(self, index: usize) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let at = if index < self.count {
            index
        } else if self.repeats {
            self.count - 1
        } else {
            return None;
        };
        Some(usize::from(self.sizes[at]))
    }

    /// "1234567" -> "12,34,567" for `3;2;0`.
    pub fn apply(self, value: u64, separator: char) -> String {
        let digits: Vec<char> = value.to_string().chars().rev().collect();
        let mut grouped = Vec::with_capacity(digits.len() * 2);
        let (mut group, mut filled) = (0, 0);
        for digit in digits {
            if self.size(group).is_some_and(|size| filled == size) {
                grouped.push(separator);
                group += 1;
                filled = 0;
            }
            grouped.push(digit);
            filled += 1;
        }
        grouped.iter().rev().collect()
    }
}

/// "1234567" -> "1,234,567" with the given separator, in the user's groups.
pub fn group_digits(value: u64, separator: char) -> String {
    digit_groups().apply(value, separator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_go_in_the_groups_the_system_says() {
        let groups = |text| DigitGroups::parse(text).apply(1_234_567_890, ',');
        assert_eq!(groups("3;0"), "1,234,567,890");
        assert_eq!(groups("3;2;0"), "1,23,45,67,890");
        assert_eq!(groups("3"), "1234567,890");
        assert_eq!(groups("3;2"), "12345,67,890");
    }

    #[test]
    fn nonsense_from_the_system_is_threes() {
        let threes = DigitGroups::default();
        assert_eq!(DigitGroups::parse(""), threes);
        assert_eq!(DigitGroups::parse("x;0"), threes);
    }

    #[test]
    fn the_systems_no_grouping_is_no_grouping() {
        assert_eq!(DigitGroups::parse("0;0").apply(1_234_567, ','), "1234567");
        assert_eq!(DigitGroups::parse("0").apply(1_234_567, ','), "1234567");
    }

    #[test]
    fn short_numbers_have_no_separator() {
        assert_eq!(DigitGroups::default().apply(0, ','), "0");
        assert_eq!(DigitGroups::default().apply(999, ','), "999");
        assert_eq!(DigitGroups::default().apply(1000, ','), "1,000");
    }
}
