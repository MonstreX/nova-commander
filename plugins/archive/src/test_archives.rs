//! Archives for the tests, made through 7-Zip's installed library (the `zip` crate gone with S6.3d):
//! entries of given paths, data and times written as an archive of a format, and an archive file read
//! as the panel reads it.

use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

use nc_plugin_sdk::sevenzip::{Feed, Library, Opening, Prop, UpdateItem, Updating, prop};

use crate::library_open::{Request, open};
use crate::opened::Opened;

/// The time the entries are given: 2024-01-02 03:04:05 UTC, in `FILETIME` units.
pub const TIME: u64 = 133_486_550_450_000_000;

/// One entry: its path (`/` or `\` between parts), its data (none: a folder).
pub type Entry<'a> = (&'a str, Option<&'a [u8]>);

/// The library as the plugin finds it (7-Zip's installed one).
pub fn library() -> Library {
    Library::find(Path::new("."))
}

struct Maker<'a> {
    entries: &'a [Entry<'a>],
    /// Each entry's Windows attributes (none: a folder's or a file's own).
    attributes: &'a [u32],
    out: File,
    password: Option<String>,
    sources: Vec<std::path::PathBuf>,
}

impl Updating for Maker<'_> {
    fn item(&mut self, _index: u32) -> UpdateItem {
        UpdateItem {
            new_data: true,
            new_properties: true,
            in_archive: None,
        }
    }

    fn property(&mut self, index: u32, id: u32) -> Prop {
        let (path, data) = self.entries[index as usize];
        match id {
            prop::PATH => Prop::Text(path.replace('/', "\\")),
            prop::IS_DIR => Prop::Bool(data.is_none()),
            prop::SIZE => Prop::U64(data.map_or(0, |data| data.len() as u64)),
            prop::ATTRIBUTES => Prop::U32(
                self.attributes
                    .get(index as usize)
                    .copied()
                    .unwrap_or(if data.is_none() { 0x10 } else { 0x20 }),
            ),
            prop::MODIFIED | prop::CREATED | prop::ACCESSED => Prop::Time(TIME),
            _ => Prop::Empty,
        }
    }

    fn stream(&mut self, index: u32) -> Feed {
        let Some(data) = self.entries[index as usize].1 else {
            return Feed::Nothing;
        };
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("vc-archive-test-{}-{next}.bin", std::process::id());
        let source = std::env::temp_dir().join(name);
        std::fs::write(&source, data).unwrap();
        self.sources.push(source.clone());
        Feed::File(File::open(source).unwrap())
    }

    fn done(&mut self, ok: bool) -> bool {
        ok
    }

    fn password(&mut self) -> Option<String> {
        self.password.clone()
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.out.write_all(data)
    }

    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.out.seek(to)
    }

    fn set_size(&mut self, size: u64) -> io::Result<()> {
        self.out.set_len(size)
    }
}

/// `entries` written at `path` as an archive of `format` (7-Zip's name), stored (level 0) or with the
/// format's default level, encrypted by `password` when one is given.
pub fn make(
    path: &Path,
    format: &str,
    entries: &[Entry<'_>],
    (stored, password): (bool, Option<&str>),
) {
    make_with(path, format, entries, (stored, password), &[]);
}

/// [`make`] with each entry's Windows attributes.
fn make_with(
    path: &Path,
    format: &str,
    entries: &[Entry<'_>],
    (stored, password): (bool, Option<&str>),
    attributes: &[u32],
) {
    let library = library();
    let found = (library.formats().iter())
        .position(|known| known.name.eq_ignore_ascii_case(format))
        .unwrap();
    let mut properties = Vec::new();
    if stored {
        properties.push(("x".to_string(), Prop::U32(0)));
    }
    let mut maker = Maker {
        entries,
        attributes,
        out: File::create(path).unwrap(),
        password: password.map(str::to_string),
        sources: Vec::new(),
    };
    let count = u32::try_from(entries.len()).unwrap();
    library
        .create_archive(found, count, &properties, &mut maker)
        .unwrap();
    for source in &maker.sources {
        let _ = std::fs::remove_file(source);
    }
}

/// Files of `entries` (path, data, Windows attributes) written at `path` as a zip, stored (Windows: the
/// test that reads the attributes back).
#[cfg(windows)]
pub fn make_attributed(path: &Path, entries: &[(&str, &[u8], u32)]) {
    let plain: Vec<Entry<'_>> = (entries.iter())
        .map(|(name, data, _)| (*name, Some(*data)))
        .collect();
    let attributes: Vec<u32> = entries
        .iter()
        .map(|(_, _, attributes)| *attributes)
        .collect();
    make_with(path, "zip", &plain, (true, None), &attributes);
}

/// `entries` written at `path` as a zip, stored.
pub fn zip(path: &Path, entries: &[Entry<'_>]) {
    make(path, "zip", entries, (true, None));
}

/// No password but the one given.
pub struct NoAsk(pub Option<String>);

impl Opening for NoAsk {
    fn password(&mut self) -> Option<String> {
        self.0.clone()
    }
}

/// `path` read as the panel reads it (its first way), with `password` for encrypted headers.
pub fn opened_with(path: &Path, password: Option<&str>) -> Opened {
    let library = library();
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let mut asker = NoAsk(password.map(str::to_string));
    let chain = open(&request, path, &mut asker).unwrap().remove(0);
    let name = chain.name(&formats);
    let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
    let format = chain.format().and_then(|format| formats.get(format));
    let default_name = crate::library_items::default_name(&file_name, format);
    let mode = format.and_then(crate::partition::part_mode);
    let writes = (chain.parts.len() == 1)
        .then_some(format)
        .flatten()
        .filter(|format| format.updatable)
        .map(|format| format.name.clone());
    let opened = Opened::from_library(path, chain, (name, &default_name), None).unwrap();
    let mut opened = opened.with_partitions(mode);
    opened.writes = writes;
    opened.password = password.map(str::to_string);
    opened
}

/// `path` read as the panel reads it.
pub fn opened(path: &Path) -> Opened {
    opened_with(path, None)
}

/// The library kept for the test's run (a request borrows it).
pub fn leaked() -> &'static Library {
    Box::leak(Box::new(library()))
}

/// Each item's path (`/` between parts, a folder's with `/` after it), sorted.
pub fn marked_names(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = (opened(path).items.iter())
        .map(|item| {
            let name = item.path.join("/");
            if item.is_dir {
                format!("{name}/")
            } else {
                name
            }
        })
        .collect();
    names.sort();
    names
}

/// The data of the file `name` (`/` between parts) of the archive at `path`.
pub fn data_of(path: &Path, name: &str) -> Vec<u8> {
    let mut opened = opened(path);
    let item = (opened.items.iter())
        .position(|item| item.path.join("/") == name)
        .unwrap();
    let mut buffer = vec![0; 1 << 16];
    let read = opened.read(item, 0, &mut buffer).unwrap();
    buffer.truncate(read);
    buffer
}
