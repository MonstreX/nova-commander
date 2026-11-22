//! arclite's progress window while it adds to an archive (`ArchiveUpdateProgress`, update.cpp): its title
//! `{N%} Updating archive...` (`Creating archive...` for a new one), the archive, the file at hand with
//! its bytes and bar, all the bytes at the speed with the time left, what was read against what was
//! written, the whole bar; shown as the extraction's is (`progress`).

use std::time::Duration;

use crate::layout::fit;
use crate::progress::{SIZES, SPEEDS, WIDTH, bar, data_size, percent};
use crate::update::UpdateAt;

/// arclite's `format_time`: hours, minutes and seconds, two digits each.
fn time(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// The window as arclite's `do_update_ui` writes it, `elapsed` the time worked so far.
pub fn text(
    (new_arc, archive): (bool, &str),
    at: &UpdateAt<'_>,
    elapsed: Duration,
) -> (String, Vec<String>) {
    let millis = elapsed.as_millis();
    let speed = if millis == 0 {
        0
    } else {
        u64::try_from(u128::from(at.done) * 1000 / millis).unwrap_or(u64::MAX)
    };
    let worked = elapsed.as_secs();
    // The whole time as the bytes so far took theirs, never less than the time worked.
    let whole = if at.done == 0 {
        0
    } else {
        u64::try_from(u128::from(at.total) * u128::from(worked) / u128::from(at.done))
            .unwrap_or(u64::MAX)
    }
    .max(worked);
    let lines = vec![
        fit(archive, WIDTH),
        "\u{1}".to_string(),
        fit(&at.file.display().to_string(), WIDTH),
        format!(
            "{:>7} / {}",
            data_size(at.file_done, &SIZES),
            data_size(at.file_total, &SIZES)
        ),
        bar(percent(at.file_done, at.file_total)),
        "\u{1}".to_string(),
        format!(
            "{:>7} / {} @ {:>9} -{}",
            data_size(at.done, &SIZES),
            data_size(at.total, &SIZES),
            data_size(speed, &SPEEDS),
            time(whole - worked)
        ),
        format!(
            "{:>7} \u{2192} {:>7} = {:>2}%",
            data_size(at.done, &SIZES),
            data_size(at.written, &SIZES),
            percent(at.written, at.done)
        ),
        bar(percent(at.done, at.total)),
    ];
    let what = if new_arc {
        "Creating archive..."
    } else {
        "Updating archive..."
    };
    (format!("{{{}%}} {what}", percent(at.done, at.total)), lines)
}

/// The window while items are deleted as arclite's `ArchiveFileDeleterProgress` writes it: the bytes
/// written of all at the speed, the bar; its percent rounded (`al_round`).
pub fn delete_text(at: &UpdateAt<'_>, elapsed: Duration) -> (String, Vec<String>) {
    let millis = elapsed.as_millis();
    let speed = if millis == 0 {
        0
    } else {
        u64::try_from(u128::from(at.done) * 1000 / millis).unwrap_or(u64::MAX)
    };
    let rounded = if at.total == 0 {
        0
    } else {
        u64::try_from(
            (u128::from(at.done) * 200 + u128::from(at.total)) / (2 * u128::from(at.total)),
        )
        .unwrap_or(100)
        .min(100)
    };
    let lines = vec![
        format!(
            "{:>7} / {} @ {:>9}",
            data_size(at.done, &SIZES),
            data_size(at.total, &SIZES),
            data_size(speed, &SPEEDS)
        ),
        bar(rounded),
    ];
    (format!("{{{rounded}%}} Updating archive..."), lines)
}

#[cfg(test)]
#[path = "update_progress_tests.rs"]
mod tests;
