//! An update written through 7-Zip's library (arclite's `Archive::update`, `Archive::create`,
//! `delete_files`, `create_dir` with its `ArchiveUpdater`): the plan's items given as 7-Zip numbers them
//! (each item kept by its index, one a file takes the place of with the file's data and properties,
//! the new ones after them; the ones deleted left out), each new file read from the disk as the library
//! asks (a failure to open it asked about: ignored, the library leaves it out), its progress the
//! library's, the archive's parameters set as arclite sets them; written beside the archive (what is
//! before an archive after a stub copied first) or, split, into volumes at their place.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use nc_plugin_sdk::sevenzip::{Feed, Library, Prop, UpdateItem, Updating, prop};

use crate::extract::Fault;
use crate::opened::Opened;
use crate::update::{Added, Faults, Plan, UpdateAt};
use crate::update_options::UpdateOptions;
use crate::update_volumes::{Output, Volumes};
use crate::update_write::{Stop, open_source, temp_name};

/// arclite's `MSG_ERROR_UPDATE_UNKNOWN`: the library failed to write an item.
pub const UPDATE_UNKNOWN: &str = "Unknown update error";

/// One item of the new archive, as 7-Zip numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// The archive's item kept as it is.
    Old(u32),
    /// The archive's item whose place a file takes (the plan's added one).
    Replaced(u32, usize),
    /// A new item (the plan's added one).
    New(usize),
}

/// The plan's items as 7-Zip numbers them: the archive's `count` items but the ones deleted, each kept
/// or taken by its file, then the new ones in the order walked.
pub fn entries(count: usize, plan: &Plan) -> Vec<Entry> {
    let replacing: BTreeSet<usize> = plan.replaced.values().copied().collect();
    let index = |item: usize| u32::try_from(item).unwrap_or(u32::MAX);
    let mut entries: Vec<Entry> = (0..count)
        .filter(|item| !plan.removed.contains(item))
        .map(|item| match plan.replaced.get(&item) {
            Some(&added) => Entry::Replaced(index(item), added),
            None => Entry::Old(index(item)),
        })
        .collect();
    entries.extend(
        (0..plan.added.len())
            .filter(|added| !replacing.contains(added))
            .map(Entry::New),
    );
    entries
}

/// A new item's property as arclite's `GetProperty` gives it.
pub fn added_property(added: &Added, id: u32) -> Prop {
    let time = |moment: Option<std::time::SystemTime>| {
        (moment.and_then(crate::item::to_ticks))
            .map_or(Prop::Empty, |ticks| Prop::Time(ticks.cast_unsigned()))
    };
    match id {
        prop::PATH => Prop::Text(added.path.join("\\")),
        prop::NAME => Prop::Text(added.path.last().cloned().unwrap_or_default()),
        prop::IS_DIR => Prop::Bool(added.is_dir),
        prop::SIZE => Prop::U64(added.size),
        prop::ATTRIBUTES => Prop::U32(added.attributes | if added.is_dir { 0x10 } else { 0 }),
        prop::CREATED => time(added.created),
        prop::ACCESSED => time(added.accessed),
        prop::MODIFIED => time(added.modified),
        _ => Prop::Empty,
    }
}

/// The update answering the library.
struct Through<'a, 'f> {
    entries: Vec<Entry>,
    plan: &'a Plan,
    options: &'a UpdateOptions,
    faults: &'a mut Faults<'f>,
    out: Output,
    done: u64,
    total: u64,
    written: u64,
    /// The file being read: its path, size and the progress when it was opened.
    file: PathBuf,
    file_total: u64,
    file_start: u64,
    /// The archive's path, for a failure of the library's.
    archive: PathBuf,
    failed: Option<Fault>,
}

impl Through<'_, '_> {
    fn added(&self, index: u32) -> Option<&Added> {
        match self.entries.get(index as usize)? {
            Entry::Old(_) => None,
            Entry::Replaced(_, added) | Entry::New(added) => self.plan.added.get(*added),
        }
    }

    /// The progress shown; false when the user stopped it.
    fn shown(&mut self) -> bool {
        let file_done = self
            .done
            .saturating_sub(self.file_start)
            .min(self.file_total);
        self.faults.asks.progress(&UpdateAt {
            file: &self.file,
            file_done,
            file_total: self.file_total,
            done: self.done,
            total: self.total,
            written: self.written,
        });
        !self.faults.asks.stopped()
    }
}

