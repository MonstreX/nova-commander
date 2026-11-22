//! Who holds a file, in the words of FAR's `OperationFailed`: the lines an error question shows.

use std::io;

use crate::platform::files::{self, HoldNote, Holders};
use crate::vfs::VfsPath;

/// The lines about who holds any of `paths`, when `error` may be their doing and somebody can be named.
pub fn held_lines(paths: &[&VfsPath], error: &io::Error) -> Vec<String> {
    let local: Vec<&std::path::Path> = paths.iter().filter_map(|path| path.as_local()).collect();
    if local.is_empty() || !files::may_be_held(error) {
        return Vec::new();
    }
    lines(&files::file_holders(&local))
}

fn lines(holders: &Holders) -> Vec<String> {
    if holders.programs.is_empty() && holders.notes.is_empty() {
        return Vec::new();
    }
    let mut lines = vec!["Object is being opened in:".to_string()];
    lines.extend(holders.programs.iter().cloned());
    if holders.more > 0 {
        lines.push(format!("and {} more processes.", holders.more));
    }
    for note in &holders.notes {
        lines.extend(note_text(*note).split('\n').map(str::to_string));
    }
    lines
}

fn note_text(note: HoldNote) -> &'static str {
    match note {
        // Two lines: the dialogs have no room for it in one.
        HoldNote::OtherSession => {
            "One or more processes are running\nin another Terminal Services session."
        }
        HoldNote::CriticalProcess => "One or more processes are critical processes.",
        HoldNote::CriticalService => "One or more services are critical services.",
        HoldNote::ThisProcess => "The file is opened by the current process.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nobody_named_means_no_lines() {
        assert!(lines(&Holders::default()).is_empty());
    }

    #[test]
    fn the_programs_the_rest_and_why_as_far_says_it() {
        let holders = Holders {
            programs: vec!["a.exe (PID 1)".to_string(), "b.exe (PID 2)".to_string()],
            more: 3,
            notes: vec![HoldNote::CriticalProcess],
        };
        assert_eq!(
            lines(&holders),
            [
                "Object is being opened in:",
                "a.exe (PID 1)",
                "b.exe (PID 2)",
                "and 3 more processes.",
                "One or more processes are critical processes.",
            ]
        );
    }

    #[test]
    fn a_long_note_comes_in_two_lines_that_fit() {
        let holders = Holders {
            notes: vec![HoldNote::OtherSession],
            ..Holders::default()
        };
        let shown = lines(&holders);
        assert_eq!(shown.len(), 3, "{shown:?}");
        assert!(
            shown.iter().all(|line| line.chars().count() <= 58),
            "{shown:?}"
        );
    }

    #[test]
    fn a_note_alone_is_shown_too() {
        let holders = Holders {
            notes: vec![HoldNote::ThisProcess],
            ..Holders::default()
        };
        assert_eq!(
            lines(&holders),
            [
                "Object is being opened in:",
                "The file is opened by the current process."
            ]
        );
    }

    #[test]
    fn an_error_nobody_can_cause_is_not_looked_into() {
        let path = VfsPath::local(r"C:\no such file here");
        assert!(held_lines(&[&path], &io::Error::from_raw_os_error(2)).is_empty());
    }
}
