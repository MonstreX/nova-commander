use std::collections::VecDeque;

use nc_plugin_sdk::GetMode;

use super::*;
use crate::extract_options::{ExtractSettings, Tri};
use crate::test_archives::{TIME, opened, zip};

/// The answers the test gives, in order, and what it was asked.
#[derive(Default)]
struct Scripted {
    overwrites: VecDeque<Option<(Overwrite, bool)>>,
    passwords: VecDeque<Option<String>>,
    choices: VecDeque<Choice>,
    asked: Vec<String>,
    stop: bool,
    /// Esc after this many looks.
    stop_after: Option<usize>,
    progress: Vec<(u64, u64)>,
    /// Esc pressed at this look only (counted from 1).
    esc_at: Option<usize>,
    looks: usize,
}

impl Asks for Scripted {
    fn overwrite(
        &mut self,
        path: &Path,
        source: &FileInfo,
        destination: &FileInfo,
    ) -> Option<(Overwrite, bool)> {
        let name = path.file_name().unwrap().to_string_lossy();
        (self.asked).push(format!(
            "overwrite {name} {} {}",
            source.size, destination.size
        ));
        self.overwrites.pop_front().flatten()
    }

    fn password(&mut self) -> Option<String> {
        self.asked.push("password".to_string());
        self.passwords.pop_front().flatten()
    }

    fn fault(&mut self, fault: &Fault, can_retry: bool) -> Choice {
        let first = fault.messages.first().cloned().unwrap_or_default();
        self.asked.push(format!("fault {first} retry={can_retry}"));
        self.choices.pop_front().unwrap_or(Choice::Cancel)
    }

    fn progress(&mut self, _file: &Path, done: u64, total: u64) {
        self.progress.push((done, total));
    }

    fn stopped(&mut self) -> bool {
        self.looks += 1;
        if self.esc_at == Some(self.looks) {
            return true;
        }
        match self.stop_after {
            Some(0) => true,
            Some(left) => {
                self.stop_after = Some(left - 1);
                self.stop
            }
            None => self.stop,
        }
    }
}

/// A zip made through 7-Zip's library, stored: the folder `docs` with `docs/a.txt`, `docs/sub/b.txt`
/// under a folder only implied, `top.txt`, `x:y.txt`; every time [`TIME`].
fn archive(folder: &Path) -> Opened {
    let path = folder.join("a.zip");
    zip(
        &path,
        &[
            ("docs", None),
            ("docs/a.txt", Some(b"alpha")),
            ("docs/sub/b.txt", Some(b"beta")),
            ("top.txt", Some(b"top")),
            ("x:y.txt", Some(b"colon")),
        ],
    );
    opened(&path)
}

/// The root's nodes named so.
fn nodes(opened: &Opened, names: &[&str]) -> Vec<usize> {
    names
        .iter()
        .map(|name| opened.tree.find(&format!("/{name}")).unwrap())
        .collect()
}

fn options(destination: &Path) -> ExtractOptions {
    let settings = ExtractSettings::default();
    ExtractOptions::starting(destination.to_path_buf(), GetMode::default(), &settings)
}

/// A folder with what is in it, a folder only implied, a name Windows refuses as its look-alike; the
/// files' times as the archive keeps them, the destination's folders made.
#[test]
fn folders_and_files_are_written_with_their_times() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out").join("deep");
    let chosen = nodes(&opened, &["docs", "top.txt", "x:y.txt"]);
    let mut asks = Scripted::default();
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(std::fs::read(out.join("docs/a.txt")).unwrap(), b"alpha");
    assert_eq!(std::fs::read(out.join("docs/sub/b.txt")).unwrap(), b"beta");
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
    assert_eq!(std::fs::read(out.join("x\u{2026}y.txt")).unwrap(), b"colon");
    let a = std::fs::metadata(out.join("docs/a.txt")).unwrap();
    let item = &opened.items[opened.tree.nodes[opened.tree.find("/docs/a.txt").unwrap()]
        .item
        .unwrap()];
    assert_eq!(a.modified().unwrap(), item.modified.unwrap());
    let folder = std::fs::metadata(out.join("docs")).unwrap();
    assert_eq!(
        folder.modified().ok(),
        crate::item::from_ticks(TIME),
        "the folder's own time"
    );
    assert_eq!(asks.progress.last(), Some(&(17, 17)), "every byte of all");
    let _ = nc_plugin_sdk::system::set_attributes(&out.join("docs/a.txt"), 0);
}

