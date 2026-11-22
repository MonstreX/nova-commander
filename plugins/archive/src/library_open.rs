//! An archive opened through 7-Zip's library as arclite opens it (open.cpp `Archive::open`): its file's
//! start searched ([`crate::detect`]), each format tried in turn - at its place (an archive after a stub
//! through a stream from there, past the stub's size arclite measured), a `kPreArc` format again with
//! data after it allowed - and an archive that names its one main file opened again inside it (a chain:
//! `Dmg→HFS`). Enter takes the first that opens and its deepest archive; the format menu all of them.

use std::io::{self, SeekFrom};
use std::path::Path;
use std::rc::Rc;

use nc_plugin_sdk::sevenzip::{Archive, Format, InStream, Library, Opening, flags, prop};

use crate::detect::{self, Entry, MAX_CHECK};

/// How deep a chain may go (a format whose main file opens as itself again ends there).
const DEEPEST: usize = 8;

/// One way a file opened: the formats from the file's own inwards and the archives they opened.
#[derive(Clone)]
pub struct Chain {
    pub parts: Vec<Entry>,
    /// The archive shown: the deepest; released before the ones around it (declared first: the fields
    /// drop in their order), as arclite's child holds its parent.
    pub archive: Rc<Archive>,
    /// The archives around the one shown, the file's own first: each inner one reads through them.
    pub outer: Vec<Rc<Archive>>,
}

impl Chain {
    /// The archive shown.
    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    /// arclite's `ArcChain::to_string`: the formats' names joined by arrows.
    pub fn name(&self, formats: &[Format]) -> String {
        (self.parts.iter())
            .filter_map(|part| formats.get(part.format).map(|format| format.name.as_str()))
            .collect::<Vec<_>>()
            .join("→")
    }

    /// The format of the archive shown.
    pub fn format(&self) -> Option<usize> {
        self.parts.last().map(|part| part.format)
    }
}

/// What opening is given.
pub struct Request<'a> {
    pub library: &'a Library,
    pub formats: &'a [Format],
    pub enabled: &'a [usize],
    /// The format menu's: every way, not the first only.
    pub every: bool,
}

/// The file at `path` opened every way `request` asks: none when no format opens it. A stop or a
/// password refused ends it (`ErrorKind::Interrupted`).
pub fn open(request: &Request, path: &Path, asker: &mut dyn Opening) -> io::Result<Vec<Chain>> {
    let stream = InStream::file(path)?;
    let size = std::fs::metadata(path)?.len();
    let level = Level {
        stream,
        size,
        name: path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        path: Some(path),
        parent: None,
    };
    let mut chains = Vec::new();
    open_level(request, level, asker, &mut chains)?;
    if !request.every && chains.len() > 1 {
        chains.drain(..chains.len() - 1);
    }
    Ok(chains)
}

/// A stream to open and what is known of it.
struct Level<'a> {
    stream: InStream,
    size: u64,
    /// Its file's name (an inner one's: its item's), for its extension.
    name: String,
    /// The archive's file (none inside another: no volumes).
    path: Option<&'a Path>,
    parent: Option<Chain>,
}

fn open_level(
    request: &Request,
    level: Level,
    asker: &mut dyn Opening,
    chains: &mut Vec<Chain>,
) -> io::Result<()> {
    let entries = detected(request, &level)?;
    let mut skip_header = 0u64;
    let mut first = true;
    for (at, entry) in entries.iter().enumerate() {
        let next_later = entries.get(at + 1).is_some_and(|next| next.offset > 0);
        let tried = try_entry(
            request,
            &level,
            *entry,
            (first, next_later),
            &mut skip_header,
            asker,
        )?;
        first = false;
        let Some((archive, tail)) = tried else {
            continue;
        };
        let physical = archive
            .archive_property(prop::PHYSICAL_SIZE)
            .number()
            .unwrap_or(0);
        let chain = chained(level.parent.as_ref(), *entry, archive);
        chains.push(chain.clone());
        inner(request, &chain, asker, chains)?;
        if !request.every && !tail {
            break;
        }
        // arclite's archive size from its place on (`arc_info.set_size`) or what it measured.
        skip_header = entry.offset + level.size.saturating_sub(entry.offset).min(physical);
    }
    Ok(())
}

