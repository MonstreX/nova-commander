//! Text formatting for panels and dialogs: the clock and padding/cutting to a width (dates are
//! `platform::locale::date_time`).

use std::time::SystemTime;

use crate::app::text_cut::{cells, head, left_aligned};
use crate::platform;

/// The current local time as "hh:mm" for the clock.
pub fn clock(now: SystemTime) -> String {
    platform::locale::date_time(now, false, 5).1
}

/// Pads or cuts `text` to `width` cells (R7 review E14: the screen's, a wide character two); a cut text
/// ends with `»`.
pub fn fit(text: &str, width: usize) -> String {
    if cells(text) <= width {
        return left_aligned(text, width);
    }
    if width == 0 {
        return String::new();
    }
    left_aligned(&format!("{}»", head(text, width - 1)), width)
}

/// `text` centred in `width` cells, cut on the right if longer (FAR's `fit_to_center`).
pub fn center(text: &str, width: usize) -> String {
    let cut = head(text, width);
    let free = width - cells(cut);
    format!(
        "{}{cut}{}",
        " ".repeat(free / 2),
        " ".repeat(free - free / 2)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_pads_and_cuts() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 4), "abc»");
        assert_eq!(fit("файл", 4), "файл");
        assert_eq!(fit("x", 0), "");
        assert_eq!(
            fit("日本語", 4),
            "日» ",
            "a wide character crossing the edge left out"
        );
    }

    #[test]
    fn center_pads_both_sides() {
        assert_eq!(center("Up", 6), "  Up  ");
        assert_eq!(center("Name", 3), "Nam");
        assert_eq!(center("日本", 7), " 日本  ");
    }
}
