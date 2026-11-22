//! Testing an archive as arclite does (`Archive::test`, test.cpp's `ArchiveTester`): the files under the
//! entries asked for given to 7-Zip's library in test mode ([`crate::testing_library`]), nothing
//! written, the password asked when it asks it; the first failure ends it in arclite's words (the
//! file's path from the folder tested, the archive's path), Esc stops it. Its window: `{N%} Testing
//! archive...`, the file at hand, how much of how much at what speed, the bar.

use std::time::Duration;

use crate::extract::Fault;
use crate::layout::fit;
use crate::opened::Opened;
use crate::progress::{SIZES, SPEEDS, WIDTH, bar, data_size, percent};

/// arclite's `MSG_PROGRESS_TEST`.
pub const TESTING: &str = "Testing archive...";

/// arclite's `MSG_TEST_OK`.
pub const TEST_OK: &str = "Archive test complete";

/// What a test asks the user and tells.
pub trait TestAsks {
    /// The archive's password; none for Cancel.
    fn password(&mut self) -> Option<String>;
    /// The file at hand (its path from the folder tested) and the bytes read of all.
    fn testing(&mut self, file: &str, done: u64, total: u64);
    /// Whether the user stopped it (Esc, and the question after it answered so).
    fn stopped(&mut self) -> bool;
}

/// Why a test did not end well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Untested {
    /// The user stopped it (arclite's `E_ABORT`).
    Stopped,
    /// A file's data failed (arclite's `SetOperationResult` throwing).
    Failed(Fault),
}

/// arclite's `prepare_test`: the files of `node` and of the folders under it, with their paths from the
/// folder tested (`way` the folder's own path from it).
fn gather(opened: &Opened, node: usize, way: &str, files: &mut Vec<(usize, String)>) {
    let entry = &opened.tree.nodes[node];
    let path = if way.is_empty() {
        entry.name.clone()
    } else {
        format!("{way}\\{}", entry.name)
    };
    if entry.is_dir {
        for &child in &entry.children {
            gather(opened, child, &path, files);
        }
    } else if let Some(item) = entry.item {
        files.push((item, path));
    }
}

/// The entries `nodes` of one folder tested (arclite's `Archive::test`).
pub fn test(opened: &mut Opened, nodes: &[usize], asks: &mut dyn TestAsks) -> Result<(), Untested> {
    let mut files = Vec::new();
    for &node in nodes {
        gather(opened, node, "", &mut files);
    }
    files.sort_by_key(|(item, _)| *item);
    files.dedup_by_key(|(item, _)| *item);
    let results = open_results(opened);
    let tested = crate::testing_library::test_files(opened, &files, asks);
    with_open_results(tested, results)
}

/// arclite's `read_open_results` of the archive shown (its file's size compared when it is the file's
/// own archive: from its place on, after a stub - arclite's `arc_info.set_size(size - sig_pos)`).
fn open_results(opened: &Opened) -> crate::open_results::OpenResults {
    let Some((chain, _)) = opened.library() else {
        return crate::open_results::OpenResults::default();
    };
    let own =
        (chain.parts.len() == 1).then(|| opened.file.size.saturating_sub(chain.parts[0].offset));
    crate::open_results::read(chain.archive(), own)
}

/// arclite's `SetResults` then `Append`: the open errors and warnings first, a failure's words after
/// them; open errors or warnings alone fail the test too (arclite throws its `Error` when it holds any).
fn with_open_results(
    tested: Result<(), Untested>,
    results: crate::open_results::OpenResults,
) -> Result<(), Untested> {
    let opened_well = results.errors.is_empty() && results.warnings.is_empty();
    match tested {
        Err(Untested::Stopped) => Err(Untested::Stopped),
        Ok(()) if opened_well => Ok(()),
        Ok(()) => Err(Untested::Failed(Fault {
            messages: results.errors,
            warnings: results.warnings,
            ..Fault::default()
        })),
        Err(Untested::Failed(fault)) => {
            let mut messages = results.errors;
            messages.extend(fault.messages);
            Err(Untested::Failed(Fault {
                messages,
                warnings: results.warnings,
                ..fault
            }))
        }
    }
}

/// The window as arclite's `ArchiveTester::do_update_ui` writes it: its title, the file, the sizes at
/// the speed, the bar.
pub fn text(file: &str, done: u64, total: u64, elapsed: Duration) -> (String, Vec<String>) {
    let percent = percent(done, total);
    let millis = elapsed.as_millis();
    let speed = if millis == 0 {
        0
    } else {
        u64::try_from(u128::from(done) * 1000 / millis).unwrap_or(u64::MAX)
    };
    let sizes = format!(
        "{:>7} / {} @ {:>9}",
        data_size(done, &SIZES),
        data_size(total, &SIZES),
        data_size(speed, &SPEEDS)
    );
    let lines = vec![fit(file, WIDTH), sizes, bar(percent)];
    (format!("{{{percent}%}} {TESTING}"), lines)
}

#[cfg(test)]
#[path = "testing_tests.rs"]
mod tests;
