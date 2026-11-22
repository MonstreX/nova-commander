//! An archive opened on a panel (arclite's `Archive::open`): its items, their tree, the archive file's
//! own size and times (the times of a folder only implied, arclite's `arc_info`), every format read
//! through 7-Zip's library (ADR-0022).

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nc_plugin_sdk::InfoLine;

use crate::item::Item;
use crate::library_open::Chain;
use crate::tree::Tree;

/// The archive file's own times and size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileInfo {
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
}

/// One archive open.
pub struct Opened {
    pub path: PathBuf,
    /// Its formats' names as arclite writes them ("zip", "Dmg→HFS").
    pub format: String,
    pub items: Vec<Item>,
    pub tree: Tree,
    pub file: FileInfo,
    /// The password given (`arc:-p:`, or asked as its headers were read), tried for an encrypted file.
    pub password: Option<String>,
    /// The archives 7-Zip's library opened (the one shown the deepest).
    chain: Chain,
    /// arclite's info panel lines of the one shown.
    lines: Vec<InfoLine>,
    /// What the library gave of the file read, kept while it is read.
    spool: Option<crate::library_read::Spool>,
    /// The format the archive is written as (arclite's `updatable`: one archive of the file's own level
    /// of a format 7-Zip's library writes); none when it cannot be changed.
    pub writes: Option<String>,
    /// arclite's `part_mode`: `m` or `g` for a disk's MBR or GPT, its partitions at its root.
    pub part_mode: Option<char>,
    /// The partition of the disk's archive it shows (arclite's `part_idx`), named in its title.
    pub partition: Option<u32>,
}

impl Opened {
    /// The archive `chain` 7-Zip's library opened from `path`, `format` its formats' names, `default_name`
    /// for an item without a path; `password` the one its headers asked for.
    pub fn from_library(
        path: &Path,
        chain: Chain,
        (format, default_name): (String, &str),
        password: Option<String>,
    ) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let file = file_info(&metadata);
        let items = crate::library_items::items(chain.archive(), default_name, &file);
        let shown = path.to_string_lossy();
        let after_stub =
            (chain.parts.first()).map_or(file.size, |part| file.size.saturating_sub(part.offset));
        let lines = crate::props::info_lines(&shown, chain.archive(), &items, after_stub);
        Ok(Self {
            path: path.to_path_buf(),
            format,
            tree: Tree::new(&items),
            items,
            file,
            password,
            chain,
            lines,
            spool: None,
            writes: None,
            part_mode: None,
            partition: None,
        })
    }

    /// A disk's archive (`mode` its partitions' kind) with its partitions at its root renamed by their
    /// file systems ([`crate::partition::rename_roots`]).
    pub fn with_partitions(mut self, mode: Option<char>) -> Self {
        if let Some(mode) = mode {
            crate::partition::rename_roots(self.chain.archive(), &mut self.items, mode);
            self.tree = Tree::new(&self.items);
            self.part_mode = Some(mode);
        }
        self
    }

    /// The library's archives and the info lines.
    pub fn library(&self) -> Option<(&Chain, &[InfoLine])> {
        Some((&self.chain, &self.lines))
    }

    /// The archive file's name (arclite's `arc_name`).
    pub fn name(&self) -> String {
        (self.path.file_name()).map_or_else(
            || self.path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    /// Up to `buffer.len()` bytes of the file of `item` from `offset` ([`crate::library_read`]): what the
    /// library gave of the file kept while that file is read, extracted further when a read goes past
    /// it; a failure forgets it.
    pub fn read(&mut self, item: usize, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let archive = std::rc::Rc::clone(&self.chain.archive);
        let index = u32::try_from(item).map_err(|_| io::Error::from(io::ErrorKind::NotFound))?;
        let size = self.items.get(item).map_or(0, |item| item.size);
        let password = self.password.clone();
        if self
            .spool
            .as_ref()
            .is_none_or(|spool| spool.item() != index)
        {
            self.spool = Some(crate::library_read::Spool::new(index));
        }
        let Some(spool) = self.spool.as_mut() else {
            return Ok(0);
        };
        let read = spool.read(&archive, (offset, buffer), (size, password));
        if read.is_err() {
            self.spool = None;
        }
        read
    }
}

/// The archive file's size and times.
fn file_info(metadata: &std::fs::Metadata) -> FileInfo {
    FileInfo {
        size: metadata.len(),
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
    }
}
