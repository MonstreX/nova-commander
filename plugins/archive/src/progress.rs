//! arclite's progress window while it extracts (`ExtractProgress`, `ProgressMonitor`): first shown half a
//! second in, then kept up twice a second - its title `{N%} Extracting files...`, the archive, the file at
//! hand, how much of how much at what speed, the bar. arclite's lines of its write cache are not there:
//! the plugin writes each piece as it comes.

use std::time::{Duration, Instant};

use crate::layout::fit;

/// arclite's `c_first_delay_div` and `c_update_delay_div`: half a second each.
const FIRST: Duration = Duration::from_millis(500);
const EVERY: Duration = Duration::from_millis(500);

/// The window's width (arclite's `c_width`).
pub const WIDTH: usize = 60;

/// arclite's suffixes of sizes (none for bytes) and of speeds.
pub const SIZES: [&str; 5] = ["", "KB", "MB", "GB", "TB"];
pub const SPEEDS: [&str; 5] = ["B/s", "KB/s", "MB/s", "GB/s", "TB/s"];

/// The progress of one extraction.
pub struct Progress {
    archive: String,
    started: Instant,
    /// The time other windows were over it, not counted for the speed (arclite's `ProgressSuspend`).
    paused: Duration,
    /// When the window is shown next.
    next: Instant,
}

impl Progress {
    pub fn new(archive: &str) -> Self {
        let started = Instant::now();
        Self {
            archive: archive.to_string(),
            started,
            paused: Duration::ZERO,
            next: started + FIRST,
        }
    }

    /// Another window was over it from `since` until now: that time not counted, the window back at the
    /// next update.
    pub fn shown_again(&mut self, since: Instant) {
        let now = Instant::now();
        self.paused += now.saturating_duration_since(since);
        self.next = now;
    }

    /// The window's title and lines when it is due, `file` at hand, `done` of `total` bytes.
    pub fn update(&mut self, file: &str, done: u64, total: u64) -> Option<(String, Vec<String>)> {
        let working = self.due()?;
        Some(text(&self.archive, file, done, total, working))
    }

    /// When the window is due: the time worked so far (other windows' time not counted).
    pub fn due(&mut self) -> Option<Duration> {
        let now = Instant::now();
        if now < self.next {
            return None;
        }
        self.next = now + EVERY;
        Some((now - self.started).saturating_sub(self.paused))
    }

    /// The archive it is for.
    pub fn archive(&self) -> &str {
        &self.archive
    }
}

/// arclite's `get_progress_bar_str`: `percent` of the window's width filled.
pub fn bar(percent: u64) -> String {
    let filled = usize::try_from(u128::from(percent) * WIDTH as u128 / 100).unwrap_or(WIDTH);
    format!(
        "{}{}",
        "\u{2588}".repeat(filled.min(WIDTH)),
        "\u{2591}".repeat(WIDTH - filled.min(WIDTH))
    )
}

/// arclite's `calc_percent`.
pub fn percent(done: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        (u128::from(done) * 100 / u128::from(total))
            .try_into()
            .unwrap_or(100)
            .min(100)
    }
}

/// The window as arclite's `do_update_ui` and `display` write it.
pub fn text(
    archive: &str,
    file: &str,
    done: u64,
    total: u64,
    elapsed: Duration,
) -> (String, Vec<String>) {
    let percent = percent(done, total);
    let millis = elapsed.as_millis();
    let speed = (u128::from(done) * 1000)
        .checked_div(millis)
        .map_or(0, |speed| u64::try_from(speed).unwrap_or(u64::MAX));
    let sizes = format!(
        "{:>7} / {} @ {:>9}",
        data_size(done, &SIZES),
        data_size(total, &SIZES),
        data_size(speed, &SPEEDS)
    );
    let lines = vec![
        fit(archive, WIDTH),
        "\u{1}".to_string(),
        fit(file, WIDTH),
        sizes,
        bar(percent),
    ];
    (format!("{{{percent}%}} Extracting files..."), lines)
}

