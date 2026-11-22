//! The local provider's folder reading as FAR's (`enum_files`): one pass over the folder that gives each
//! entry whole (`platform::files::read_directory`); where the system has no such pass (another OS, a file
//! system without it: FAR falls back there too) `std::fs::read_dir`, without allocation sizes and change
//! times.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::platform;
use crate::platform::files::{DirectoryRecord, LinkKind};
use crate::vfs::Entry;

/// Reads a folder's entries (not ".."), handing each to `found` in the order the file system gives
/// them; stops with `Interrupted` once `cancel` is set. Entries whose metadata cannot be read are still
/// listed (size 0, no date).
pub fn read_dir(path: &Path, cancel: &AtomicBool, found: &mut dyn FnMut(Entry)) -> io::Result<()> {
    let mut position = 0;
    let one_pass = platform::files::read_directory(path, cancel, &mut |record| {
        found(entry_of(record, position));
        position += 1;
    });
    match one_pass {
        // Nothing given by a pass that failed: `std` reads the folder, or tells why it cannot.
        Err(error) if position == 0 && error.kind() != io::ErrorKind::Interrupted => {
            read_with_std(path, cancel, found)
        }
        result => result,
    }
}

/// The folder through `std::fs`: what its entries tell, no allocation sizes or change times.
fn read_with_std(path: &Path, cancel: &AtomicBool, found: &mut dyn FnMut(Entry)) -> io::Result<()> {
    for (position, item) in fs::read_dir(path)?.enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let item = item?;
        let name = item.file_name();
        let entry = match item.metadata() {
            Ok(metadata) => entry_of(
                DirectoryRecord {
                    attributes: platform::files::attributes(&item.path(), &metadata),
                    size: metadata.len(),
                    created: metadata.created().ok(),
                    accessed: metadata.accessed().ok(),
                    modified: metadata.modified().ok(),
                    name,
                    ..DirectoryRecord::default()
                },
                position,
            ),
            Err(_) => Entry {
                name: name.to_string_lossy().into_owned(),
                position,
                ..Entry::default()
            },
        };
        found(entry);
    }
    Ok(())
}

/// The entry a record shows as. As FAR: what the directory listing says of the entry itself; a link to
/// a folder carries the folder attribute and is a folder (sorted, entered and shown as one).
fn entry_of(record: DirectoryRecord, position: usize) -> Entry {
    let attributes = record.attributes;
    let is_dir = attributes.is_directory();
    Entry {
        folder_size: crate::vfs::FolderSize::Unknown,
        name: record.name.to_string_lossy().into_owned(),
        is_dir,
        size: if is_dir { 0 } else { record.size },
        allocated: record.allocated,
        modified: record.modified,
        created: record.created,
        accessed: record.accessed,
        changed: record.changed,
        attributes,
        position,
        is_parent: false,
        // A link whose kind could not be read counts as a symbolic link, as in FAR.
        link: attributes
            .is_link()
            .then(|| record.link.unwrap_or(LinkKind::Symlink)),
        link_target: None,
        real_path: None,
        texts: None,
    }
}

