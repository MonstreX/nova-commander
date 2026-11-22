//! A plugin's log (ADR-0016 §8): `settings\Logs\<plugin>.log`, each line after the local time it was
//! written. What a plugin would print goes here: its stdout is the program's screen. A log past its size is
//! moved to `<plugin>.old.log` (the one before it gone) and begun anew. Written on the plugin's thread; a log
//! that cannot be written loses the line, the plugin goes on.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::platform;

/// The size at which a log is begun anew.
const MOST: u64 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLog {
    path: PathBuf,
}

impl PluginLog {
    /// The log of plugin `name` in the folder `logs`.
    pub fn new(logs: &Path, name: &str) -> Self {
        Self {
            path: logs.join(format!("{name}.log")),
        }
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `text` in the log, a line of the log for each of its lines.
    pub fn write(&self, text: &str) {
        let _ = self.append(text, &stamp(SystemTime::now()));
    }

    fn append(&self, text: &str, stamp: &str) -> io::Result<()> {
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder)?;
        }
        if fs::metadata(&self.path).is_ok_and(|meta| meta.len() >= MOST) {
            // An old log that cannot be replaced (open elsewhere): this one goes on growing.
            let _ = fs::rename(&self.path, self.path.with_extension("old.log"));
        }
        let mut lines = String::new();
        let mut written = text.lines().peekable();
        if written.peek().is_none() {
            lines.push_str(stamp);
            lines.push('\n');
        }
        for line in written {
            lines.push_str(stamp);
            lines.push(' ');
            lines.push_str(line);
            lines.push('\n');
        }
        // One write: lines of two threads never mix within a call.
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(lines.as_bytes())
    }
}

/// The local time as `2026-11-12 09:05:03`, empty when the system cannot tell it.
fn stamp(time: SystemTime) -> String {
    platform::locale::local_time(time).map_or_else(String::new, |at| {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            at.year, at.month, at.day, at.hour, at.minute, at.second
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_line_is_written_after_the_time() {
        let temp = tempfile::tempdir().unwrap();
        let log = PluginLog::new(&temp.path().join("Logs"), "probe");
        assert_eq!(log.path(), temp.path().join("Logs").join("probe.log"));
        log.append("one\r\ntwo", "T1").unwrap();
        log.append("", "T2").unwrap();
        log.append("three\n", "T3").unwrap();
        let written = fs::read_to_string(log.path()).unwrap();
        assert_eq!(
            written, "T1 one\nT1 two\nT2\nT3 three\n",
            "the folder made, lines added, an empty text a line of its own"
        );
    }

    #[test]
    fn a_full_log_is_moved_aside_and_begun_anew() {
        let temp = tempfile::tempdir().unwrap();
        let log = PluginLog::new(temp.path(), "probe");
        let full = "x".repeat(usize::try_from(MOST).unwrap());
        fs::write(log.path(), &full).unwrap();
        fs::write(temp.path().join("probe.old.log"), "older").unwrap();
        log.append("new", "T").unwrap();
        assert_eq!(fs::read_to_string(log.path()).unwrap(), "T new\n");
        assert_eq!(
            fs::read_to_string(temp.path().join("probe.old.log")).unwrap(),
            full,
            "the one before it replaced"
        );
        log.append("next", "T").unwrap();
        assert_eq!(
            fs::read_to_string(log.path()).unwrap(),
            "T new\nT next\n",
            "a short log is added to"
        );
    }

    #[test]
    fn a_log_that_cannot_be_written_loses_the_line() {
        let temp = tempfile::tempdir().unwrap();
        // A file where the folder would be.
        let blocked = temp.path().join("Logs");
        fs::write(&blocked, "").unwrap();
        let log = PluginLog::new(&blocked, "probe");
        assert!(log.append("lost", "T").is_err());
        log.write("lost too");
        assert_eq!(fs::read_to_string(&blocked).unwrap(), "");
    }

    #[test]
    fn a_log_just_short_of_its_size_is_added_to() {
        let temp = tempfile::tempdir().unwrap();
        let log = PluginLog::new(temp.path(), "probe");
        let almost = "x".repeat(usize::try_from(MOST - 1).unwrap());
        fs::write(log.path(), &almost).unwrap();
        log.append("new", "T").unwrap();
        assert_eq!(
            fs::read_to_string(log.path()).unwrap(),
            format!("{almost}T new\n")
        );
        assert!(!temp.path().join("probe.old.log").exists());
    }

    /// 2001-02-05 12:03:07 UTC: in any time zone the month and the day have one digit, the seconds 07.
    #[cfg(windows)]
    #[test]
    fn each_part_of_the_time_has_its_digits() {
        let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(981_374_587);
        let text = stamp(time);
        assert_eq!(text.len(), "2001-02-05 12:03:07".len(), "{text}");
        assert!(text.starts_with("2001-02-0"), "{text}");
        assert!(text.ends_with(":07"), "{text}");
        for (at, ch) in text.char_indices() {
            match at {
                4 | 7 => assert_eq!(ch, '-', "{text}"),
                10 => assert_eq!(ch, ' ', "{text}"),
                13 | 16 => assert_eq!(ch, ':', "{text}"),
                _ => assert!(ch.is_ascii_digit(), "{text}"),
            }
        }
    }

    #[test]
    fn the_time_is_written_in_full() {
        let text = stamp(SystemTime::now());
        if cfg!(windows) {
            assert_eq!(text.len(), "2026-11-12 09:05:03".len(), "{text}");
            assert_eq!(&text[4..5], "-");
            assert_eq!(&text[10..11], " ");
        }
    }
}