/// arclite's `format_data_size` with `suffixes` (bytes, KB, MB, GB, TB).
pub fn data_size(value: u64, suffixes: &[&str; 5]) -> String {
    crate::panel::format_data_size(value, suffixes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_arclites() {
        let (title, lines) = text(
            r"C:\a.zip",
            r"C:\out\b.txt",
            512,
            2048,
            Duration::from_secs(2),
        );
        assert_eq!(title, "{25%} Extracting files...");
        assert_eq!(lines[0], r"C:\a.zip");
        assert_eq!(lines[1], "\u{1}", "a separator");
        assert_eq!(lines[3], "    512 / 2 KB @   256 B/s");
        assert_eq!(lines[4].chars().count(), 60);
        assert_eq!(lines[4].chars().filter(|&c| c == '\u{2588}').count(), 15);
        assert_eq!(percent(5, 0), 0);
    }

    #[test]
    fn it_comes_half_a_second_in() {
        let mut progress = Progress::new("a.zip");
        assert_eq!(progress.update("f", 0, 1), None, "not yet");
        // A real pause, not an hour back: Windows' Instant counts from the boot, a fresh machine is younger.
        let since = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        progress.shown_again(since);
        assert!(progress.paused >= Duration::from_millis(20), "not counted");
        assert!(progress.update("f", 0, 1).is_some(), "due at once");
        assert_eq!(progress.update("f", 0, 1), None, "next in half a second");
    }

    /// arclite's `calc_percent`: rounded down, never past 100, 0 of nothing; no overflow at the ends.
    #[test]
    fn the_percent_is_rounded_down_and_capped() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(0, 10), 0);
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(2, 3), 66);
        assert_eq!(percent(199, 200), 99);
        assert_eq!(percent(10, 10), 100);
        assert_eq!(percent(11, 10), 100, "capped");
        assert_eq!(percent(u64::MAX, 1), 100);
        assert_eq!(percent(u64::MAX, u64::MAX), 100);
        assert_eq!(percent(1, u64::MAX), 0);
    }

    fn bar(done: u64, total: u64) -> (usize, usize) {
        let (_, lines) = text("a", "f", done, total, Duration::from_secs(1));
        let full = lines[4].chars().filter(|&c| c == '\u{2588}').count();
        let empty = lines[4].chars().filter(|&c| c == '\u{2591}').count();
        (full, empty)
    }

    /// The bar's 60 columns filled as the percent says, rounded down: none at 0 and 1%, one at 2%, all
    /// at 100% and past it.
    #[test]
    fn the_bar_fills_its_sixty_columns() {
        assert_eq!(bar(0, 100), (0, 60));
        assert_eq!(bar(1, 100), (0, 60));
        assert_eq!(bar(2, 100), (1, 59));
        assert_eq!(bar(50, 100), (30, 30));
        assert_eq!(bar(99, 100), (59, 1));
        assert_eq!(bar(100, 100), (60, 0));
        assert_eq!(bar(150, 100), (60, 0), "never past its width");
        assert_eq!(bar(5, 0), (0, 60), "nothing to do");
    }

    /// The speed counts milliseconds (none yet: 0); the title the percent; long paths cut to 60.
    #[test]
    fn the_speed_counts_milliseconds_and_long_paths_are_cut() {
        let long = format!(r"C:\{}\a.zip", "d".repeat(80));
        let (title, lines) = text(&long, &long, 3000, 4000, Duration::from_millis(1500));
        assert_eq!(title, "{75%} Extracting files...");
        assert_eq!(lines[3], "2.93 KB / 3.91 KB @ 1.95 KB/s", "2000 B/s");
        assert_eq!(lines[0].chars().count(), 60);
        assert!(lines[0].starts_with(r"C:\ddd") && lines[0].ends_with(r"dd\a.zip"));
        assert!(lines[0].contains("..."));
        assert_eq!(lines[2], lines[0], "the file's path as the archive's");
        assert_eq!(lines.len(), 5);
        let (title, lines) = text("a", "f", 7, 7, Duration::ZERO);
        assert_eq!(title, "{100%} Extracting files...");
        assert!(lines[3].ends_with("@     0 B/s"), "{}", lines[3]);
    }

    /// The time other windows were over the progress is not counted for the speed: 10 MB in the 10 of
    /// 20 seconds it was not hidden is about 1 MB/s (977 KB/s), not half of it.
    #[test]
    fn time_under_other_windows_is_not_counted() {
        let now = Instant::now();
        let mut progress = Progress {
            archive: "a.zip".to_string(),
            started: now - Duration::from_secs(20),
            paused: Duration::ZERO,
            next: now,
        };
        progress.shown_again(now - Duration::from_secs(10));
        let (title, lines) = progress.update("f", 10_000_000, 20_000_000).unwrap();
        assert_eq!(title, "{50%} Extracting files...");
        let speed = lines[3].rsplit('@').next().unwrap_or_default().trim();
        assert!(
            speed.starts_with('9') && speed.ends_with(" KB/s"),
            "{}",
            lines[3]
        );
        assert_eq!(lines[0], "a.zip");
        assert_eq!(lines[2], "f");
    }
}