/// An existing file asked about: skipped, overwritten, renamed (`name.0`), added to; "Remember choice"
/// answers the next ones; Cancel stops it.
#[test]
fn an_existing_file_is_asked_about() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("top.txt"), "old!").unwrap();
    let chosen = nodes(&opened, &["top.txt"]);
    let mut asks = Scripted::default();
    asks.overwrites.push_back(Some((Overwrite::Skip, false)));
    extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"old!");
    assert_eq!(asks.asked, ["overwrite top.txt 3 4"]);
    asks.overwrites.push_back(Some((Overwrite::Append, false)));
    extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"old!top");
    asks.overwrites.push_back(Some((Overwrite::Rename, false)));
    extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(std::fs::read(out.join("top.txt.0")).unwrap(), b"top");
    asks.overwrites
        .push_back(Some((Overwrite::Overwrite, false)));
    extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
    asks.overwrites.push_back(None);
    assert_eq!(
        extract(&mut opened, &chosen, &options(&out), &mut asks),
        Err(Abort)
    );
    let both = nodes(&opened, &["docs"]);
    extract(&mut opened, &both, &options(&out), &mut Scripted::default()).unwrap();
    let mut remembered = Scripted::default();
    remembered
        .overwrites
        .push_back(Some((Overwrite::Skip, true)));
    extract(&mut opened, &both, &options(&out), &mut remembered).unwrap();
    assert_eq!(remembered.asked.len(), 1, "asked once for both");
    let _ = nc_plugin_sdk::system::set_attributes(&out.join("docs/a.txt"), 0);
}

/// A file that cannot be made: Retry tries again, Ignore logs it, Ignore All lets the next ones be
/// unasked, "Ignore errors" never asks; Esc stops at the next file.
#[test]
fn failures_are_asked_about_or_let_be() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out");
    std::fs::create_dir_all(out.join("top.txt")).unwrap();
    std::fs::create_dir_all(out.join("x\u{2026}y.txt")).unwrap();
    let chosen = nodes(&opened, &["top.txt", "x:y.txt"]);
    let mut asks = Scripted::default();
    asks.overwrites
        .push_back(Some((Overwrite::Overwrite, true)));
    asks.choices.extend([Choice::Retry, Choice::IgnoreAll]);
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(log.len(), 2, "both kept for the log");
    let faults = asks
        .asked
        .iter()
        .filter(|line| line.starts_with("fault"))
        .count();
    assert_eq!(faults, 2, "asked, retried, then all let be");
    assert!(log[0].system.is_some(), "the system's words");
    let ignoring = ExtractOptions {
        ignore_errors: true,
        overwrite: Overwrite::Overwrite,
        ..options(&out)
    };
    let mut quiet = Scripted::default();
    assert_eq!(
        extract(&mut opened, &chosen, &ignoring, &mut quiet)
            .unwrap()
            .len(),
        2
    );
    assert!(quiet.asked.is_empty());
    let mut stopped = Scripted {
        stop: true,
        ..Scripted::default()
    };
    assert_eq!(
        extract(&mut opened, &chosen, &ignoring, &mut stopped),
        Err(Abort)
    );
    assert_eq!(options(&out).separate_dir, Tri::Undef);
}

