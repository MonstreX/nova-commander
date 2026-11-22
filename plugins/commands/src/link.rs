//! `link:` as FAR's farcmds makes links (`__proc_Link`, the program's `MkLink`: `MkHardLink`,
//! `MkSymLink`): `link:[/msg] [/n] [/s] source destination`. A drive (`E:` or `E:\`) is mounted on a
//! folder; a folder gets a junction (`/s`: a symbolic link), a file a hard link (`/s`: a symbolic link). A
//! destination ending in `\` takes the source's name (a drive's: `Disk_<letter>`); the folders on its way
//! are made. Nothing is replaced. An error is told only with `/msg`; the panels are read again unless `/n`.

use std::path::{Path, PathBuf};

use crate::text;

/// What `link:` asks for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Link {
    pub source: String,
    pub destination: String,
    /// `/msg`: errors told.
    pub tell: bool,
    /// `/n`: the panels not read again.
    pub keep_panels: bool,
    /// `/s`: symbolic links.
    pub symbolic: bool,
}

/// A link's kind (FAR's `LINK_TYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Hard,
    Junction,
    SymbolicFile,
    SymbolicFolder,
    Mount,
}

/// `link:`'s arguments read; none without a destination (its syntax shown).
pub fn parse(text: &str) -> Option<Link> {
    let mut link = Link::default();
    let mut rest = text.trim();
    loop {
        let upper = rest.to_ascii_uppercase();
        if upper.starts_with("/MSG") {
            link.tell = true;
            rest = rest[4..].trim();
        } else if upper.starts_with("/N") {
            link.keep_panels = true;
            rest = rest[2..].trim();
        } else if upper.starts_with("/S") {
            link.symbolic = true;
            rest = rest[2..].trim();
        } else {
            break;
        }
    }
    let (source, destination) = match rest.strip_prefix('"') {
        Some(quoted) => {
            let end = quoted.find('"')?;
            (&quoted[..end], quoted[end + 1..].trim())
        }
        None => {
            let first = rest.chars().next()?.len_utf8();
            let end = rest[first..].find(' ')? + first;
            (&rest[..end], rest[end + 1..].trim())
        }
    };
    let destination = match destination.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or_default(),
        None => destination,
    };
    if destination.is_empty() {
        return None;
    }
    link.source = source.to_string();
    link.destination = destination.to_string();
    Some(link)
}

/// The kind for the source `source` (whole) as `link` asks.
pub fn kind(link: &Link, source: &Path) -> Kind {
    let bytes = link.source.as_bytes();
    let drive = bytes.len() >= 2
        && bytes[1] == b':'
        && (bytes.len() == 2 || (bytes.len() == 3 && bytes[2] == b'\\'));
    if drive {
        return Kind::Mount;
    }
    match (source.is_dir(), link.symbolic) {
        (true, true) => Kind::SymbolicFolder,
        (true, false) => Kind::Junction,
        (false, true) if source.exists() => Kind::SymbolicFile,
        _ => Kind::Hard,
    }
}

/// The link's whole path: a destination ending in a separator takes the source's name (a drive's:
/// `Disk_<letter>`).
pub fn link_path(link: &Link, kind: Kind, folder: &Path) -> PathBuf {
    let destination = text::expand(&link.destination);
    let full = text::full_path(&destination, folder);
    if !destination.ends_with(['\\', '/']) {
        return full;
    }
    match kind {
        Kind::Mount => full.join(format!("Disk_{}", &link.source[..1])),
        _ => {
            let source = link.source.trim_end_matches(['\\', '/']);
            full.join(&source[text::name_start(source)..])
        }
    }
}

/// The link made: `source` whole, `path` the link's. The error when it was not.
pub fn make(kind: Kind, source: &Path, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.exists()) {
        std::fs::create_dir_all(parent)?;
    }
    match kind {
        Kind::Hard => std::fs::hard_link(source, path),
        Kind::Junction => system::junction(source, path),
        Kind::SymbolicFile => system::symlink_file(source, path),
        Kind::SymbolicFolder => system::symlink_dir(source, path),
        Kind::Mount => system::mount(source, path),
    }
}

#[cfg(windows)]
mod system {
    use std::path::Path;

    pub fn junction(source: &Path, path: &Path) -> std::io::Result<()> {
        nc_plugin_sdk::system::junction(source, path)
    }

    pub fn symlink_file(source: &Path, path: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(source, path)
    }

    pub fn symlink_dir(source: &Path, path: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(source, path)
    }

    pub fn mount(source: &Path, path: &Path) -> std::io::Result<()> {
        nc_plugin_sdk::system::mount_volume(source, path)
    }
}

#[cfg(not(windows))]
mod system {
    use std::path::Path;

    fn unsupported() -> std::io::Result<()> {
        Err(std::io::ErrorKind::Unsupported.into())
    }

    pub fn junction(_source: &Path, _path: &Path) -> std::io::Result<()> {
        unsupported()
    }

    pub fn symlink_file(source: &Path, path: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(source, path)
    }

    pub fn symlink_dir(source: &Path, path: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(source, path)
    }

    pub fn mount(_source: &Path, _path: &Path) -> std::io::Result<()> {
        unsupported()
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
