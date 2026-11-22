//! Extraction through 7-Zip's library (arclite's `ArchiveExtractor` with its `FileWriteCache`): the files
//! asked for given to the library in one pass in the archive's order (a solid block read once), each
//! asked about as it comes (overwrite, skip, rename, append), made, written as its data comes, its
//! attributes and times set once it is whole; the library's outcome of each in arclite's words
//! (`SetOperationResult`), the password asked when the library asks it (a wrong one forgotten); an
//! "anti" item deletes its file. A file whose making or writing failed is not left; one whose data
//! failed is kept with what was written, as arclite keeps it.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

use nc_plugin_sdk::sevenzip::{AskMode, Extracting, Outcome, Prop, prop};

use super::{Abort, Extraction, Fault, Overwrite, Tried, auto_rename, create, set_attributes};

/// arclite's words for an item's outcome (`MSG_ERROR_EXTRACT_*`), told apart for an encrypted archive;
/// none when it is good. A data or CRC error forgets the password (arclite's `m_password.clear()`).
pub(crate) fn outcome_words(outcome: Outcome, encrypted: bool) -> Option<&'static str> {
    Some(match outcome {
        Outcome::Ok | Outcome::DataAfterEnd => return None,
        Outcome::UnsupportedMethod => "Unsupported compression method",
        Outcome::DataError if encrypted => "Wrong password or data error",
        Outcome::DataError => "Data error",
        Outcome::CrcError if encrypted => "Wrong password or CRC check failure",
        Outcome::CrcError => "CRC check failed",
        Outcome::Unavailable => "Unavailable data",
        Outcome::UnexpectedEnd => "Unexpected end of data",
        Outcome::IsNotArc => "Is not archive",
        Outcome::HeadersError => "Headers Error",
        Outcome::WrongPassword => "Wrong password",
        Outcome::Other(_) => "Unknown extraction error",
    })
}

/// The file being written.
struct Current {
    item: usize,
    target: PathBuf,
    /// None once writing it failed (the file goes when it ends).
    file: Option<File>,
}

/// The extraction answering the library.
struct Through<'a, 'b> {
    extraction: &'a mut Extraction<'b>,
    archive: Rc<nc_plugin_sdk::sevenzip::Archive>,
    /// The files asked for and where they go.
    paths: HashMap<usize, PathBuf>,
    current: Option<Current>,
    /// The path of the item at hand, for its outcome.
    at: Option<PathBuf>,
    /// The destination: an item the library passes over (in a solid block) told by its path under it.
    destination: PathBuf,
}

impl Through<'_, '_> {
    /// The file `item` begins at `path`: asked about, made; true when its data is wanted.
    fn begin(&mut self, item: usize, path: &PathBuf) -> Result<bool, Abort> {
        let Some(action) = self.extraction.action(item, path)? else {
            self.extraction.skipped.insert(item);
            return Ok(false);
        };
        let index = u32::try_from(item).unwrap_or(u32::MAX);
        if self.archive.property(index, prop::IS_ANTI) == Prop::Bool(true) {
            // arclite's anti item: the file it stands for deleted.
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
            return Ok(false);
        }
        let ex = &mut *self.extraction;
        ex.asks.progress(path, ex.done, ex.total);
        let target = if action == Overwrite::Rename {
            auto_rename(path)
        } else {
            path.clone()
        };
        if matches!(
            action,
            Overwrite::Overwrite | Overwrite::OverwriteCase | Overwrite::Append
        ) {
            let _ = set_attributes(&target, 0);
        }
        let Tried::Done(file) = ex.attempt(true, || create(&target, action))? else {
            return Ok(false);
        };
        self.current = Some(Current {
            item,
            target,
            file: Some(file),
        });
        Ok(true)
    }

    /// A piece of the file's data written (a failure let be: the rest passed over, the file goes).
    fn put(&mut self, data: &[u8]) -> Result<(), Abort> {
        if self.extraction.asks.stopped() {
            return Err(Abort);
        }
        let Some(current) = self.current.as_mut() else {
            return Ok(());
        };
        let Some(file) = current.file.as_mut() else {
            return Ok(());
        };
        let target = current.target.clone();
        let written = self.extraction.attempt(true, || {
            file.write_all(data)
                .map_err(|error| Fault::system(&error, &target))
        })?;
        if let Tried::Ignored = written {
            current.file = None;
        }
        Ok(())
    }