/// One object as its folder would list it (no allocation size): what a job checks again before it acts
/// on what the panel showed earlier.
pub fn stat(path: &Path) -> io::Result<Entry> {
    let metadata = fs::symlink_metadata(path)?;
    let attributes = platform::files::attributes(path, &metadata);
    let is_dir = attributes.is_directory();
    let link = attributes.is_link().then(|| {
        platform::files::reparse_tag(path).map_or(LinkKind::Symlink, LinkKind::from_reparse_tag)
    });
    Ok(Entry {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        is_dir,
        size: if is_dir { 0 } else { metadata.len() },
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
        changed: platform::files::change_time(path, false),
        attributes,
        link,
        ..Entry::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::{SortMode, SortOrder, sort_entries};

    fn list_directory(path: &Path) -> io::Result<Vec<Entry>> {
        let mut entries = Vec::new();
        read_dir(path, &AtomicBool::new(false), &mut |entry| {
            entries.push(entry)
        })?;
        Ok(entries)
    }

    fn sort(entries: &mut [Entry], mode: SortMode) {
        sort_entries(entries, &SortOrder::new(mode), |_| false);
    }

    fn file(name: &str, size: u64) -> Entry {
        Entry {
            name: name.to_string(),
            size,
            ..Entry::default()
        }
    }

    fn dir(name: &str) -> Entry {
        Entry {
            is_dir: true,
            ..file(name, 0)
        }
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    fn mklink(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
            .status;
        assert!(status.success(), "mklink {args:?}");
    }

    /// Symbolic links need Developer Mode or administrator rights (junctions need neither).
    #[test]
    fn links_to_folders_are_folders_as_in_far() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("target")).unwrap();
        std::fs::write(temp.path().join("a.txt"), b"hello").unwrap();
        mklink(temp.path(), &["/J", "junction", "target"]);
        mklink(temp.path(), &["/D", "symlink", "target"]);
        mklink(temp.path(), &["z.txt", "a.txt"]);

        let mut entries = list_directory(temp.path()).unwrap();
        sort(&mut entries, SortMode::Name);

        assert_eq!(
            names(&entries),
            vec!["junction", "symlink", "target", "a.txt", "z.txt"]
        );
        let kinds: Vec<(bool, Option<LinkKind>)> = entries
            .iter()
            .map(|entry| (entry.is_dir, entry.link))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (true, Some(LinkKind::Junction)),
                (true, Some(LinkKind::Symlink)),
                (true, None),
                (false, None),
                (false, Some(LinkKind::Symlink)),
            ]
        );
        assert!(
            entries.iter().all(|entry| entry.link_target.is_none()),
            "read lazily"
        );
    }

    #[test]
    fn a_folder_whose_name_ends_with_a_space_is_read_not_its_sibling() {
        let temp = tempfile::tempdir().unwrap();
        let exact = |name: &str| crate::platform::files::exact_path(&temp.path().join(name));
        fs::create_dir(exact("d ")).unwrap();
        fs::create_dir(exact("d")).unwrap();
        fs::write(exact("d").join("sibling.txt"), "x").unwrap();
        fs::write(exact("d ").join("own.txt"), "x").unwrap();
        let provider = crate::vfs::local::LocalProvider;
        let mut names = Vec::new();
        crate::vfs::Provider::list(
            &provider,
            &crate::vfs::VfsPath::local(temp.path().join("d ")),
            &AtomicBool::new(false),
            &mut |entry| names.push(entry.name),
        )
        .unwrap();
        assert_eq!(names, ["own.txt"]);
    }

    #[test]
    fn stat_tells_a_junction_from_the_folder_it_points_to() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        mklink(temp.path(), &["/J", "link", "target"]);
        let link = stat(&temp.path().join("link")).unwrap();
        assert!(link.is_dir && link.points_elsewhere());
        assert_eq!(link.name, "link");
        let folder = stat(&target).unwrap();
        assert!(folder.is_dir && folder.link.is_none());
        let error = stat(&temp.path().join("missing")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn lists_files_and_directories() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(temp.path().join("sub")).unwrap();

        let mut entries = list_directory(temp.path()).unwrap();
        sort(&mut entries, SortMode::Name);

        assert_eq!(names(&entries), vec!["sub", "a.txt"]);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].size, 5);
        assert!(entries[1].modified.is_some());
    }

    #[test]
    fn a_set_cancel_stops_the_reading() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.txt"), b"").unwrap();
        let mut found = 0;
        let result = read_dir(temp.path(), &AtomicBool::new(true), &mut |_| found += 1);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!(found, 0);
    }

    #[test]
    fn sorts_directories_first_then_by_name_case_insensitive() {
        let mut entries = vec![
            file("b.txt", 1),
            dir("Zeta"),
            file("A.txt", 1),
            dir("alpha"),
        ];
        sort(&mut entries, SortMode::Name);
        assert_eq!(names(&entries), vec!["alpha", "Zeta", "A.txt", "b.txt"]);
    }

    #[test]
    fn sorts_by_extension_then_name() {
        let mut entries = vec![
            file("z.rs", 1),
            file("a.toml", 1),
            file("b.rs", 1),
            file("noext", 1),
        ];
        sort(&mut entries, SortMode::Extension);
        assert_eq!(names(&entries), vec!["noext", "b.rs", "z.rs", "a.toml"]);
    }

    #[test]
    fn sorts_by_size_largest_first() {
        let mut entries = vec![file("small", 1), file("big", 100), file("mid", 10)];
        sort(&mut entries, SortMode::Size);
        assert_eq!(names(&entries), vec!["big", "mid", "small"]);
    }

    #[test]
    fn extension_ignores_leading_dot_and_directories() {
        assert_eq!(file(".gitignore", 0).extension(), "");
        assert_eq!(file("Archive.TAR.GZ", 0).extension(), "gz");
        assert_eq!(dir("folder.d").extension(), "");
    }
}