/// The files go in the archive's order whatever the order chosen; the progress told before each file
/// and after each piece, of the chosen files' bytes.
#[test]
fn files_go_in_the_archives_order_with_their_progress() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out");
    std::fs::create_dir_all(out.join("docs/sub")).unwrap();
    for name in ["top.txt", "docs/a.txt", "docs/sub/b.txt"] {
        std::fs::write(out.join(name), "12").unwrap();
    }
    let chosen = nodes(&opened, &["top.txt", "docs"]);
    let mut asks = Scripted::default();
    asks.overwrites.extend([Some((Overwrite::Skip, false)); 3]);
    extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    let asked = ["a.txt 5 2", "b.txt 4 2", "top.txt 3 2"].map(|line| format!("overwrite {line}"));
    assert_eq!(asks.asked, asked, "each asked: no answer kept for all");
    for name in ["top.txt", "docs/a.txt", "docs/sub/b.txt"] {
        assert_eq!(
            std::fs::read(out.join(name)).unwrap(),
            b"12",
            "{name} skipped"
        );
    }
    let fresh = temp.path().join("fresh");
    let chosen = nodes(&opened, &["x:y.txt", "top.txt"]);
    let mut asks = Scripted::default();
    extract(&mut opened, &chosen, &options(&fresh), &mut asks).unwrap();
    assert_eq!(
        asks.progress.last(),
        Some(&(8, 8)),
        "the library's progress of all"
    );
}

/// A read-only file there is overwritten or added to (its attributes cleared first) and takes the
/// archive's attributes.
#[test]
fn a_read_only_file_is_overwritten_or_added_to() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out");
    let a = out.join("docs/a.txt");
    std::fs::create_dir_all(out.join("docs")).unwrap();
    std::fs::write(&a, "older").unwrap();
    nc_plugin_sdk::system::set_attributes(&a, 0x1).unwrap();
    let docs = nodes(&opened, &["docs"]);
    let overwriting = ExtractOptions {
        overwrite: Overwrite::Overwrite,
        ..options(&out)
    };
    let log = extract(&mut opened, &docs, &overwriting, &mut Scripted::default()).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(std::fs::read(&a).unwrap(), b"alpha");
    assert!(!std::fs::metadata(&a).unwrap().permissions().readonly());
    nc_plugin_sdk::system::set_attributes(&a, 0x1).unwrap();
    let appending = ExtractOptions {
        overwrite: Overwrite::Append,
        ..options(&out)
    };
    let log = extract(&mut opened, &docs, &appending, &mut Scripted::default()).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(std::fs::read(&a).unwrap(), b"alphaalpha");
    let b = std::fs::read(out.join("docs/sub/b.txt")).unwrap();
    assert_eq!(b, b"betabeta");
}

/// A file takes the attributes the archive keeps: read-only, hidden and archive set, none of them on the
/// file kept without them.
#[cfg(windows)]
#[test]
fn a_file_takes_the_attributes_the_archive_keeps() {
    use std::os::windows::fs::MetadataExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("dos.zip");
    crate::test_archives::make_attributed(
        &path,
        &[("ro.txt", b"ro", 0x23), ("rw.txt", b"rw", 0x20)],
    );
    let mut opened = opened(&path);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["ro.txt", "rw.txt"]);
    let log = extract(
        &mut opened,
        &chosen,
        &options(&out),
        &mut Scripted::default(),
    )
    .unwrap();
    assert!(log.is_empty(), "{log:?}");
    let of = |name: &str| std::fs::metadata(out.join(name)).unwrap().file_attributes() & 0x23;
    assert_eq!(of("ro.txt"), 0x23, "read-only, hidden, archive");
    assert_eq!(of("rw.txt"), 0x20);
    assert_eq!(std::fs::read(out.join("ro.txt")).unwrap(), b"ro");
    let _ = nc_plugin_sdk::system::set_attributes(&out.join("ro.txt"), 0);
}