    /// The library's progress shown (arclite's `update_extract_completed`): the item at hand, the
    /// bytes it decoded of all it decodes; false when the user stopped it.
    fn shown(&mut self) -> bool {
        let ex = &mut *self.extraction;
        if let Some(at) = &self.at {
            ex.asks.progress(at, ex.done, ex.total);
        }
        !ex.asks.stopped()
    }

    /// The item's end: a whole file's attributes and times set; a failed outcome told and let be (the
    /// file at hand goes when the question is answered by Cancel).
    fn end(&mut self, outcome: Outcome) -> Result<(), Abort> {
        let target = (self.current.as_ref())
            .map(|current| current.target.clone())
            .or_else(|| self.at.clone());
        if let (Some(words), Some(target)) = (self.words(outcome), &target) {
            let fault = Fault::message(vec![
                words.to_string(),
                target.display().to_string(),
                self.extraction.opened.path.display().to_string(),
            ]);
            self.extraction
                .attempt(false, || Err::<(), _>(fault.clone()))?;
        }
        let Some(Current { item, target, file }) = self.current.take() else {
            return Ok(());
        };
        let Some(file) = file else {
            let _ = std::fs::remove_file(&target);
            return Ok(());
        };
        drop(file);
        if !self.extraction.finish(item, &target)? {
            let _ = std::fs::remove_file(&target);
        }
        Ok(())
    }

    /// The outcome's words, the password forgotten after a data or CRC error.
    fn words(&mut self, outcome: Outcome) -> Option<&'static str> {
        let encrypted = self.extraction.opened.password.is_some();
        let words = outcome_words(outcome, encrypted)?;
        if matches!(outcome, Outcome::DataError | Outcome::CrcError) {
            self.extraction.opened.password = None;
        }
        Some(words)
    }
}

impl Extracting for Through<'_, '_> {
    fn set_total(&mut self, total: u64) -> bool {
        self.extraction.total = total;
        self.shown()
    }

    fn set_completed(&mut self, done: u64) -> bool {
        self.extraction.done = done;
        self.shown()
    }

    fn item(&mut self, index: u32, mode: AskMode) -> Option<bool> {
        self.current = None;
        let item = index as usize;
        let path = self.paths.get(&item).cloned();
        // arclite's `GetStream` names every item, one only passed over too (its path from the root).
        self.at = path.clone().or_else(|| {
            let parts = &self.extraction.opened.items.get(item)?.path;
            Some(
                parts
                    .iter()
                    .fold(self.destination.clone(), |at, part| at.join(part)),
            )
        });
        let (Some(path), AskMode::Extract) = (path, mode) else {
            return Some(false);
        };
        if self.extraction.asks.stopped() {
            return None;
        }
        self.begin(item, &path).ok()
    }

    fn write(&mut self, data: &[u8]) -> bool {
        self.put(data).is_ok()
    }

    fn done(&mut self, outcome: Outcome) -> bool {
        self.end(outcome).is_ok()
    }

    fn password(&mut self) -> Option<String> {
        let opened = &mut *self.extraction.opened;
        // arclite asks while its password is empty (an empty one given is none).
        if opened.password.as_deref().is_none_or(str::is_empty) {
            opened.password = Some(self.extraction.asks.password()?);
        }
        self.extraction.opened.password.clone()
    }
}

/// `files` (their items, their paths) extracted through the library in one pass.
pub(super) fn extract_files(
    extraction: &mut Extraction,
    files: &[(usize, PathBuf)],
    destination: &std::path::Path,
) -> Result<(), Abort> {
    let Some((chain, _)) = extraction.opened.library() else {
        return Ok(());
    };
    let archive = Rc::clone(&chain.archive);
    let indices: Vec<u32> = (files.iter())
        .filter_map(|(item, _)| u32::try_from(*item).ok())
        .collect();
    let mut through = Through {
        extraction,
        archive: Rc::clone(&archive),
        paths: files.iter().cloned().collect(),
        current: None,
        at: None,
        destination: destination.to_path_buf(),
    };
    let result = archive.extract(&indices, false, &mut through);
    // A file left half written by a stop goes (arclite's cache as it is destroyed).
    if let Some(current) = through.current.take() {
        drop(current.file);
        let _ = std::fs::remove_file(&current.target);
    }
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Err(Abort),
        Err(error) => {
            let fault = Fault::system(&error, &through.extraction.opened.path);
            through
                .extraction
                .attempt(false, || Err::<(), _>(fault.clone()))?;
            Ok(())
        }
    }
}