/// The formats to try the stream as, in arclite's order.
fn detected(request: &Request, level: &Level) -> io::Result<Vec<Entry>> {
    let mut buffer = vec![0u8; MAX_CHECK];
    level.stream.seek(SeekFrom::Start(0))?;
    let read = level.stream.read(&mut buffer)?;
    buffer.truncate(read);
    let extension = Path::new(&level.name)
        .extension()
        .map_or_else(String::new, |ext| format!(".{}", ext.to_string_lossy()));
    let is_arc = |format: usize, data: &[u8]| request.library.is_arc(format, data);
    let formats = detect::Formats {
        all: request.formats,
        enabled: request.enabled,
        is_arc: &is_arc,
    };
    let stream = &level.stream;
    let mut tail = || {
        let mut last = vec![0u8; 16 * 1024];
        stream.seek(SeekFrom::End(-16 * 1024)).ok()?;
        let read = stream.read(&mut last).ok()?;
        last.truncate(read);
        Some(last)
    };
    Ok(detect::detect(
        &formats,
        &buffer,
        read < MAX_CHECK,
        &extension,
        &mut tail,
    ))
}

/// One format tried (arclite's loop body): at the start, again with a tail allowed for a `kPreArc`
/// format the first time, the stub's size measured when the next try is further in; at a later place
/// through a stream from there, when past what was measured. The archive and whether it has a tail.
fn try_entry(
    request: &Request,
    level: &Level,
    entry: Entry,
    (first, next_later): (bool, bool),
    skip_header: &mut u64,
    asker: &mut dyn Opening,
) -> io::Result<Option<(Archive, bool)>> {
    let library = request.library;
    level.stream.seek(SeekFrom::Start(entry.offset))?;
    if entry.offset == 0 {
        let opened = library.open(entry.format, &level.stream, level.path, false, asker)?;
        if opened.is_some() || !first {
            return Ok(opened.map(|archive| (archive, false)));
        }
        let pre_arc =
            (request.formats.get(entry.format)).is_some_and(|it| it.flags & flags::PRE_ARC != 0);
        if pre_arc {
            level.stream.seek(SeekFrom::Start(0))?;
            if let Some(archive) =
                library.open(entry.format, &level.stream, level.path, true, asker)?
            {
                return Ok(Some((archive, true)));
            }
        }
        if next_later {
            *skip_header = (library.physical_size(entry.format, &level.stream, MAX_CHECK as u64))
                .filter(|size| *size < level.size)
                .unwrap_or(0);
        }
        return Ok(None);
    }
    if entry.offset < *skip_header {
        return Ok(None);
    }
    let from = level.stream.from_offset(entry.offset)?;
    Ok(library
        .open(entry.format, &from, level.path, false, asker)?
        .map(|archive| (archive, false)))
}

/// `parent` with one more archive inside.
fn chained(parent: Option<&Chain>, entry: Entry, archive: Archive) -> Chain {
    let (mut parts, mut outer) = (Vec::new(), Vec::new());
    if let Some(parent) = parent {
        parts.clone_from(&parent.parts);
        outer.clone_from(&parent.outer);
        outer.push(Rc::clone(&parent.archive));
    }
    parts.push(entry);
    Chain {
        parts,
        archive: Rc::new(archive),
        outer,
    }
}

/// arclite's recursion: the archive's main file, when it names one whose stream seeks, opened in turn.
fn inner(
    request: &Request,
    chain: &Chain,
    asker: &mut dyn Opening,
    chains: &mut Vec<Chain>,
) -> io::Result<()> {
    if chain.parts.len() >= DEEPEST {
        return Ok(());
    }
    let archive = chain.archive();
    let Some(main) = archive.main_file() else {
        return Ok(());
    };
    let Some(stream) = archive.stream(main) else {
        return Ok(());
    };
    let path = archive.property(main, prop::PATH);
    let name = (path.text())
        .and_then(|path| path.rsplit(['/', '\\']).next())
        .unwrap_or("")
        .to_string();
    let level = Level {
        stream,
        size: archive.property(main, prop::SIZE).number().unwrap_or(0),
        name,
        path: None,
        parent: Some(chain.clone()),
    };
    open_level(request, level, asker, chains)
}

#[cfg(test)]
#[path = "library_open_tests.rs"]
mod tests;
