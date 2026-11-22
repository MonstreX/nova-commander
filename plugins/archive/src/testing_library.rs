//! Testing through 7-Zip's library (arclite's `ArchiveTester`): the files asked for given to the library
//! in one pass in test mode, nothing written; its progress as the library tells it; the password when
//! it asks; the first item that fails ends the test in the tester's words (an unsupported method, a data
//! or CRC error told apart for an encrypted archive, else "Unknown extraction error").

use std::collections::HashMap;
use std::rc::Rc;

use nc_plugin_sdk::sevenzip::{AskMode, Extracting, Outcome};

use crate::extract::Fault;
use crate::opened::Opened;
use crate::testing::{TestAsks, Untested};

/// arclite's tester's words for an item's outcome; none when it is good.
pub(crate) fn tester_words(outcome: Outcome, encrypted: bool) -> Option<&'static str> {
    Some(match outcome {
        Outcome::Ok => return None,
        Outcome::UnsupportedMethod => "Unsupported compression method",
        Outcome::DataError if encrypted => "Wrong password or data error",
        Outcome::DataError => "Data error",
        Outcome::CrcError if encrypted => "Wrong password or CRC check failure",
        Outcome::CrcError => "CRC check failed",
        _ => "Unknown extraction error",
    })
}

/// The test answering the library.
struct Tester<'a> {
    opened: &'a mut Opened,
    asks: &'a mut dyn TestAsks,
    /// The files asked for, by their paths from the folder tested.
    paths: HashMap<usize, String>,
    /// The item at hand's path (one passed over: from the root).
    at: String,
    done: u64,
    total: u64,
    failed: Option<Fault>,
}

impl Extracting for Tester<'_> {
    fn set_total(&mut self, total: u64) -> bool {
        self.total = total;
        self.asks.testing(&self.at, self.done, self.total);
        !self.asks.stopped()
    }

    fn set_completed(&mut self, done: u64) -> bool {
        self.done = done;
        self.asks.testing(&self.at, self.done, self.total);
        !self.asks.stopped()
    }

    fn item(&mut self, index: u32, _mode: AskMode) -> Option<bool> {
        let item = index as usize;
        self.at = (self.paths.get(&item).cloned())
            .or_else(|| Some(self.opened.items.get(item)?.path.join("\\")))
            .unwrap_or_default();
        self.asks.testing(&self.at, self.done, self.total);
        (!self.asks.stopped()).then_some(false)
    }

    fn write(&mut self, _data: &[u8]) -> bool {
        true
    }

    fn done(&mut self, outcome: Outcome) -> bool {
        let encrypted = self.opened.password.is_some();
        let Some(words) = tester_words(outcome, encrypted) else {
            return true;
        };
        self.failed = Some(Fault::message(vec![
            words.to_string(),
            self.at.clone(),
            self.opened.path.display().to_string(),
        ]));
        false
    }

    fn password(&mut self) -> Option<String> {
        // arclite asks while its password is empty (an empty one given is none).
        if self.opened.password.as_deref().is_none_or(str::is_empty) {
            self.opened.password = Some(self.asks.password()?);
        }
        self.opened.password.clone()
    }
}

/// `files` (their items, their paths from the folder tested) tested through the library.
pub(crate) fn test_files(
    opened: &mut Opened,
    files: &[(usize, String)],
    asks: &mut dyn TestAsks,
) -> Result<(), Untested> {
    let Some((chain, _)) = opened.library() else {
        return Ok(());
    };
    let archive = Rc::clone(&chain.archive);
    let indices: Vec<u32> = (files.iter())
        .filter_map(|(item, _)| u32::try_from(*item).ok())
        .collect();
    let mut tester = Tester {
        opened,
        asks,
        paths: files.iter().cloned().collect(),
        at: String::new(),
        done: 0,
        total: 0,
        failed: None,
    };
    let result = archive.extract(&indices, true, &mut tester);
    if let Some(fault) = tester.failed {
        return Err(Untested::Failed(fault));
    }
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Err(Untested::Stopped),
        Err(error) => Err(Untested::Failed(Fault::system(&error, &tester.opened.path))),
    }
}

#[cfg(test)]
#[path = "testing_library_tests.rs"]
mod tests;