impl Updating for Through<'_, '_> {
    fn set_total(&mut self, total: u64) -> bool {
        self.total = total;
        self.shown()
    }

    fn set_completed(&mut self, done: u64) -> bool {
        self.done = done;
        self.shown()
    }

    fn item(&mut self, index: u32) -> UpdateItem {
        match self.entries.get(index as usize) {
            Some(Entry::Old(item)) => UpdateItem {
                new_data: false,
                new_properties: false,
                in_archive: Some(*item),
            },
            Some(Entry::Replaced(item, _)) => UpdateItem {
                new_data: true,
                new_properties: true,
                in_archive: Some(*item),
            },
            _ => UpdateItem {
                new_data: true,
                new_properties: true,
                in_archive: None,
            },
        }
    }

    fn property(&mut self, index: u32, id: u32) -> Prop {
        self.added(index)
            .map_or(Prop::Empty, |added| added_property(added, id))
    }

    fn stream(&mut self, index: u32) -> Feed {
        let Some(added) = self.added(index).cloned() else {
            return Feed::Nothing;
        };
        if added.is_dir {
            return Feed::Nothing;
        }
        self.file.clone_from(&added.source);
        self.file_total = added.size;
        self.file_start = self.done;
        if !self.shown() {
            return Feed::Stop;
        }
        let shared = self.options.open_shared;
        let opened = self.faults.attempt(|| {
            open_source(&added.source, shared).map_err(|error| Fault::system(&error, &added.source))
        });
        match opened {
            Ok(Some(file)) => Feed::File(file),
            Ok(None) => Feed::Skipped,
            Err(_) => Feed::Stop,
        }
    }

    fn done(&mut self, ok: bool) -> bool {
        if !ok {
            self.failed = Some(Fault::message(vec![
                UPDATE_UNKNOWN.to_string(),
                self.archive.display().to_string(),
            ]));
        }
        ok
    }

    fn password(&mut self) -> Option<String> {
        self.options.encrypt.then(|| self.options.password.clone())
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.out.write(data)?;
        self.written += data.len() as u64;
        Ok(())
    }

    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.out.seek(to)
    }

    fn set_size(&mut self, size: u64) -> io::Result<()> {
        self.out.set_size(size)
    }
}

/// What a writing through the library ended with: the stop, the library's failure of an item, or its
/// failure as told.
fn outcome(result: io::Result<()>, failed: Option<Fault>, out: &Path) -> Result<(), Stop> {
    if let Some(fault) = failed {
        return Err(Stop::Failed(fault));
    }
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Err(Stop::Abort),
        Err(error) => Err(Stop::Failed(Fault::system(&error, out))),
    }
}

/// The update of `opened` written into a new file beside it (arclite's `Archive::update`, its
/// parameters set first): its path; nothing of it left when it stops.
pub fn write(
    opened: &Opened,
    plan: &Plan,
    options: &UpdateOptions,
    faults: &mut Faults<'_>,
) -> Result<PathBuf, Stop> {
    write_with(opened, plan, options, faults, true)
}

/// [`write`] with no parameters set (arclite's `delete_files` and `create_dir` set none: the items kept
/// as they are, a new folder holds no data).
pub fn write_plain(
    opened: &Opened,
    plan: &Plan,
    options: &UpdateOptions,
    faults: &mut Faults<'_>,
) -> Result<PathBuf, Stop> {
    write_with(opened, plan, options, faults, false)
}

