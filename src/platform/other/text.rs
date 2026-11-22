//! Locale and text on every OS but Windows until the port: the user's formats as `std` gives none,
//! code pages as UTF-8 and none installed, no folding for the fuzzy search.

use super::super::locale::{CodePageInfo, DateFormat, DigitGroups, NumberFormat};

pub fn date_format() -> DateFormat {
    DateFormat::default()
}

/// The short name of `month` (1 .. 12), English as FAR's for a system that names none.
pub fn month_name(month: u16) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let at = usize::from(month.clamp(1, 12)) - 1;
    MONTHS[at].to_string()
}

pub fn number_format() -> NumberFormat {
    NumberFormat::default()
}

pub fn digit_groups() -> DigitGroups {
    DigitGroups::default()
}

pub fn two_digit_year_max() -> u32 {
    2049
}

pub fn decode_oem(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Windows' Western code pages, for the lists that name them.
pub fn ansi_code_page() -> u32 {
    1252
}

pub fn oem_code_page() -> u32 {
    437
}

pub fn installed_code_pages() -> &'static [CodePageInfo] {
    &[]
}

pub fn decode_page(_code_page: u32, bytes: &[u8], _last: bool) -> (String, usize) {
    (String::from_utf8_lossy(bytes).into_owned(), bytes.len())
}

pub fn encode_page(_code_page: u32, text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

pub fn take_locale_change() -> bool {
    false
}

/// FAR's fuzzy search folds nothing here until the port.
pub fn fuzzy_fold(text: &str) -> String {
    text.to_string()
}