/// Ignore lets one failure be and asks at the next; Cancel at a failure stops it.
#[test]
fn ignore_is_for_one_failure_and_cancel_stops() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out");
    std::fs::create_dir_all(out.join("top.txt")).unwrap();
    std::fs::create_dir_all(out.join("x\u{2026}y.txt")).unwrap();
    let chosen = nodes(&opened, &["top.txt", "x:y.txt"]);
    let overwriting = ExtractOptions {
        overwrite: Overwrite::Overwrite,
        ..options(&out)
    };
    let mut asks = Scripted::default();
    asks.choices.extend([Choice::Ignore, Choice::Ignore]);
    let log = extract(&mut opened, &chosen, &overwriting, &mut asks).unwrap();
    assert_eq!(log.len(), 2);
    let top = out.join("top.txt").display().to_string();
    assert_eq!(asks.asked[0], format!("fault {top} retry=true"));
    assert_eq!(asks.asked.len(), 2, "asked for each");
    let mut cancel = Scripted::default();
    cancel.choices.push_back(Choice::Cancel);
    let stopped = extract(&mut opened, &chosen, &overwriting, &mut cancel);
    assert_eq!(stopped, Err(Abort));
    assert_eq!(cancel.asked.len(), 1, "nothing after it");
}

/// Whether a file there has the very case of the name: the folder's listing tells; none there, or no
/// folder, counts as the same.
#[test]
fn the_case_of_a_name_is_read_from_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Top.txt"), "x").unwrap();
    assert!(same_case(&temp.path().join("Top.txt")));
    assert!(!same_case(&temp.path().join("top.txt")));
    assert!(!same_case(&temp.path().join("TOP.TXT")));
    assert!(same_case(&temp.path().join("other.txt")));
    assert!(same_case(&temp.path().join("none").join("a.txt")));
    let missing = create(&temp.path().join("none.txt"), Overwrite::Append);
    let fault = missing.err().unwrap();
    assert!(fault.system.is_some());
    assert!(fault.messages[0].ends_with("none.txt"));
}

/// arclite's `strict_case`: Overwrite answered for a file there in another case writes it in the
/// archive's case, and "for all" carries it to the next; an answer kept from the settings does not.
#[cfg(windows)]
#[test]
fn an_overwrite_asked_takes_the_archives_case() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = archive(temp.path());
    let out = temp.path().join("out");
    std::fs::create_dir_all(out.join("docs/sub")).unwrap();
    std::fs::write(out.join("A.TXT"), "x").unwrap();
    std::fs::write(out.join("TOP.TXT"), "old!").unwrap();
    std::fs::write(out.join("docs/A.TXT"), "old!").unwrap();
    std::fs::write(out.join("docs/sub/B.TXT"), "old!").unwrap();
    let listed = |folder: &Path| -> Vec<String> {
        let mut names: Vec<String> = (std::fs::read_dir(folder).unwrap().flatten())
            .filter(|entry| entry.path().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let kept = ExtractOptions {
        overwrite: Overwrite::Overwrite,
        ..options(&out)
    };
    let top = nodes(&opened, &["top.txt"]);
    extract(&mut opened, &top, &kept, &mut Scripted::default()).unwrap();
    assert_eq!(listed(&out), ["A.TXT", "TOP.TXT"], "not asked: as it was");
    assert_eq!(std::fs::read(out.join("TOP.TXT")).unwrap(), b"top");
    let mut asks = Scripted::default();
    asks.overwrites
        .push_back(Some((Overwrite::Overwrite, false)));
    extract(&mut opened, &top, &options(&out), &mut asks).unwrap();
    assert_eq!(listed(&out), ["A.TXT", "top.txt"]);
    let docs = nodes(&opened, &["docs"]);
    let mut all = Scripted::default();
    all.overwrites.push_back(Some((Overwrite::Overwrite, true)));
    extract(&mut opened, &docs, &options(&out), &mut all).unwrap();
    assert_eq!(all.asked.len(), 1, "for all");
    assert_eq!(listed(&out.join("docs")), ["a.txt"]);
    assert_eq!(
        listed(&out.join("docs/sub")),
        ["b.txt"],
        "carried to the next"
    );
    assert_eq!(std::fs::read(out.join("docs/sub/b.txt")).unwrap(), b"beta");
    let _ = nc_plugin_sdk::system::set_attributes(&out.join("docs/a.txt"), 0);
}

#[path = "extract_library_tests.rs"]
mod library;