fn write_with(
    opened: &Opened,
    plan: &Plan,
    options: &UpdateOptions,
    faults: &mut Faults<'_>,
    parameters: bool,
) -> Result<PathBuf, Stop> {
    let Some((chain, _)) = opened.library() else {
        return Err(Stop::Failed(Fault::message(vec![
            "Archive modification not supported".to_string(),
        ])));
    };
    let archive = chain.archive();
    let temp = temp_name(&opened.path);
    let failed = |error: &io::Error| Stop::Failed(Fault::system(error, &temp));
    let mut file = File::create_new(&temp).map_err(|error| failed(&error))?;
    // arclite's `copy_prologue`: what is before the archive (an SFX's program) kept.
    let start = chain.parts.first().map_or(0, |part| part.offset);
    let copied = copy_prologue(&opened.path, start, &mut file);
    if let Err(error) = copied {
        drop(file);
        let _ = std::fs::remove_file(&temp);
        return Err(failed(&error));
    }
    let entries = entries(opened.items.len(), plan);
    let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    let properties = if parameters {
        crate::update_props::properties(options, false)
    } else {
        Vec::new()
    };
    let mut through = Through {
        entries,
        plan,
        options,
        faults,
        out: Output::File(file),
        done: 0,
        total: 0,
        written: start,
        file: opened.path.clone(),
        file_total: 0,
        file_start: 0,
        archive: opened.path.clone(),
        failed: None,
    };
    let result = archive.update(count, &properties, &mut through);
    let ended = outcome(result, through.failed.take(), &temp);
    drop(through);
    if ended.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    ended.map(|()| temp)
}

/// The first `length` bytes of `path` copied into `out`.
fn copy_prologue(path: &Path, length: u64, out: &mut File) -> io::Result<()> {
    if length == 0 {
        return Ok(());
    }
    let source = File::open(path)?;
    let copied = io::copy(&mut io::Read::take(source, length), out)?;
    if copied < length {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    Ok(())
}

/// The file a new archive is written into at `temp` (made new): plain, or after the SFX module `sfx`
/// makes (arclite's `SfxUpdateStream`: the module made, the archive's places counted from its end; the
/// file deleted when the module fails).
fn beside(temp: &Path, sfx: Option<&crate::sfx::Sfx>) -> io::Result<Output> {
    let Some(sfx) = sfx else {
        return File::create_new(temp).map(Output::File);
    };
    File::create_new(temp)?;
    let made = crate::sfx_module::create_module(temp, &sfx.module, &sfx.options)
        .and_then(|()| {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(temp)
        })
        .and_then(|mut file| {
            let start = file.seek(SeekFrom::End(0))?;
            Ok(Output::Shifted(file, start))
        });
    if made.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    made
}

/// Where a new archive went: written beside its place (to be put there), or into its volumes there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Made {
    Beside(PathBuf),
    InVolumes,
}

/// A new archive of `options`' format written with `plan` (arclite's `Archive::create`): beside
/// `archive`, or split into volumes at its place; nothing of it left when it stops.
pub fn write_new(
    library: &Library,
    archive: &Path,
    plan: &Plan,
    options: &UpdateOptions,
    faults: &mut Faults<'_>,
) -> Result<Made, Stop> {
    let formats = library.formats();
    let format = (formats.iter())
        .position(|format| format.name.eq_ignore_ascii_case(&options.kind.format))
        .ok_or_else(|| Stop::Failed(Fault::message(vec!["Select archive format".to_string()])))?;
    let (out, made, target) = match options.kind.volume_size {
        Some(size) => {
            let volumes = Volumes::new(archive, size)
                .map_err(|error| Stop::Failed(Fault::system(&error, archive)))?;
            (
                Output::Volumes(volumes),
                Made::InVolumes,
                archive.to_path_buf(),
            )
        }
        None => {
            let temp = temp_name(archive);
            let out = beside(&temp, options.kind.sfx.as_ref())
                .map_err(|error| Stop::Failed(Fault::system(&error, &temp)))?;
            (out, Made::Beside(temp.clone()), temp)
        }
    };
    let entries = entries(0, plan);
    let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    let properties = crate::update_props::properties(options, false);
    let mut through = Through {
        entries,
        plan,
        options,
        faults,
        out,
        done: 0,
        total: 0,
        written: 0,
        file: archive.to_path_buf(),
        file_total: 0,
        file_start: 0,
        archive: archive.to_path_buf(),
        failed: None,
    };
    let result = library.create_archive(format, count, &properties, &mut through);
    let ended = outcome(result, through.failed.take(), &target);
    if ended.is_err() {
        match through.out {
            Output::Volumes(volumes) => volumes.clean(),
            Output::File(file) | Output::Shifted(file, _) => {
                drop(file);
                let _ = std::fs::remove_file(&target);
            }
        }
    }
    ended.map(|()| made)
}

#[cfg(test)]
#[path = "update_library_tests.rs"]
mod tests;
